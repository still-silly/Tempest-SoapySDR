cmake_minimum_required(VERSION 3.22)

project(TSDRPlugin_RTLSDR_Android LANGUAGES C)

if(NOT ANDROID)
    message(FATAL_ERROR "TSDRPlugin_RTLSDR_Android is only intended for Android builds")
endif()

set(LIBUSB_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/vendor/libusb")
set(LIBUSB_SRC "${LIBUSB_ROOT}/libusb")
set(RTLSDR_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/vendor/librtlsdr")

add_library(usb_android STATIC
    ${LIBUSB_SRC}/core.c
    ${LIBUSB_SRC}/descriptor.c
    ${LIBUSB_SRC}/hotplug.c
    ${LIBUSB_SRC}/io.c
    ${LIBUSB_SRC}/sync.c
    ${LIBUSB_SRC}/strerror.c
    ${LIBUSB_SRC}/os/linux_usbfs.c
    ${LIBUSB_SRC}/os/events_posix.c
    ${LIBUSB_SRC}/os/threads_posix.c
)
target_include_directories(usb_android PUBLIC
    ${LIBUSB_ROOT}/android
    ${LIBUSB_SRC}
    ${LIBUSB_SRC}/os
)
target_compile_definitions(usb_android PRIVATE
    HAVE_STRUCT_TIMESPEC=1
    OS_LINUX=1
    PLATFORM_POSIX=1
    LIBUSB_TARGET_OS=Android
)
target_compile_options(usb_android PRIVATE
    -Wno-unused-parameter
    -Wno-sign-compare
)
set_target_properties(usb_android PROPERTIES POSITION_INDEPENDENT_CODE ON)

add_library(rtlsdr_android STATIC
    ${RTLSDR_ROOT}/src/librtlsdr.c
    ${RTLSDR_ROOT}/src/tuner_e4k.c
    ${RTLSDR_ROOT}/src/tuner_fc0012.c
    ${RTLSDR_ROOT}/src/tuner_fc0013.c
    ${RTLSDR_ROOT}/src/tuner_fc2580.c
    ${RTLSDR_ROOT}/src/tuner_r82xx.c
)
target_include_directories(rtlsdr_android PUBLIC
    ${RTLSDR_ROOT}/include
    ${RTLSDR_ROOT}/src
    ${LIBUSB_ROOT}/android
    ${LIBUSB_SRC}
)
target_compile_options(rtlsdr_android PRIVATE
    -Wno-unused-parameter
    -Wno-sign-compare
    -Wno-implicit-function-declaration
)
target_link_libraries(rtlsdr_android PUBLIC usb_android log)
set_target_properties(rtlsdr_android PROPERTIES POSITION_INDEPENDENT_CODE ON)

add_library(TSDRPlugin_RTLSDR SHARED src/TSDRPlugin_RTLSDR_Android.c)
target_include_directories(TSDRPlugin_RTLSDR PRIVATE
    ../TSDRPlugin_Soapy/src
    ${RTLSDR_ROOT}/include
)
target_compile_features(TSDRPlugin_RTLSDR PRIVATE c_std_11)
target_compile_options(TSDRPlugin_RTLSDR PRIVATE -Wall -Wextra)
target_link_libraries(TSDRPlugin_RTLSDR PRIVATE rtlsdr_android usb_android log android)
set_target_properties(TSDRPlugin_RTLSDR PROPERTIES
    C_VISIBILITY_PRESET hidden
    POSITION_INDEPENDENT_CODE ON
)
