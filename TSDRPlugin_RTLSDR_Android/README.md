# Android RTL-SDR plugin

This TempestSDR source plugin opens an RTL2832U device through a file
descriptor already authorized by Android's `UsbManager`. Pass the descriptor
as the plugin argument `fd=NUMBER`.

The vendored `libusb` and `librtlsdr` trees are derived from RavenSDR commit
`cd412c3e30e3bc94d88457599e4d19f377fb0dde`. RavenSDR adds
`rtlsdr_open_android`, which creates a no-discovery libusb context and wraps
the Android descriptor with `libusb_wrap_sys_device`. Their original license
and copyright files are retained in each vendor directory.

Reference: <https://github.com/JamesBurnettUSA/RavenSDR>
