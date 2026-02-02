# TI-84 CE Emulator Build Commands
#
# Quick reference:
#   ./scripts/build.sh android [--debug] [--cemu] [--install] [--all-abis]
#   ./scripts/build.sh ios [--debug] [--cemu] [--sim] [--open]

.PHONY: android android-debug android-cemu android-install android-cemu-install \
        ios ios-debug ios-cemu ios-sim ios-sim-cemu \
        log-android test clean cemu cemu-test cemu-clean help

#------------------------------------------------------------------------------
# Android
#------------------------------------------------------------------------------

# Android release (arm64, Rust)
android:
	./scripts/build.sh android

# Android debug (arm64, Rust)
android-debug:
	./scripts/build.sh android --debug

# Android release with CEmu backend
android-cemu:
	./scripts/build.sh android --cemu

# Android release + install
android-install:
	./scripts/build.sh android --install

# Android CEmu + install
android-cemu-install:
	./scripts/build.sh android --cemu --install

#------------------------------------------------------------------------------
# iOS
#------------------------------------------------------------------------------

# iOS device release (arm64, Rust)
ios:
	./scripts/build.sh ios

# iOS device debug (arm64, Rust)
ios-debug:
	./scripts/build.sh ios --debug

# iOS device release with CEmu backend
ios-cemu:
	./scripts/build.sh ios --cemu

# iOS Simulator (Rust)
ios-sim:
	./scripts/build.sh ios --sim

# iOS Simulator with CEmu backend
ios-sim-cemu:
	./scripts/build.sh ios --sim --cemu

#------------------------------------------------------------------------------
# Utilities
#------------------------------------------------------------------------------

# Capture Android emulator logs
log-android:
	@echo "Capturing Android emulator logs..."
	@echo "Press Ctrl+C to stop logging"
	@adb logcat -c
	@adb logcat EmuCore:V EmuJNI:V MainActivity:D *:S | tee emulator_logs.txt

# Run Rust tests
test:
	cd core && cargo test --lib

# Clean all build artifacts
clean:
	cd core && cargo clean
	-cd android && ./gradlew clean
	rm -rf android/app/.cxx android/app/build/intermediates/cmake
	rm -rf ios/build ios/cemu/build-* ios/DerivedData

#------------------------------------------------------------------------------
# CEmu (reference emulator for macOS)
#------------------------------------------------------------------------------

cemu:
	@echo "Building CEmu core library..."
	$(MAKE) -C cemu-ref/core lib
	@echo "Building CEmu wrapper..."
	$(MAKE) -C cemu-ref/test cemu_wrapper.o
	@echo "CEmu library built: cemu-ref/core/libcemucore.a"

cemu-test: cemu
	@echo "Building CEmu test programs..."
	$(MAKE) -C cemu-ref/test all
	@echo ""
	@echo "Run tests with:"
	@echo "  cd cemu-ref/test && ./test_cemu 'path/to/TI-84 CE.rom'"
	@echo "  cd cemu-ref/test && ./test_wrapper 'path/to/TI-84 CE.rom'"

cemu-clean:
	$(MAKE) -C cemu-ref/core clean
	$(MAKE) -C cemu-ref/test clean

#------------------------------------------------------------------------------
# Help
#------------------------------------------------------------------------------

help:
	@echo "TI-84 CE Emulator Build System"
	@echo ""
	@echo "Unified build script:"
	@echo "  ./scripts/build.sh <platform> [options]"
	@echo ""
	@echo "  Platforms: android, ios"
	@echo "  Options:   --debug, --cemu, --install, --sim, --open, --all-abis"
	@echo ""
	@echo "Make targets (shortcuts):"
	@echo ""
	@echo "  Android:"
	@echo "    make android              Release, arm64, Rust"
	@echo "    make android-debug        Debug, arm64, Rust"
	@echo "    make android-cemu         Release, arm64, CEmu"
	@echo "    make android-install      Release, arm64, Rust + install"
	@echo "    make android-cemu-install Release, arm64, CEmu + install"
	@echo ""
	@echo "  iOS:"
	@echo "    make ios             Release, device, Rust"
	@echo "    make ios-debug       Debug, device, Rust"
	@echo "    make ios-cemu        Release, device, CEmu"
	@echo "    make ios-sim         Release, simulator, Rust"
	@echo "    make ios-sim-cemu    Release, simulator, CEmu"
	@echo ""
	@echo "  Utilities:"
	@echo "    make test            Run Rust tests"
	@echo "    make clean           Clean all build artifacts"
	@echo "    make log-android     Capture Android logs"
	@echo ""
	@echo "  CEmu (macOS reference):"
	@echo "    make cemu            Build CEmu library"
	@echo "    make cemu-test       Build CEmu test programs"
	@echo "    make cemu-clean      Clean CEmu artifacts"
