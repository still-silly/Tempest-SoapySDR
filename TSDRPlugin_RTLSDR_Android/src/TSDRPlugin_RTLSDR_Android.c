/*
 * Android RTL-SDR source plugin for TempestSDR.
 *
 * The file descriptor is supplied by Android's UsbManager through the plugin
 * parameter string ("fd=NUMBER"). The bundled librtlsdr adaptation wraps that
 * descriptor instead of trying to enumerate /dev/bus/usb.
 */
#include <errno.h>
#include <math.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <rtl-sdr.h>

#include "TSDRCodes.h"
#include "TSDRPlugin.h"

#define DEFAULT_SAMPLE_RATE 2400000U

static rtlsdr_dev_t *device;
static uint32_t requested_frequency = 400000000U;
static uint32_t requested_sample_rate = DEFAULT_SAMPLE_RATE;
static float requested_gain = 0.5f;
static atomic_int running;
static int last_error_code = TSDR_OK;
static char last_error[256];

static int fail_with(int code, const char *message) {
    last_error_code = code;
    snprintf(last_error, sizeof(last_error), "%s", message ? message : "Unknown RTL-SDR error");
    return code;
}

static void clear_error(void) {
    last_error_code = TSDR_OK;
    last_error[0] = '\0';
}

static int parse_fd(const char *parameters, int *fd) {
    if (!parameters || !fd)
        return 0;

    const char *value = strstr(parameters, "fd=");
    if (!value)
        return 0;
    value += 3;

    errno = 0;
    char *end = NULL;
    long parsed = strtol(value, &end, 10);
    if (errno != 0 || end == value || parsed < 0 || parsed > INT32_MAX)
        return 0;

    *fd = (int)parsed;
    return 1;
}

static int apply_gain(void) {
    if (!device)
        return 0;

    int count = rtlsdr_get_tuner_gains(device, NULL);
    if (count <= 0)
        return rtlsdr_set_tuner_gain_mode(device, 0);

    int *gains = malloc((size_t)count * sizeof(*gains));
    if (!gains)
        return -ENOMEM;

    count = rtlsdr_get_tuner_gains(device, gains);
    if (count <= 0) {
        free(gains);
        return rtlsdr_set_tuner_gain_mode(device, 0);
    }
    float normalized = requested_gain;
    if (normalized < 0.0f)
        normalized = 0.0f;
    if (normalized > 1.0f)
        normalized = 1.0f;
    int index = (int)lroundf(normalized * (float)(count - 1));

    int result = rtlsdr_set_tuner_gain_mode(device, 1);
    if (result == 0)
        result = rtlsdr_set_tuner_gain(device, gains[index]);
    free(gains);
    return result;
}

TSDRPLUGIN_API void __stdcall tsdrplugin_getName(char *name) {
    strcpy(name, "TSDR RTL-SDR Android Plugin");
}

TSDRPLUGIN_API int __stdcall tsdrplugin_init(const char *parameters) {
    if (device)
        tsdrplugin_cleanup();

    int fd = -1;
    if (!parse_fd(parameters, &fd))
        return fail_with(TSDR_PLUGIN_PARAMETERS_WRONG,
                         "Missing Android USB descriptor; expected fd=NUMBER");

    char open_error[256] = {0};
    int result = rtlsdr_open_android(&device, fd, open_error, sizeof(open_error));
    if (result < 0 || !device) {
        device = NULL;
        return fail_with(TSDR_CANNOT_OPEN_DEVICE,
                         open_error[0] ? open_error : "Could not open the RTL-SDR USB descriptor");
    }

    if (rtlsdr_set_sample_rate(device, requested_sample_rate) != 0) {
        tsdrplugin_cleanup();
        return fail_with(TSDR_SAMPLE_RATE_WRONG, "RTL-SDR rejected the requested sample rate");
    }
    requested_sample_rate = rtlsdr_get_sample_rate(device);

    if (rtlsdr_set_center_freq(device, requested_frequency) != 0) {
        tsdrplugin_cleanup();
        return fail_with(TSDR_CANNOT_OPEN_DEVICE, "RTL-SDR rejected the center frequency");
    }
    if (apply_gain() != 0) {
        tsdrplugin_cleanup();
        return fail_with(TSDR_CANNOT_OPEN_DEVICE, "RTL-SDR rejected the tuner gain");
    }
    if (rtlsdr_reset_buffer(device) != 0) {
        tsdrplugin_cleanup();
        return fail_with(TSDR_CANNOT_OPEN_DEVICE, "RTL-SDR could not reset its sample buffer");
    }

    clear_error();
    return TSDR_OK;
}

