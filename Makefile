# TI-84 CE Emulator Build Commands
#
# Quick reference:
#   ./scripts/build.sh android [--debug] [--rust|--cemu|--both] [--install]
#   ./scripts/build.sh ios [--debug] [--rust|--cemu|--both] [--sim] [--open]

.PHONY: android android-debug android-install android-both android-both-install \
        android-cemu android-cemu-install \
        ios ios-debug ios-sim ios-cemu ios-sim-cemu ios-both ios-sim-both \
        web web-cemu web-dev web-clean \
        log-android test clean cemu cemu-test cemu-clean help

#------------------------------------------------------------------------------
# Android - Rust only (default)
#------------------------------------------------------------------------------

# Android release (arm64, Rust only)
android:
	./scripts/build.sh android --rust

# Android debug (arm64, Rust only)
android-debug:
	./scripts/build.sh android --rust --debug

# Android release + install (Rust only, all ABIs)
android-install:
	./scripts/build.sh android --rust --install --all-abis

#------------------------------------------------------------------------------
# Android - Both backends (runtime switching)
#------------------------------------------------------------------------------

# Android release with both backends
android-both:
	./scripts/build.sh android --both

# Android release with both backends + install
android-both-install:
	./scripts/build.sh android --both --install

#------------------------------------------------------------------------------
# Android - CEmu only
#------------------------------------------------------------------------------

# Android release with CEmu backend only
android-cemu:
	./scripts/build.sh android --cemu

# Android CEmu + install
android-cemu-install:
	./scripts/build.sh android --cemu --install

#------------------------------------------------------------------------------
# iOS - Rust only (default)
#------------------------------------------------------------------------------

# iOS device release (arm64, Rust)
ios:
	./scripts/build.sh ios --rust

# iOS device debug (arm64, Rust)
ios-debug:
	./scripts/build.sh ios --rust --debug

# iOS Simulator (Rust)
ios-sim:
	./scripts/build.sh ios --rust --sim

#------------------------------------------------------------------------------
# iOS - Both backends (runtime switching)
#------------------------------------------------------------------------------

# iOS device release with both backends
ios-both:
	./scripts/build.sh ios --both

# iOS Simulator with both backends
ios-sim-both:
	./scripts/build.sh ios --both --sim

#------------------------------------------------------------------------------
# iOS - CEmu only
#------------------------------------------------------------------------------

# iOS device release with CEmu backend
ios-cemu:
	./scripts/build.sh ios --cemu

# iOS Simulator with CEmu backend
ios-sim-cemu:
	./scripts/build.sh ios --cemu --sim

#------------------------------------------------------------------------------
# Web
#------------------------------------------------------------------------------

# Web release (Rust WASM)
web:
	@echo "Building Rust WASM package..."
	cd core && wasm-pack build --target web --release
	@echo "Copying WASM package to web app..."
	rm -rf web/src/emu-core
	cp -r core/pkg web/src/emu-core
	@echo "Installing npm dependencies..."
	cd web && npm install
	@echo "Building web app..."
	cd web && npm run build
	@echo ""
	@echo "Done! Output in web/dist/"
	@echo "To serve locally: cd web && npx serve dist"

# Web release with CEmu backend (Emscripten)
web-cemu:
	@echo "Building CEmu WASM with Emscripten..."
	@if ! command -v emcc >/dev/null 2>&1; then \
		echo "Error: Emscripten not found. Install with: brew install emscripten"; \
		exit 1; \
	fi
	@if [ ! -d "cemu-ref" ]; then \
		echo "Cloning CEmu reference repository..."; \
		git clone --depth 1 https://github.com/CE-Programming/CEmu.git cemu-ref; \
	fi
	$(MAKE) -C web -f cemu-emscripten.mk wasm
	@echo "Copying CEmu WASM to web app..."
	mkdir -p web/src/cemu-core
	cp web/build-cemu/WebCEmu.js web/build-cemu/WebCEmu.wasm web/src/cemu-core/
	@echo ""
	@echo "CEmu WASM built! Files in web/src/cemu-core/"

# Web development server
web-dev:
	@if [ ! -d "web/src/emu-core" ]; then \
		echo "WASM package not found. Building first..."; \
		$(MAKE) web; \
	fi
	cd web && npm run dev

# Clean web artifacts
web-clean:
	rm -rf web/dist web/node_modules web/src/emu-core
	rm -rf core/pkg

#------------------------------------------------------------------------------
# Utilities
#------------------------------------------------------------------------------

# Capture Android emulator logs
log-android:
	@echo "Capturing Android emulator logs..."
	@echo "Press Ctrl+C to stop logging"
	@adb logcat -c
	@adb logcat EmuCore:V EmuJNI:V EmuBackend:V EmulatorBridge:V MainActivity:D *:S | tee emulator_logs.txt

# Run Rust tests
test:
	cd core && cargo test --lib

# Clean all build artifacts
clean:
	cd core && cargo clean
	-cd android && ./gradlew clean
	rm -rf android/app/.cxx android/app/build/intermediates/cmake
	rm -rf ios/build ios/cemu/build-* ios/DerivedData
	rm -rf web/dist web/src/emu-core core/pkg

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
	@echo "  Options:   --debug, --rust, --cemu, --both, --install, --sim, --open"
	@echo ""
	@echo "Make targets (shortcuts):"
	@echo ""
	@echo "  Android (Rust only - default):"
	@echo "    make android              Release, arm64, Rust"
	@echo "    make android-debug        Debug, arm64, Rust"
	@echo "    make android-install      Release + install, all ABIs, Rust"
	@echo ""
	@echo "  Android (Both backends - runtime switching):"
	@echo "    make android-both         Release, both backends"
	@echo "    make android-both-install Release + install, both backends"
	@echo ""
	@echo "  Android (CEmu only):"
	@echo "    make android-cemu         Release, CEmu only"
	@echo "    make android-cemu-install Release + install, CEmu"
	@echo ""
	@echo "  iOS (Rust only - default):"
	@echo "    make ios             Release, device, Rust"
	@echo "    make ios-debug       Debug, device, Rust"
	@echo "    make ios-sim         Release, simulator, Rust"
	@echo ""
	@echo "  iOS (Both backends - runtime switching):"
	@echo "    make ios-both        Release, device, both backends"
	@echo "    make ios-sim-both    Release, simulator, both backends"
	@echo ""
	@echo "  iOS (CEmu only):"
	@echo "    make ios-cemu        Release, device, CEmu"
	@echo "    make ios-sim-cemu    Release, simulator, CEmu"
	@echo ""
	@echo "  Web:"
	@echo "    make web             Build web app (Rust WASM)"
	@echo "    make web-cemu        Build CEmu WASM (Emscripten)"
	@echo "    make web-dev         Run web dev server"
	@echo "    make web-clean       Clean web artifacts"
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
