/*
#-------------------------------------------------------------------------------
# Copyright (c) 2014 Martin Marinov.
# All rights reserved. This program and the accompanying materials
# are made available under the terms of the GNU Public License v3.0
# which accompanies this distribution, and is available at
# http://www.gnu.org/licenses/gpl.html
#
# Contributors:
#     Martin Marinov - initial API and implementation
#-------------------------------------------------------------------------------
*/
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <SoapySDR/Device.h>
#include <SoapySDR/Formats.h>
#include <SoapySDR/Errors.h>

#include "TSDRPlugin.h"
#include "TSDRCodes.h"

#include <stdint.h>

#define HOW_OFTEN_TO_CALL_CALLBACK_SEC (0.06)

SoapySDRDevice *device = NULL;
SoapySDRStream *stream = NULL;
volatile int is_running = 0;

uint32_t req_freq = 400e6;
float req_gain = 1;
double req_rate = 2.0e6; // Conservative default supported by most Soapy devices

int errormsg_code;
char * errormsg;
int errormsg_size = 0;

#define RETURN_EXCEPTION(message, status) {announceexception(message, status); return status;}
#define RETURN_OK() {errormsg_code = TSDR_OK; return TSDR_OK;}

static inline void announceexception(const char * message, int status) {
	errormsg_code = status;
	if (status == TSDR_OK) return;

	const int length = strlen(message);
	if (errormsg_size == 0) {
			errormsg_size = length;
			errormsg = (char *) malloc(length+1);
		} else if (length > errormsg_size) {
			errormsg_size = length;
			errormsg = (char *) realloc((void*) errormsg, length+1);
		}
	strcpy(errormsg, message);
}

double tosoapygain(float gain, SoapySDRDevice *device) {
	SoapySDRRange range = SoapySDRDevice_getGainRange(device, SOAPY_SDR_RX, 0);
	if (gain < 0) gain = 0;
	if (gain > 1) gain = 1;
	return gain * (range.maximum - range.minimum) + range.minimum;
}

static int apply_gain(void) {
	size_t num_gains = 0;
	char **gainNames = SoapySDRDevice_listGains(device, SOAPY_SDR_RX, 0, &num_gains);
	int ret = 0;
	if (num_gains > 0)
		ret = SoapySDRDevice_setGain(device, SOAPY_SDR_RX, 0, tosoapygain(req_gain, device));
	SoapySDRStrings_clear(&gainNames, num_gains);
	return ret;
}

static void apply_bandwidth(void) {
	// TempestSDR reconstructs a wideband amplitude envelope. On SDRplay's
	// Soapy module, leaving this unset selects a 200 kHz filter even when the
	// sample rate is 8 MS/s, which can remove most of the display sideband.
	(void)SoapySDRDevice_setBandwidth(device, SOAPY_SDR_RX, 0, req_rate);
}

char * strtoken = NULL;
int pos = 0;
// this function splits a string into tokens separated by spaces. If tokens are surrounded by ' or ", the spaces inside are ignored
// it will destroy the string it is provided with!
char * nexttoken(char * input) {
    if (input != NULL) {
        strtoken = input;
        pos = 0;
    } else if (strtoken == NULL)
        return NULL;

    int quotes = 0;
    int i = pos;

    char ch;
    while((ch = strtoken[i++]) != 0) {
        switch (ch) {
        case '\'':
        case '"':
            quotes = !quotes;
            if (quotes) {
                pos++;
                break;
            }
        case ' ':
            if (!quotes) {
                strtoken[i-1] = 0;
                const int start = pos;
                pos = i;
                if (pos - start > 1)
                    return &strtoken[start];
            }
            break;
        }
    }

    if (quotes)
        return NULL;
    else {
        char * answer = strtoken;
        strtoken = NULL;
        return &answer[pos];
    }
}

EXTERNC TSDRPLUGIN_API void __stdcall tsdrplugin_getName(char * name) {
	strcpy(name, "TSDR SoapySDR Plugin");
}

