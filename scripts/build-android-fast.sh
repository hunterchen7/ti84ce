#!/bin/bash
# Fast Android build - only arm64, auto-installs
# Usage: ./scripts/build-android-fast.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

echo "==> Building Rust core (arm64 only)..."
cd core
cargo build --release --target aarch64-linux-android

echo "==> Building Android APK..."
cd "$PROJECT_ROOT/android"
./gradlew assembleDebug

echo "==> Installing APK..."
adb install -r app/build/outputs/apk/debug/app-debug.apk

echo "==> Done!"
