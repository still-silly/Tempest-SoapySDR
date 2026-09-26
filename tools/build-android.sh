#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPOSITORY_DIR="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
ANDROID_SDK_DIR="${ANDROID_SDK_ROOT:-${ANDROID_HOME:-}}"

if [[ -z "${ANDROID_SDK_DIR}" ]]; then
    echo "Set ANDROID_SDK_ROOT or ANDROID_HOME to an Android SDK." >&2
    exit 2
fi

if [[ -n "${ANDROID_NDK_ROOT:-}" ]]; then
    ANDROID_NDK_DIR="${ANDROID_NDK_ROOT}"
else
    ANDROID_NDK_DIR="$(find "${ANDROID_SDK_DIR}/ndk" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -n 1)"
fi

if [[ ! -f "${ANDROID_NDK_DIR}/build/cmake/android.toolchain.cmake" ]]; then
    echo "Android NDK not found under ${ANDROID_NDK_DIR}." >&2
    exit 2
fi

NATIVE_BUILD_DIR="${REPOSITORY_DIR}/build/android-arm64"
RTLSDR_BUILD_DIR="${REPOSITORY_DIR}/build/android-rtlsdr-arm64"
JNI_DIR="${REPOSITORY_DIR}/android/app/src/main/jniLibs/arm64-v8a"
ANDROID_LINKER="${ANDROID_NDK_DIR}/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android26-clang"

cmake \
    -S "${REPOSITORY_DIR}/TempestSDR" \
    -B "${NATIVE_BUILD_DIR}" \
    -DCMAKE_TOOLCHAIN_FILE="${ANDROID_NDK_DIR}/build/cmake/android.toolchain.cmake" \
    -DANDROID_ABI=arm64-v8a \
    -DANDROID_PLATFORM=26 \
    -DCMAKE_BUILD_TYPE=Release
cmake --build "${NATIVE_BUILD_DIR}" --parallel

cmake \
    -S "${REPOSITORY_DIR}/TSDRPlugin_RTLSDR_Android" \
    -B "${RTLSDR_BUILD_DIR}" \
    -DCMAKE_TOOLCHAIN_FILE="${ANDROID_NDK_DIR}/build/cmake/android.toolchain.cmake" \
    -DANDROID_ABI=arm64-v8a \
    -DANDROID_PLATFORM=26 \
    -DCMAKE_BUILD_TYPE=Release
cmake --build "${RTLSDR_BUILD_DIR}" --parallel

TSDR_LIBRARY_DIR="${NATIVE_BUILD_DIR}" \
CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${ANDROID_LINKER}" \
cargo build \
    --manifest-path "${REPOSITORY_DIR}/rust/Cargo.toml" \
    --target aarch64-linux-android \
    --package tempest-ui \
    --lib \
    --release

install -d "${JNI_DIR}"
install -m 0644 "${NATIVE_BUILD_DIR}/libTSDRLibrary.so" "${JNI_DIR}/libTSDRLibrary.so"
install -m 0644 \
    "${RTLSDR_BUILD_DIR}/libTSDRPlugin_RTLSDR.so" \
    "${JNI_DIR}/libTSDRPlugin_RTLSDR.so"
install -m 0644 \
    "${REPOSITORY_DIR}/rust/target/aarch64-linux-android/release/libtempest_ui.so" \
    "${JNI_DIR}/libtempest_ui.so"

"${REPOSITORY_DIR}/android/gradlew" \
    --project-dir "${REPOSITORY_DIR}/android" \
    clean assembleDebug

echo "APK: ${REPOSITORY_DIR}/android/app/build/outputs/apk/debug/app-debug.apk"
