#include <atomic>
#include <chrono>
#include <dlfcn.h>
#include <iostream>
#include <string>
#include <thread>

#include "TSDRPlugin.h"

using get_name_fn = void (*)(char *);
using init_fn = int (*)(const char *);
using set_rate_fn = uint32_t (*)(uint32_t);
using set_freq_fn = int (*)(uint32_t);
using set_gain_fn = int (*)(float);
using stop_fn = int (*)();
using cleanup_fn = void (*)();
using error_fn = char *(*)();
using callback_fn = void (*)(float *, uint64_t, void *, int64_t);
using read_fn = int (*)(callback_fn, void *);

struct counters {
	std::atomic<uint64_t> complex_samples{0};
	std::atomic<uint64_t> callbacks{0};
	std::atomic<int64_t> dropped{0};
};

static void on_samples(float *, uint64_t items, void *ctx, int64_t dropped) {
	auto *counts = static_cast<counters *>(ctx);
	counts->complex_samples += items / 2;
	counts->callbacks++;
	counts->dropped += dropped;
}

template <typename T>
static T symbol(void *handle, const char *name) {
	return reinterpret_cast<T>(dlsym(handle, name));
}

int main(int argc, char **argv) {
	const std::string library = argc > 1 ? argv[1] :
		"TSDRPlugin_Soapy/bin/LINUX/X64/libTSDRPlugin_Soapy.so";
	const std::string params = argc > 2 ? argv[2] : "driver=sdrplay";
	const uint32_t requested_rate = argc > 3 ? static_cast<uint32_t>(std::stoul(argv[3])) : 2000000;
	const uint32_t requested_frequency = argc > 4 ? static_cast<uint32_t>(std::stoul(argv[4])) : 154000000;

	void *handle = dlopen(library.c_str(), RTLD_NOW | RTLD_LOCAL);
	if (!handle) {
		std::cerr << "dlopen failed: " << dlerror() << '\n';
		return 2;
	}

	const auto get_name = symbol<get_name_fn>(handle, "tsdrplugin_getName");
	const auto init = symbol<init_fn>(handle, "tsdrplugin_init");
	const auto set_rate = symbol<set_rate_fn>(handle, "tsdrplugin_setsamplerate");
	const auto set_freq = symbol<set_freq_fn>(handle, "tsdrplugin_setbasefreq");
	const auto set_gain = symbol<set_gain_fn>(handle, "tsdrplugin_setgain");
	const auto read = symbol<read_fn>(handle, "tsdrplugin_readasync");
	const auto stop = symbol<stop_fn>(handle, "tsdrplugin_stop");
	const auto cleanup = symbol<cleanup_fn>(handle, "tsdrplugin_cleanup");
	const auto last_error = symbol<error_fn>(handle, "tsdrplugin_getlasterrortext");

	char name[128] = {};
	get_name(name);
	std::cout << "Plugin: " << name << '\n';
	if (const int result = init(params.c_str()); result != 0) {
		std::cerr << "init failed (" << result << "): "
			<< (last_error() ? last_error() : "unknown error") << '\n';
		cleanup();
		dlclose(handle);
		return 3;
	}

	// Use a valid RSP1A rate and the first harmonic from the example monitor.
	const uint32_t actual_rate = set_rate(requested_rate);
	set_freq(requested_frequency);
	set_gain(0.5f);
	std::cout << "Configured sample rate: " << actual_rate << " S/s\n";

	counters counts;
	int read_result = 0;
	std::thread reader([&] { read_result = read(on_samples, &counts); });
	std::this_thread::sleep_for(std::chrono::seconds(2));
	stop();
	reader.join();

	std::cout << "Read result: " << read_result << '\n'
		      << "Callbacks: " << counts.callbacks.load() << '\n'
		      << "Complex samples: " << counts.complex_samples.load() << '\n'
		      << "Reported dropped samples: " << counts.dropped.load() << '\n';
	cleanup();
	dlclose(handle);
	return read_result == 0 && counts.complex_samples > 0 ? 0 : 4;
}
