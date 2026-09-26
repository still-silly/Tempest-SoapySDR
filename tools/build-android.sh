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

ANDROID_ABIS="${ANDROID_ABIS:-arm64-v8a x86_64}"

for ANDROID_ABI in ${ANDROID_ABIS}; do
    case "${ANDROID_ABI}" in
        arm64-v8a)
            RUST_TARGET="aarch64-linux-android"
            LINKER_PREFIX="aarch64-linux-android"
            ;;
        x86_64)
            RUST_TARGET="x86_64-linux-android"
            LINKER_PREFIX="x86_64-linux-android"
            ;;
        *)
            echo "Unsupported Android ABI: ${ANDROID_ABI}" >&2
            exit 2
            ;;
    esac

    BUILD_SUFFIX="${ANDROID_ABI//-/_}"
    NATIVE_BUILD_DIR="${REPOSITORY_DIR}/build/android-${BUILD_SUFFIX}"
    RTLSDR_BUILD_DIR="${REPOSITORY_DIR}/build/android-rtlsdr-${BUILD_SUFFIX}"
    JNI_DIR="${REPOSITORY_DIR}/android/app/src/main/jniLibs/${ANDROID_ABI}"
    ANDROID_LINKER="${ANDROID_NDK_DIR}/toolchains/llvm/prebuilt/linux-x86_64/bin/${LINKER_PREFIX}26-clang"

    cmake \
        -S "${REPOSITORY_DIR}/TempestSDR" \
        -B "${NATIVE_BUILD_DIR}" \
        -DCMAKE_TOOLCHAIN_FILE="${ANDROID_NDK_DIR}/build/cmake/android.toolchain.cmake" \
        -DANDROID_ABI="${ANDROID_ABI}" \
        -DANDROID_PLATFORM=26 \
        -DCMAKE_BUILD_TYPE=Release
    cmake --build "${NATIVE_BUILD_DIR}" --parallel

    cmake \
        -S "${REPOSITORY_DIR}/TSDRPlugin_RTLSDR_Android" \
        -B "${RTLSDR_BUILD_DIR}" \
        -DCMAKE_TOOLCHAIN_FILE="${ANDROID_NDK_DIR}/build/cmake/android.toolchain.cmake" \
        -DANDROID_ABI="${ANDROID_ABI}" \
        -DANDROID_PLATFORM=26 \
        -DCMAKE_BUILD_TYPE=Release
    cmake --build "${RTLSDR_BUILD_DIR}" --parallel

    LINKER_VARIABLE="CARGO_TARGET_${RUST_TARGET^^}_LINKER"
    LINKER_VARIABLE="${LINKER_VARIABLE//-/_}"
    env \
        ANDROID_NDK="${ANDROID_NDK_DIR}" \
        TSDR_LIBRARY_DIR="${NATIVE_BUILD_DIR}" \
        "${LINKER_VARIABLE}=${ANDROID_LINKER}" \
        cargo build \
        --manifest-path "${REPOSITORY_DIR}/rust/Cargo.toml" \
        --target "${RUST_TARGET}" \
        --package tempest-ui \
        --lib \
        --release

    install -d "${JNI_DIR}"
    install -m 0644 "${NATIVE_BUILD_DIR}/libTSDRLibrary.so" "${JNI_DIR}/libTSDRLibrary.so"
    install -m 0644 \
        "${RTLSDR_BUILD_DIR}/libTSDRPlugin_RTLSDR.so" \
        "${JNI_DIR}/libTSDRPlugin_RTLSDR.so"
    install -m 0644 \
        "${REPOSITORY_DIR}/rust/target/${RUST_TARGET}/release/libtempest_ui.so" \
        "${JNI_DIR}/libtempest_ui.so"
done

"${REPOSITORY_DIR}/android/gradlew" \
    --project-dir "${REPOSITORY_DIR}/android" \
    clean assembleDebug

echo "APK: ${REPOSITORY_DIR}/android/app/build/outputs/apk/debug/app-debug.apk"