EXTERNC TSDRPLUGIN_API int __stdcall tsdrplugin_init(const char * params) {
	if (device != NULL || stream != NULL)
		tsdrplugin_cleanup();

	// Pass the complete SoapySDR argument string through unchanged.
	SoapySDRKwargs args = SoapySDRKwargs_fromString(params ? params : "");

	// Create device
	device = SoapySDRDevice_make(&args);
	SoapySDRKwargs_clear(&args);

	if (device == NULL) {
		const char *error = SoapySDRDevice_lastError();
		RETURN_EXCEPTION(error ? error : "Failed to create SoapySDR device", TSDR_CANNOT_OPEN_DEVICE);
	}

	// Setup device
	int ret = SoapySDRDevice_setSampleRate(device, SOAPY_SDR_RX, 0, req_rate);
	if (ret != 0) {
		SoapySDRDevice_unmake(device);
		device = NULL;
		const char *error = SoapySDRDevice_lastError();
		RETURN_EXCEPTION(error ? error : "Failed to set sample rate", TSDR_SAMPLE_RATE_WRONG);
	}
	apply_bandwidth();

	ret = SoapySDRDevice_setFrequency(device, SOAPY_SDR_RX, 0, req_freq, NULL);
	if (ret != 0) {
		SoapySDRDevice_unmake(device);
		device = NULL;
		const char *error = SoapySDRDevice_lastError();
		RETURN_EXCEPTION(error ? error : "Failed to set frequency", TSDR_CANNOT_OPEN_DEVICE);
	}

	// TempestSDR exposes a manual normalized gain control. Disable AGC when
	// the backend supports it so devices such as the RSP1A can honor that gain.
	if (SoapySDRDevice_hasGainMode(device, SOAPY_SDR_RX, 0))
		SoapySDRDevice_setGainMode(device, SOAPY_SDR_RX, 0, false);

	if (apply_gain() != 0) {
		const char *error = SoapySDRDevice_lastError();
		SoapySDRDevice_unmake(device);
		device = NULL;
		RETURN_EXCEPTION(error ? error : "Failed to set gain", TSDR_CANNOT_OPEN_DEVICE);
	}

	RETURN_OK();
}

EXTERNC TSDRPLUGIN_API uint32_t __stdcall tsdrplugin_setsamplerate(uint32_t rate) {
	if (is_running)
		return tsdrplugin_getsamplerate();

	req_rate = rate;

	if (device != NULL) {
		const int ret = SoapySDRDevice_setSampleRate(device, SOAPY_SDR_RX, 0, req_rate);
		if (ret != 0) {
			const char *error = SoapySDRDevice_lastError();
			announceexception(error ? error : "Failed to set sample rate", TSDR_SAMPLE_RATE_WRONG);
			return 0;
		}
		req_rate = SoapySDRDevice_getSampleRate(device, SOAPY_SDR_RX, 0);
		apply_bandwidth();
	}

	return req_rate;
}

EXTERNC TSDRPLUGIN_API uint32_t __stdcall tsdrplugin_getsamplerate() {
	if (device != NULL) {
		req_rate = SoapySDRDevice_getSampleRate(device, SOAPY_SDR_RX, 0);
	}

	return req_rate;
}

EXTERNC TSDRPLUGIN_API int __stdcall tsdrplugin_setbasefreq(uint32_t freq) {
	req_freq = freq;

	if (device != NULL) {
		SoapySDRDevice_setFrequency(device, SOAPY_SDR_RX, 0, req_freq, NULL);
	}

	RETURN_OK();
}

EXTERNC TSDRPLUGIN_API int __stdcall tsdrplugin_stop(void) {
	is_running = 0;
	RETURN_OK();
}

EXTERNC TSDRPLUGIN_API int __stdcall tsdrplugin_setgain(float gain) {
	req_gain = gain;
	if (device != NULL) apply_gain();
	RETURN_OK();
}

