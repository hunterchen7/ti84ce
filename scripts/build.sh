#!/bin/bash
# Unified build script for TI-84 CE Emulator
#
# Usage: ./scripts/build.sh <platform> [OPTIONS]
#
# Platforms:
#   android         Build for Android
#   ios             Build for iOS device
#
# Options:
#   --release       Release build (default)
#   --debug         Debug build
#   --rust          Use Rust backend only (default)
#   --cemu          Use CEmu backend only
#   --both          Use both backends (runtime switching)
#   --sim           iOS: Build for Simulator
#   --install       Android: Install APK after build
#   --open          iOS: Open Xcode after build
#   --all-abis      Android: Build all ABIs (default: arm64 only)
#   --help          Show this help
#
# Examples:
#   ./scripts/build.sh android                    # Android, Rust only, Release
#   ./scripts/build.sh android --both --install   # Android, both backends, install
#   ./scripts/build.sh android --cemu             # Android, CEmu only
#   ./scripts/build.sh ios --sim                  # iOS Simulator, Rust only
#   ./scripts/build.sh ios --both                 # iOS device, both backends

set -e

# Check for platform argument
if [ $# -eq 0 ] || [ "$1" = "--help" ] || [ "$1" = "-h" ]; then
    head -28 "$0" | tail -27
    exit 0
fi

PLATFORM="$1"
shift

# Validate platform
if [ "$PLATFORM" != "android" ] && [ "$PLATFORM" != "ios" ]; then
    echo "Error: Unknown platform '$PLATFORM'"
    echo "Use 'android' or 'ios'"
    exit 1
fi

# Defaults
BUILD_CONFIG="Release"
BUILD_RUST=true
BUILD_CEMU=false
TARGET="device"      # device or simulator (iOS only)
INSTALL=false        # Android only
OPEN_XCODE=false     # iOS only
ALL_ABIS=false       # Android only

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --release)
            BUILD_CONFIG="Release"
            shift
            ;;
        --debug)
            BUILD_CONFIG="Debug"
            shift
            ;;
        --cemu)
            BUILD_RUST=false
            BUILD_CEMU=true
            shift
            ;;
        --rust)
            BUILD_RUST=true
            BUILD_CEMU=false
            shift
            ;;
        --both)
            BUILD_RUST=true
            BUILD_CEMU=true
            shift
            ;;
        --sim|--simulator)
            TARGET="simulator"
            shift
            ;;
        --device)
            TARGET="device"
            shift
            ;;
        --install)
            INSTALL=true
            shift
            ;;
        --open)
            OPEN_XCODE=true
            shift
            ;;
        --all-abis)
            ALL_ABIS=true
            shift
            ;;
        --help|-h)
            head -28 "$0" | tail -27
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage"
            exit 1
            ;;
    esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

# Determine backend description
if [ "$BUILD_RUST" = true ] && [ "$BUILD_CEMU" = true ]; then
    BACKEND_DESC="both (Rust + CEmu)"
elif [ "$BUILD_RUST" = true ]; then
    BACKEND_DESC="Rust"
else
    BACKEND_DESC="CEmu"
fi

echo "==> Build Configuration:"
echo "    Platform: $PLATFORM"
echo "    Config:   $BUILD_CONFIG"
echo "    Backend:  $BACKEND_DESC"
[ "$PLATFORM" = "ios" ] && echo "    Target:   $TARGET"
[ "$PLATFORM" = "android" ] && [ "$ALL_ABIS" = true ] && echo "    ABIs:     all"
[ "$PLATFORM" = "android" ] && [ "$ALL_ABIS" = false ] && echo "    ABIs:     arm64-v8a"
echo ""

# Check CEmu if needed
if [ "$BUILD_CEMU" = true ] && [ ! -d "cemu-ref/core" ]; then
    echo "Error: cemu-ref not found. Please clone CEmu first:"
    echo "  git clone https://github.com/CE-Programming/CEmu.git cemu-ref"
    exit 1
fi

