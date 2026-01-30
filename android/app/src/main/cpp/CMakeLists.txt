cmake_minimum_required(VERSION 3.22.1)
project("emu_jni")

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Path to Rust core
set(RUST_CORE_DIR "${CMAKE_SOURCE_DIR}/../../../../../core")
set(RUST_INCLUDE_DIR "${RUST_CORE_DIR}/include")

# Determine Rust target based on Android ABI
if(${ANDROID_ABI} STREQUAL "arm64-v8a")
    set(RUST_TARGET "aarch64-linux-android")
elseif(${ANDROID_ABI} STREQUAL "armeabi-v7a")
    set(RUST_TARGET "armv7-linux-androideabi")
elseif(${ANDROID_ABI} STREQUAL "x86_64")
    set(RUST_TARGET "x86_64-linux-android")
elseif(${ANDROID_ABI} STREQUAL "x86")
    set(RUST_TARGET "i686-linux-android")
else()
    message(FATAL_ERROR "Unsupported Android ABI: ${ANDROID_ABI}")
endif()

# Path to Rust static library
set(RUST_LIB_DIR "${RUST_CORE_DIR}/target/${RUST_TARGET}/release")
set(RUST_LIB "${RUST_LIB_DIR}/libemu_core.a")
file(GLOB_RECURSE RUST_SOURCES
    "${RUST_CORE_DIR}/src/*.rs"
    "${RUST_CORE_DIR}/Cargo.toml"
    "${RUST_CORE_DIR}/Cargo.lock"
)

# Custom command to build Rust library
add_custom_command(
    OUTPUT ${RUST_LIB}
    COMMAND ${CMAKE_COMMAND} -E env
        CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=${ANDROID_TOOLCHAIN_ROOT}/bin/aarch64-linux-android${ANDROID_NATIVE_API_LEVEL}-clang
        CARGO_TARGET_ARMV7_LINUX_ANDROIDEABI_LINKER=${ANDROID_TOOLCHAIN_ROOT}/bin/armv7a-linux-androideabi${ANDROID_NATIVE_API_LEVEL}-clang
        CARGO_TARGET_X86_64_LINUX_ANDROID_LINKER=${ANDROID_TOOLCHAIN_ROOT}/bin/x86_64-linux-android${ANDROID_NATIVE_API_LEVEL}-clang
        CARGO_TARGET_I686_LINUX_ANDROID_LINKER=${ANDROID_TOOLCHAIN_ROOT}/bin/i686-linux-android${ANDROID_NATIVE_API_LEVEL}-clang
        CC_aarch64_linux_android=${ANDROID_TOOLCHAIN_ROOT}/bin/aarch64-linux-android${ANDROID_NATIVE_API_LEVEL}-clang
        CC_armv7_linux_androideabi=${ANDROID_TOOLCHAIN_ROOT}/bin/armv7a-linux-androideabi${ANDROID_NATIVE_API_LEVEL}-clang
        CC_x86_64_linux_android=${ANDROID_TOOLCHAIN_ROOT}/bin/x86_64-linux-android${ANDROID_NATIVE_API_LEVEL}-clang
        CC_i686_linux_android=${ANDROID_TOOLCHAIN_ROOT}/bin/i686-linux-android${ANDROID_NATIVE_API_LEVEL}-clang
        AR_aarch64_linux_android=${ANDROID_TOOLCHAIN_ROOT}/bin/llvm-ar
        AR_armv7_linux_androideabi=${ANDROID_TOOLCHAIN_ROOT}/bin/llvm-ar
        AR_x86_64_linux_android=${ANDROID_TOOLCHAIN_ROOT}/bin/llvm-ar
        AR_i686_linux_android=${ANDROID_TOOLCHAIN_ROOT}/bin/llvm-ar
        cargo build --manifest-path ${RUST_CORE_DIR}/Cargo.toml --target ${RUST_TARGET} --release
    WORKING_DIRECTORY ${RUST_CORE_DIR}
    DEPENDS ${RUST_SOURCES}
    COMMENT "Building Rust core for ${RUST_TARGET}"
    VERBATIM
)

# Create imported library for Rust core
add_library(emu_core STATIC IMPORTED)
set_target_properties(emu_core PROPERTIES
    IMPORTED_LOCATION ${RUST_LIB}
)
add_custom_target(rust_core DEPENDS ${RUST_LIB})

# JNI bridge library
add_library(${CMAKE_PROJECT_NAME} SHARED
    jni.cpp
)

add_dependencies(${CMAKE_PROJECT_NAME} rust_core)

target_include_directories(${CMAKE_PROJECT_NAME} PRIVATE
    ${RUST_INCLUDE_DIR}
)

target_link_libraries(${CMAKE_PROJECT_NAME}
    emu_core
    android
    log
)
