# TI-84 CE Emulator Build Commands

.PHONY: android android-fast android-install test clean

# Build Android APK (all ABIs)
android:
	./scripts/build-android.sh

# Fast Android build (arm64 only) + install
android-fast:
	./scripts/build-android-fast.sh

# Build all ABIs and install
android-install:
	./scripts/build-android.sh --install

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
	@echo "  make test           - Run Rust tests"
	@echo "  make clean          - Clean all build artifacts"
