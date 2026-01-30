#!/bin/bash
# Build Rust core for Android and compile the APK
# Usage: ./scripts/build-android.sh [--install]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

# Parse args
INSTALL=false
for arg in "$@"; do
    case $arg in
        --install|-i)
            INSTALL=true
            ;;
    esac
done

echo "==> Building Rust core for Android targets..."

# Build for all Android ABIs
cd core
cargo build --release --target aarch64-linux-android
cargo build --release --target armv7-linux-androideabi
cargo build --release --target x86_64-linux-android
cargo build --release --target i686-linux-android

echo "==> Building Android APK..."
cd "$PROJECT_ROOT/android"
./gradlew assembleDebug

APK_PATH="app/build/outputs/apk/debug/app-debug.apk"
echo "==> APK built: $APK_PATH"

if [ "$INSTALL" = true ]; then
    echo "==> Installing APK..."
    adb install -r "$APK_PATH"
    echo "==> Done! APK installed."
else
    echo "==> Done! Run with --install to install to device."
fi