TSDRPLUGIN_API uint32_t __stdcall tsdrplugin_setsamplerate(uint32_t rate) {
    if (atomic_load(&running))
        return requested_sample_rate;

    requested_sample_rate = rate;
    if (device && rtlsdr_set_sample_rate(device, rate) == 0)
        requested_sample_rate = rtlsdr_get_sample_rate(device);
    return requested_sample_rate;
}

TSDRPLUGIN_API uint32_t __stdcall tsdrplugin_getsamplerate(void) {
    if (device)
        requested_sample_rate = rtlsdr_get_sample_rate(device);
    return requested_sample_rate;
}

TSDRPLUGIN_API int __stdcall tsdrplugin_setbasefreq(uint32_t frequency) {
    requested_frequency = frequency;
    if (device && rtlsdr_set_center_freq(device, frequency) != 0)
        return fail_with(TSDR_CANNOT_OPEN_DEVICE, "RTL-SDR rejected the center frequency");
    clear_error();
    return TSDR_OK;
}

TSDRPLUGIN_API int __stdcall tsdrplugin_setgain(float gain) {
    requested_gain = gain;
    if (device && apply_gain() != 0)
        return fail_with(TSDR_CANNOT_OPEN_DEVICE, "RTL-SDR rejected the tuner gain");
    clear_error();
    return TSDR_OK;
}

struct callback_context {
    tsdrplugin_readasync_function callback;
    void *context;
    float *samples;
    size_t capacity;
    int allocation_failed;
};

static void rtl_samples_ready(unsigned char *bytes, uint32_t length, void *opaque) {
    struct callback_context *callback = opaque;
    if (!atomic_load(&running) || !bytes || length == 0)
        return;

    if (callback->capacity < length) {
        float *resized = realloc(callback->samples, (size_t)length * sizeof(*resized));
        if (!resized) {
            callback->allocation_failed = 1;
            atomic_store(&running, 0);
            rtlsdr_cancel_async(device);
            return;
        }
        callback->samples = resized;
        callback->capacity = length;
    }

    for (uint32_t index = 0; index < length; ++index)
        callback->samples[index] = ((float)bytes[index] - 127.5f) / 127.5f;

    callback->callback(callback->samples, length, callback->context, 0);
}

TSDRPLUGIN_API int __stdcall tsdrplugin_readasync(
    tsdrplugin_readasync_function callback,
    void *context
) {
    if (!device || !callback)
        return fail_with(TSDR_CANNOT_OPEN_DEVICE, "RTL-SDR is not initialized");
    if (atomic_exchange(&running, 1))
        return fail_with(TSDR_ALREADY_RUNNING, "RTL-SDR capture is already running");

    struct callback_context callback_state = {
        .callback = callback,
        .context = context,
        .samples = NULL,
        .capacity = 0,
        .allocation_failed = 0,
    };

    int result = rtlsdr_read_async(device, rtl_samples_ready, &callback_state, 0, 0);
    atomic_store(&running, 0);
    free(callback_state.samples);

    if (callback_state.allocation_failed)
        return fail_with(TSDR_CANNOT_OPEN_DEVICE, "Could not allocate the RTL-SDR sample buffer");
    if (result < 0)
        return fail_with(TSDR_CANNOT_OPEN_DEVICE, "RTL-SDR asynchronous read failed");

    clear_error();
    return TSDR_OK;
}

TSDRPLUGIN_API int __stdcall tsdrplugin_stop(void) {
    if (!atomic_exchange(&running, 0))
        return TSDR_OK;
    if (device)
        rtlsdr_cancel_async(device);
    clear_error();
    return TSDR_OK;
}

TSDRPLUGIN_API char *__stdcall tsdrplugin_getlasterrortext(void) {
    return last_error_code == TSDR_OK ? NULL : last_error;
}

TSDRPLUGIN_API void __stdcall tsdrplugin_cleanup(void) {
    if (atomic_exchange(&running, 0) && device)
        rtlsdr_cancel_async(device);
    if (device) {
        rtlsdr_close(device);
        device = NULL;
    }
    clear_error();
}