#------------------------------------------------------------------------------
# Android Build
#------------------------------------------------------------------------------
build_android() {
    if [ "$BUILD_RUST" = true ]; then
        echo "==> Building Rust core for Android..."
        cd core

        if [ "$ALL_ABIS" = true ]; then
            TARGETS="aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android"
        else
            TARGETS="aarch64-linux-android"
        fi

        for target in $TARGETS; do
            rustup target add "$target" 2>/dev/null || true
            if [ "$BUILD_CONFIG" = "Release" ]; then
                cargo build --release --target "$target"
            else
                cargo build --target "$target"
            fi
        done

        cd "$PROJECT_ROOT"
    fi

    echo "==> Building Android APK..."
    cd android

    # Clean native build if switching backend configurations
    rm -rf app/.cxx app/build/intermediates/cmake 2>/dev/null || true

    GRADLE_TASK="assemble${BUILD_CONFIG}"
    GRADLE_ARGS=""

    # Pass backend flags to Gradle
    if [ "$BUILD_RUST" = true ]; then
        GRADLE_ARGS="$GRADLE_ARGS -PbuildRust=true"
    else
        GRADLE_ARGS="$GRADLE_ARGS -PbuildRust=false"
    fi

    if [ "$BUILD_CEMU" = true ]; then
        GRADLE_ARGS="$GRADLE_ARGS -PbuildCemu=true"
    else
        GRADLE_ARGS="$GRADLE_ARGS -PbuildCemu=false"
    fi

    if [ "$ALL_ABIS" = false ]; then
        GRADLE_ARGS="$GRADLE_ARGS -PabiFilters=arm64-v8a"
    fi

    ./gradlew $GRADLE_TASK $GRADLE_ARGS

    # Convert BUILD_CONFIG to lowercase for APK path (POSIX-compatible)
    BUILD_CONFIG_LOWER=$(echo "$BUILD_CONFIG" | tr '[:upper:]' '[:lower:]')
    APK_PATH="app/build/outputs/apk/${BUILD_CONFIG_LOWER}/app-${BUILD_CONFIG_LOWER}.apk"

    cd "$PROJECT_ROOT"

    echo ""
    echo "==> Build complete!"
    echo "    APK: android/$APK_PATH"

    if [ "$INSTALL" = true ]; then
        echo "==> Installing APK..."
        adb install -r "android/$APK_PATH"
        echo "==> Installed!"
    fi
}