EXTERNC TSDRPLUGIN_API int __stdcall tsdrplugin_readasync(tsdrplugin_readasync_function cb, void *ctx) {
	is_running = 1;

	// Set up the stream only after all GUI-requested rate/frequency changes have
	// been applied. Some Soapy drivers reject configuration changes after this.
	stream = SoapySDRDevice_setupStream(device, SOAPY_SDR_RX, SOAPY_SDR_CF32, NULL, 0, NULL);
	if (stream == NULL) {
		is_running = 0;
		const char *error = SoapySDRDevice_lastError();
		RETURN_EXCEPTION(error ? error : "Failed to setup stream", TSDR_CANNOT_OPEN_DEVICE);
	}

	// Activate stream
	int ret = SoapySDRDevice_activateStream(device, stream, 0, 0, 0);
	if (ret != 0) {
		const char *error = SoapySDRDevice_lastError();
		char error_msg[256];
		snprintf(error_msg, sizeof(error_msg), "%s", error ? error : "Failed to activate stream");
		SoapySDRDevice_closeStream(device, stream);
		stream = NULL;
		is_running = 0;
		RETURN_EXCEPTION(error_msg, TSDR_CANNOT_OPEN_DEVICE);
	}

	size_t buff_size = HOW_OFTEN_TO_CALL_CALLBACK_SEC * req_rate * 2;
	size_t mtu = SoapySDRDevice_getStreamMTU(device, stream);
	if (buff_size < mtu * 2) buff_size = mtu * 2;

	float *buff = (float *) malloc(sizeof(float) * buff_size);
	if (buff == NULL) {
		SoapySDRDevice_deactivateStream(device, stream, 0, 0);
		SoapySDRDevice_closeStream(device, stream);
		stream = NULL;
		is_running = 0;
		RETURN_EXCEPTION("Failed to allocate buffer", TSDR_CANNOT_OPEN_DEVICE);
	}

	void *buffs[] = { buff };
	int flags;
	long long timeNs;
	long timeoutUs = 100000; // 100ms timeout

	while(is_running) {
		size_t num_samples = 0;
		int ret = SoapySDRDevice_readStream(device, stream, buffs, buff_size / 2, &flags, &timeNs, timeoutUs);

		if (ret > 0) {
			num_samples = (size_t)ret;
			// Pass data to callback (already in correct format)
			cb(buff, num_samples * 2, ctx, 0);

		} else if (ret == SOAPY_SDR_TIMEOUT) {
			// Timeout - continue
			continue;
		} else if (ret == SOAPY_SDR_OVERFLOW) {
			// The exact loss is device-dependent. Reset TempestSDR's stream
			// synchronisation before accepting more samples.
			cb(NULL, 0, ctx, 1);
			continue;
		} else {
			// Error
			char error_msg[256];
			snprintf(error_msg, sizeof(error_msg), "Stream read error: %d", ret);
			free(buff);
			SoapySDRDevice_deactivateStream(device, stream, 0, 0);
			SoapySDRDevice_closeStream(device, stream);
			stream = NULL;
			is_running = 0;
			RETURN_EXCEPTION(error_msg, TSDR_CANNOT_OPEN_DEVICE);
		}
	}

	// Cleanup
	free(buff);
	SoapySDRDevice_deactivateStream(device, stream, 0, 0);
	SoapySDRDevice_closeStream(device, stream);
	stream = NULL;

	RETURN_OK();
}

EXTERNC TSDRPLUGIN_API char* __stdcall tsdrplugin_getlasterrortext(void) {
	if (errormsg_code == TSDR_OK)
		return NULL;
	else
		return errormsg;
}

EXTERNC TSDRPLUGIN_API void __stdcall tsdrplugin_cleanup(void) {
	if (stream != NULL) {
		SoapySDRDevice_deactivateStream(device, stream, 0, 0);
		SoapySDRDevice_closeStream(device, stream);
		stream = NULL;
	}

	if (device != NULL) {
		SoapySDRDevice_unmake(device);
		device = NULL;
	}

	is_running = 0;

	if (errormsg != NULL) {
		free(errormsg);
		errormsg = NULL;
		errormsg_size = 0;
	}
}
