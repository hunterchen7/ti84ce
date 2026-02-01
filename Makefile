# TI-84 CE Emulator Build Commands

.PHONY: android android-fast android-install log-android test clean

# Build Android APK (all ABIs)
android:
	./scripts/build-android.sh

# Fast Android build (arm64 only) + install
android-fast:
	./scripts/build-android-fast.sh

# Build all ABIs and install
android-install:
	./scripts/build-android.sh --install

# Capture Android emulator logs to file
log-android:
	@echo "Capturing Android emulator logs..."
	@echo "Press Ctrl+C to stop logging"
	@adb logcat -c
	@adb logcat EmuCore:V EmuJNI:V MainActivity:D *:S | tee emulator_logs.txt

# Run Rust tests
test:
	cd core && cargo test --lib

# Clean build artifacts
clean:
	cd core && cargo clean
	cd android && ./gradlew clean

# Help
help:
	@echo "Available targets:"
	@echo "  make android        - Build APK for all Android ABIs"
	@echo "  make android-fast   - Build arm64 only + install (fastest)"
	@echo "  make android-install- Build all ABIs + install"
	@echo "  make log-android    - Capture emulator logs to emulator_logs.txt"
	@echo "  make test           - Run Rust tests"
	@echo "  make clean          - Clean all build artifacts"
