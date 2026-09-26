#include <SoapySDR/Device.h>
#include <SoapySDR/Errors.h>
#include <SoapySDR/Formats.h>
#include <iostream>
#include <vector>

int main(int argc, char **argv) {
	const char *device_args = argc > 1 ? argv[1] : "driver=sdrplay";
	SoapySDRKwargs args = SoapySDRKwargs_fromString(device_args);
	SoapySDRDevice *dev = SoapySDRDevice_make(&args);
	SoapySDRKwargs_clear(&args);
	if (!dev) {
		std::cerr << "make: " << SoapySDRDevice_lastError() << '\n';
		return 1;
	}
	std::cout << "rate=" << SoapySDRDevice_getSampleRate(dev, SOAPY_SDR_RX, 0) << '\n';
	std::cout << "set rate=" << SoapySDRDevice_setSampleRate(dev, SOAPY_SDR_RX, 0, 2000000) << '\n';
	std::cout << "set freq=" << SoapySDRDevice_setFrequency(dev, SOAPY_SDR_RX, 0, 154000000, nullptr) << '\n';
	std::cout << "setup gain mode=" << SoapySDRDevice_setGainMode(dev, SOAPY_SDR_RX, 0, false) << '\n';
	std::cout << "set gain=" << SoapySDRDevice_setGain(dev, SOAPY_SDR_RX, 0, 24) << '\n';
	SoapySDRStream *stream = SoapySDRDevice_setupStream(dev, SOAPY_SDR_RX, SOAPY_SDR_CF32, nullptr, 0, nullptr);
	if (!stream) {
		std::cerr << "setup: " << SoapySDRDevice_lastError() << '\n';
		SoapySDRDevice_unmake(dev);
		return 2;
	}
	const int active = SoapySDRDevice_activateStream(dev, stream, 0, 0, 0);
	std::cout << "activate=" << active << " (" << SoapySDRDevice_lastError() << ")\n";
	if (active == 0) {
		std::vector<float> samples(2 * 4096);
		void *buffers[] = {samples.data()};
		int flags = 0;
		long long timeNs = 0;
		int n = SoapySDRDevice_readStream(dev, stream, buffers, 4096, &flags, &timeNs, 1000000);
		std::cout << "read=" << n << " flags=" << flags << '\n';
		SoapySDRDevice_deactivateStream(dev, stream, 0, 0);
	}
	SoapySDRDevice_closeStream(dev, stream);
	SoapySDRDevice_unmake(dev);
	return active == 0 ? 0 : 3;
}