#------------------------------------------------------------------------------
# iOS Build
#------------------------------------------------------------------------------
build_ios() {
    # Determine Rust target
    if [ "$TARGET" = "simulator" ]; then
        ARCH=$(uname -m)
        if [ "$ARCH" = "arm64" ]; then
            RUST_TARGET="aarch64-apple-ios-sim"
        else
            RUST_TARGET="x86_64-apple-ios"
        fi
        XCODE_DEST="platform=iOS Simulator,arch=arm64"
        PLATFORM_SUFFIX="iphonesimulator"
    else
        RUST_TARGET="aarch64-apple-ios"
        XCODE_DEST="generic/platform=iOS"
        PLATFORM_SUFFIX="iphoneos"
    fi

    # Set up output directory
    if [ "$BUILD_CONFIG" = "Release" ]; then
        LIB_CONFIG="release"
    else
        LIB_CONFIG="debug"
    fi
    DEST_DIR="$PROJECT_ROOT/core/target/$RUST_TARGET/$LIB_CONFIG"
    mkdir -p "$DEST_DIR"

    # Dual-backend build for iOS
    if [ "$BUILD_RUST" = true ] && [ "$BUILD_CEMU" = true ]; then
        echo "==> Building dual-backend for iOS (Rust + CEmu)..."

        # Build Rust with prefixed symbols
        echo "==> Building Rust core with prefixed symbols..."
        cd core
        rustup target add "$RUST_TARGET" 2>/dev/null || true
        if [ "$BUILD_CONFIG" = "Release" ]; then
            cargo build --release --target "$RUST_TARGET" --features ios_prefixed
        else
            cargo build --target "$RUST_TARGET" --features ios_prefixed
        fi
        # Rename to libemu_rust.a
        cp "$DEST_DIR/libemu_core.a" "$DEST_DIR/libemu_rust.a"
        cd "$PROJECT_ROOT"

        # Build CEmu with prefixed symbols
        echo "==> Building CEmu adapter with prefixed symbols..."
        mkdir -p ios/cemu
        cat > ios/cemu/CMakeLists.txt << 'CMAKEOF'
cmake_minimum_required(VERSION 3.20)
project(cemu_adapter C)
set(CMAKE_C_STANDARD 11)
option(IOS_PREFIXED "Export symbols with cemu_ prefix" OFF)
set(CEMU_CORE_DIR "${CMAKE_SOURCE_DIR}/../../cemu-ref/core")
file(GLOB CEMU_SOURCES "${CEMU_CORE_DIR}/*.c" "${CEMU_CORE_DIR}/usb/*.c")
list(APPEND CEMU_SOURCES "${CEMU_CORE_DIR}/os/os-linux.c")
set(ADAPTER_SOURCE "${CMAKE_SOURCE_DIR}/../../android/app/src/main/cpp/cemu/cemu_adapter.c")
add_library(cemu_adapter STATIC ${CEMU_SOURCES} ${ADAPTER_SOURCE})
target_include_directories(cemu_adapter PRIVATE
    ${CEMU_CORE_DIR}
    ${CEMU_CORE_DIR}/usb
    ${CEMU_CORE_DIR}/os
    ${CMAKE_SOURCE_DIR}/../include
)
target_compile_definitions(cemu_adapter PRIVATE MULTITHREAD=0 CEMU_NO_UI=1)
if(IOS_PREFIXED)
    target_compile_definitions(cemu_adapter PRIVATE IOS_PREFIXED=1)
endif()
target_compile_options(cemu_adapter PRIVATE -w)
CMAKEOF

        cd ios/cemu
        BUILD_DIR="build-$TARGET-dual"
        rm -rf "$BUILD_DIR"
        mkdir -p "$BUILD_DIR"
        cd "$BUILD_DIR"

        CMAKE_EXTRA=""
        [ "$TARGET" = "simulator" ] && CMAKE_EXTRA="-DCMAKE_OSX_SYSROOT=iphonesimulator"

        cmake .. -G Xcode \
            -DCMAKE_SYSTEM_NAME=iOS \
            -DCMAKE_OSX_ARCHITECTURES=arm64 \
            -DCMAKE_OSX_DEPLOYMENT_TARGET=16.0 \
            -DIOS_PREFIXED=ON \
            $CMAKE_EXTRA

        cmake --build . --config "$BUILD_CONFIG"

        CEMU_LIB="$(pwd)/$BUILD_CONFIG-$PLATFORM_SUFFIX/libcemu_adapter.a"
        cp "$CEMU_LIB" "$DEST_DIR/libemu_cemu.a"
        cd "$PROJECT_ROOT"

        # Build backend bridge
        echo "==> Building backend bridge..."
        BRIDGE_SRC="ios/Calc/Bridge/backend_bridge.c"
        BRIDGE_OBJ="$DEST_DIR/backend_bridge.o"

        # Determine SDK path
        if [ "$TARGET" = "simulator" ]; then
            SDK_PATH=$(xcrun --sdk iphonesimulator --show-sdk-path)
        else
            SDK_PATH=$(xcrun --sdk iphoneos --show-sdk-path)
        fi

        clang -c "$BRIDGE_SRC" -o "$BRIDGE_OBJ" \
            -target arm64-apple-ios16.0 \
            -isysroot "$SDK_PATH" \
            -I ios/include \
            -DHAS_RUST_BACKEND=1 \
            -DHAS_CEMU_BACKEND=1

        # Create combined library
        ar rcs "$DEST_DIR/libemu_core.a" "$BRIDGE_OBJ"

        LIBRARY_PATH="$DEST_DIR"
        OTHER_LDFLAGS="-lemu_core -lemu_rust -lemu_cemu"

        echo ""
        echo "==> Dual-backend build complete!"
        echo "    Libraries: $LIBRARY_PATH"
        echo "      - libemu_core.a (backend bridge)"
        echo "      - libemu_rust.a (Rust backend)"
        echo "      - libemu_cemu.a (CEmu backend)"

    elif [ "$BUILD_CEMU" = true ]; then
        echo "==> Building CEmu adapter for iOS..."

        # Setup CEmu iOS build (single backend, no prefix)
        mkdir -p ios/cemu
        cat > ios/cemu/CMakeLists.txt << 'CMAKEOF'
cmake_minimum_required(VERSION 3.20)
project(cemu_adapter C)
set(CMAKE_C_STANDARD 11)
set(CEMU_CORE_DIR "${CMAKE_SOURCE_DIR}/../../cemu-ref/core")
file(GLOB CEMU_SOURCES "${CEMU_CORE_DIR}/*.c" "${CEMU_CORE_DIR}/usb/*.c")
list(APPEND CEMU_SOURCES "${CEMU_CORE_DIR}/os/os-linux.c")
set(ADAPTER_SOURCE "${CMAKE_SOURCE_DIR}/../../android/app/src/main/cpp/cemu/cemu_adapter.c")
add_library(cemu_adapter STATIC ${CEMU_SOURCES} ${ADAPTER_SOURCE})
target_include_directories(cemu_adapter PRIVATE
    ${CEMU_CORE_DIR}
    ${CEMU_CORE_DIR}/usb
    ${CEMU_CORE_DIR}/os
    ${CMAKE_SOURCE_DIR}/../include
)
target_compile_definitions(cemu_adapter PRIVATE MULTITHREAD=0 CEMU_NO_UI=1)
target_compile_options(cemu_adapter PRIVATE -w)
CMAKEOF

        cd ios/cemu
        BUILD_DIR="build-$TARGET"
        mkdir -p "$BUILD_DIR"
        cd "$BUILD_DIR"

        CMAKE_EXTRA=""
        [ "$TARGET" = "simulator" ] && CMAKE_EXTRA="-DCMAKE_OSX_SYSROOT=iphonesimulator"

        cmake .. -G Xcode \
            -DCMAKE_SYSTEM_NAME=iOS \
            -DCMAKE_OSX_ARCHITECTURES=arm64 \
            -DCMAKE_OSX_DEPLOYMENT_TARGET=16.0 \
            $CMAKE_EXTRA

        cmake --build . --config "$BUILD_CONFIG"

        CEMU_LIB="$(pwd)/$BUILD_CONFIG-$PLATFORM_SUFFIX/libcemu_adapter.a"
        rm -f "$DEST_DIR/libemu_core.a"
        cp "$CEMU_LIB" "$DEST_DIR/libemu_core.a"
        echo "==> Copied CEmu library to $DEST_DIR/libemu_core.a"

        LIBRARY_PATH="$DEST_DIR"
        OTHER_LDFLAGS="-lemu_core"

        cd "$PROJECT_ROOT"
    else
        echo "==> Building Rust core for iOS ($RUST_TARGET)..."
        cd core

        rustup target add "$RUST_TARGET" 2>/dev/null || true

        if [ "$BUILD_CONFIG" = "Release" ]; then
            cargo build --release --target "$RUST_TARGET"
        else
            cargo build --target "$RUST_TARGET"
        fi

        LIBRARY_PATH="$DEST_DIR"
        OTHER_LDFLAGS="-lemu_core"

        cd "$PROJECT_ROOT"
    fi

    echo ""
    echo "==> Backend build complete!"
    echo "    Library path: $LIBRARY_PATH"
    echo "    Linker flags: $OTHER_LDFLAGS"
    echo ""
    echo "Open ios/Calc.xcodeproj in Xcode to build and run the app."
}

# Run the appropriate build
if [ "$PLATFORM" = "android" ]; then
    build_android
else
    build_ios
fi
