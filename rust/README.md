# Rust/Slint prototype

This workspace is the native, cross-platform replacement path for the legacy
Swing UI. It currently validates these boundaries:

1. Rust owns and safely frees a `libTSDRLibrary` instance.
2. The Dell 2407WFP EDID timing is represented as data rather than UI constants.
3. Slint can display a continuously replaceable native pixel buffer.
4. A bounded capture channel drops stale UI frames rather than blocking SDR input.

The app opens on a generated grayscale test frame. Starting capture loads the
selected native plugin and replaces it with frames from `tsdr_readasync`.
On Android, the Rust layer uses `UsbManager` through JNI, with no application
Java or Kotlin code. The first press of **Start** requests permission when
needed; after approval, press **Start** again. Rust keeps the resulting
`UsbDeviceConnection` alive and passes its file descriptor to the bundled
Android RTL-SDR plugin.

## Linux development build

From the repository root:

```sh
make -C TempestSDR clean all
make -C TSDRPlugin_Soapy clean all
LD_LIBRARY_PATH="$PWD/TempestSDR/bin/LINUX/X64" cargo test --manifest-path rust/Cargo.toml
cargo run --manifest-path rust/Cargo.toml -p tempest-ui
```

Set `TSDR_LIBRARY_DIR` when using a library outside the repository's normal
`TempestSDR/bin/LINUX/X64` output directory.

## Android native core

The native library has a CMake build for Android and other CMake toolchains.
For an arm64 Android build with API 26 or newer:

```sh
cmake -S TempestSDR -B build/android-arm64 \
  -DCMAKE_TOOLCHAIN_FILE="$ANDROID_NDK_ROOT/build/cmake/android.toolchain.cmake" \
  -DANDROID_ABI=arm64-v8a \
  -DANDROID_PLATFORM=26
cmake --build build/android-arm64
```

After the Gradle wrapper has been bootstrapped, the complete arm64 debug APK is
built with:

```sh
ANDROID_SDK_ROOT=/path/to/Android/Sdk tools/build-android.sh
```

The resulting APK contains the Rust/Slint `NativeActivity`,
`libTSDRLibrary.so`, and `libTSDRPlugin_RTLSDR.so`. The prototype currently
builds only `arm64-v8a`, requires Android 8.0 (API 26) or newer, and recognizes
the common Realtek VID `0x0bda` RTL2832U product IDs. Connect the dongle through
a USB OTG adapter before starting capture.

The current UI is intentionally fixed to the supplied Dell 2407WFP timing:
1920×1200 active, 2080×1235 total, 154 MHz pixel clock, and 59.950171 Hz.
Resolution/profile selection remains a later layer and does not require a
change to the USB plugin.
