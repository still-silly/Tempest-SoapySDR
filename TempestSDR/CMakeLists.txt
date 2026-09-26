cmake_minimum_required(VERSION 3.22)

project(TSDRLibrary LANGUAGES C)

add_library(TSDRLibrary SHARED
    src/TSDRLibrary.c
    src/TSDRPluginLoader.c
    src/threading.c
    src/circbuff.c
    src/syncdetector.c
    src/frameratedetector.c
    src/gaussian.c
    src/extbuffer.c
    src/superbandwidth.c
    src/fft.c
    src/dsp.c
)

target_include_directories(TSDRLibrary
    PUBLIC src/include
    PRIVATE src
)

target_compile_features(TSDRLibrary PRIVATE c_std_11)
target_compile_options(TSDRLibrary PRIVATE -Wall -Wextra)

if(ANDROID)
    target_link_libraries(TSDRLibrary PRIVATE m dl)
elseif(UNIX)
    find_package(Threads REQUIRED)
    target_link_libraries(TSDRLibrary PRIVATE m dl Threads::Threads)
endif()

set_target_properties(TSDRLibrary PROPERTIES
    C_VISIBILITY_PRESET default
    POSITION_INDEPENDENT_CODE ON
)
