# TI-84 Plus CE Emulator

A cross-platform emulator for TI-84 Plus CE calculators, with native Android, iOS, and Web apps.

## Project Structure

```
calc/
  core/           # Rust emulator core (C ABI + WASM)
  android/        # Android app (Kotlin + Jetpack Compose)
  ios/            # iOS app (Swift + SwiftUI)
  web/            # Web app (React + TypeScript + Vite)
  scripts/        # Build scripts
  docs/           # Architecture and milestone documentation
  cemu-ref/       # CEmu reference emulator (git-ignored, optional)
```

## Architecture

### Dual Backend Design

The mobile apps (Android & iOS) and web app are designed to work with **two interchangeable emulator backends**:

```
┌─────────────────────────────────────────────────────────┐
│                      App (UI)                           │
│              Android (Kotlin/Compose)                   │
│              iOS (Swift/SwiftUI)                        │
│              Web (React/TypeScript)                     │
├─────────────────────────────────────────────────────────┤
│            C API (emu.h) / WASM Bindings                │
│    emu_create, emu_load_rom, emu_run_cycles,            │
│    emu_framebuffer, emu_set_key, ...                    │
├───────────────────────┬─────────────────────────────────┤
│   Rust Core           │   CEmu Adapter                  │
│   (libemu_core.a)     │   (libcemu_adapter.a)           │
│   (emu_core.wasm)     │   (cemu.wasm via Emscripten)    │
│                       │                                 │
│   Our implementation  │   Wraps CEmu reference          │
│   from scratch        │   emulator                      │
└───────────────────────┴─────────────────────────────────┘
```

Both backends implement the same C API (`core/include/emu.h`), allowing the apps to switch between them at build time without any code changes.

### Rust Core (Default)

The Rust core (`core/`) is our from-scratch implementation of the TI-84 Plus CE hardware:

- **eZ80 CPU** - Full instruction set with ADL mode (24-bit addressing)
- **Memory** - Flash (4MB), RAM (256KB), VRAM, memory-mapped I/O
- **Peripherals** - LCD controller, keypad, timers, RTC, interrupts, SPI
- **Scheduler** - Cycle-accurate event scheduling

### CEmu Backend (Reference)

[CEmu](https://github.com/CE-Programming/CEmu) is the established open-source TI-84 Plus CE emulator. We use it as a reference implementation for:

- **Parity testing** - Compare our emulation against known-correct behavior
- **Debugging** - When something doesn't work, check if CEmu behaves the same
- **Development** - Test app features before Rust implementation is complete

The CEmu adapter (`android/app/src/main/cpp/cemu/`) wraps CEmu's C code to match our API.

### What the Emulator Does

The emulator recreates the TI-84 Plus CE hardware in software:

1. **Loads ROM** - The calculator's operating system (you provide this)
2. **Executes CPU** - Runs eZ80 instructions at ~48MHz emulated speed
3. **Renders Display** - 320x240 16-bit color LCD at 60 FPS
4. **Handles Input** - 8x7 key matrix matching the physical calculator
5. **Emulates Peripherals** - Timers, real-time clock, interrupts, etc.

The apps provide the UI (screen display, keypad, menus) while the backend handles all emulation logic.

## Building

### Prerequisites

- Rust toolchain
- For Android: Android Studio with NDK, Android SDK (API 24+)
- For iOS: Xcode 15+, macOS
- For Web: Node.js 18+, [wasm-pack](https://rustwasm.github.io/wasm-pack/)

### Unified Build Script

The project uses a single build script for both platforms:

```bash
./scripts/build.sh <platform> [options]
```

**Platforms:** `android`, `ios`

**Options:**
| Option | Description |
|--------|-------------|
| `--release` | Release build (default) |
| `--debug` | Debug build |
| `--cemu` | Use CEmu backend instead of Rust |
| `--install` | Android: Install APK after build |
| `--sim` | iOS: Build for Simulator |
| `--all-abis` | Android: Build all ABIs (default: arm64 only) |

> **Note:** For iOS, the build script only compiles the backend library. Open `ios/Calc.xcodeproj` in Xcode to build and run the app.

**Examples:**

```bash
# Android
./scripts/build.sh android                    # Release, arm64, Rust
./scripts/build.sh android --debug --install  # Debug + install to device
./scripts/build.sh android --cemu             # Release with CEmu backend

# iOS (builds backend library, then open Xcode to build app)
./scripts/build.sh ios                        # Release, device, Rust
./scripts/build.sh ios --sim --debug          # Simulator, Debug
./scripts/build.sh ios --sim --cemu           # Simulator, CEmu backend
```

### Make Targets (Shortcuts)

```bash
# Android
make android              # Release, arm64, Rust
make android-debug        # Debug, arm64, Rust
make android-cemu         # Release, CEmu backend
make android-install      # Release + install
make android-cemu-install # CEmu + install

# iOS (builds backend library, then open Xcode to build app)
make ios              # Release, device, Rust
make ios-debug        # Debug, device, Rust
make ios-cemu         # Release, device, CEmu
make ios-sim          # Release, simulator, Rust
make ios-sim-cemu     # Release, simulator, CEmu

# Web
make web              # Build WASM + web app (production)
make web-dev          # Start dev server with hot reload
make web-cemu         # Build with CEmu WASM backend

# Utilities
make test             # Run Rust tests
make clean            # Clean all build artifacts
make help             # Show all targets
```

### Platform-Specific Setup

#### Android

**One-time setup** - Install Android targets:

```bash
rustup target add aarch64-linux-android    # ARM64 (most devices)
rustup target add armv7-linux-androideabi  # ARM32 (older devices)
rustup target add x86_64-linux-android     # x86_64 emulator
rustup target add i686-linux-android       # x86 emulator
```

**Manual Gradle build** (if not using build.sh):

```bash
cd android
./gradlew assembleDebug
```

#### iOS

**One-time setup** - Install iOS targets:

```bash
rustup target add aarch64-apple-ios        # iOS device (arm64)
rustup target add aarch64-apple-ios-sim    # iOS Simulator (Apple Silicon)
rustup target add x86_64-apple-ios         # iOS Simulator (Intel)
```

**Building and Running:**

```bash
# 1. Build the backend library
./scripts/build.sh ios --sim           # For simulator
./scripts/build.sh ios                 # For device

# 2. Open Xcode and build/run the app
open ios/Calc.xcodeproj
# Press Cmd+R to build and run
```

The build script compiles the emulator backend (Rust or CEmu) as a static library. Xcode handles building the Swift app and linking against the library.

#### Web

**One-time setup** - Install WASM target and wasm-pack:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

**Building and Running:**

```bash
# Development (with hot reload)
make web-dev

# Production build
make web
# Output in web/dist/
```

The web app runs entirely in the browser using WebAssembly (~96KB gzipped).

**Keyboard Controls:**

| Key | Function |
|-----|----------|
| 0-9 | Number keys |
| + - * / | Math operations |
| ( ) | Parentheses |
| Enter | Enter |
| Backspace | Delete |
| Arrow keys | Navigation |
| Escape | Mode |
| Shift | 2nd |
| Alt | Alpha |
| O | ON key |

### Development Workflow

For rapid iteration on Android:

```bash
# Terminal 1: Watch for Kotlin changes and auto-deploy
cd android
./watch.sh

# When changing Rust code, rebuild:
./scripts/build.sh android --debug --install
```

The `watch.sh` script requires `fswatch` (`brew install fswatch`).

### CEmu Backend (Alternative)

The apps can be built with CEmu instead of our Rust core. This is useful for:

| Use Case                 | Description                                                             |
| ------------------------ | ----------------------------------------------------------------------- |
| **Parity Testing**       | Compare behavior: does our Rust core produce the same results as CEmu?  |
| **Bug Investigation**    | If something breaks, check if it's our bug or a ROM/hardware quirk      |
| **Feature Development**  | Test new app features (UI, input) before Rust implementation catches up |
| **Performance Baseline** | Compare frame rates and responsiveness                                  |

**Setup:**

```bash
# Clone CEmu (one-time, git-ignored)
git clone https://github.com/CE-Programming/CEmu.git cemu-ref
```

**Build with CEmu:**

```bash
./scripts/build.sh android --cemu       # Android with CEmu backend
./scripts/build.sh ios --sim --cemu     # iOS simulator with CEmu backend
./scripts/build.sh ios --cemu           # iOS device with CEmu backend
# Then open ios/Calc.xcodeproj in Xcode to build the app

make web-cemu                           # Web with CEmu WASM backend
```

The app behavior should be identical regardless of backend - if it differs, that's a bug to investigate.

## Testing

### Rust Unit Tests

```bash
cd core
cargo test
```

This runs the test suite covering:

- Memory subsystem (Flash, RAM, VRAM, ports)
- Bus address decoding and wait states
- eZ80 CPU instructions and flag behavior
- ADL mode 24-bit operations
- TI-84 CE memory map verification

To run a specific test:

```bash
cargo test test_name
```

To see test output:

```bash
cargo test -- --nocapture
```

### CEmu Parity Tools

Test tools in `tools/cemu-test/` compare CEmu (reference emulator) behavior with our Rust implementation.

**Prerequisites:**

1. Clone CEmu: `git clone https://github.com/CE-Programming/CEmu.git cemu-ref`
2. Build CEmu core: `cd cemu-ref/core && make`
3. Obtain a TI-84 Plus CE ROM file

**Build the tools:**

```bash
cd tools/cemu-test
make
```

**parity_check** - Verifies RTC timing, MathPrint flag, and key state at cycle milestones:

```bash
./parity_check                           # Defaults: 60M cycles, ROM at ../../TI-84 CE.rom
./parity_check /path/to/rom.rom -m 100000000  # Custom ROM and cycle count
./parity_check -v                        # Verbose mode
```

Key addresses monitored:

- `0xD000C4` - MathPrint flag (bit 5: 1=MathPrint, 0=Classic)
- `0xF80020` - RTC control register (bit 6: load in progress)
- `0xF80040` - RTC load status (0x00=complete, 0xF8=all pending)

**trace_gen** - Generates CPU instruction traces for direct comparison:

```bash
./trace_gen ../../TI-84\ CE.rom                    # 1M steps to stdout
./trace_gen ../../TI-84\ CE.rom -n 100000 -o cemu_trace.txt  # To file
```

Output format: `step cycles PC SP AF BC DE HL IX IY ADL IFF1 IFF2 IM HALT opcode`

## Debugging

The emulator includes a consolidated debug tool for testing, tracing, and diagnostics.

### Quick Commands (Cargo Aliases)

Run these from the `core/` directory:

```bash
cd core
cargo boot      # Run boot test with progress reporting
cargo screen    # Render screen to PNG after boot
cargo vram      # Analyze VRAM colors
cargo trace     # Generate trace log (100k steps)
cargo dbg       # Show debug tool help
cargo t         # Run all tests
cargo rb        # Release build
```

### Full Debug Tool

For more options, use the debug tool directly (from `core/`):

```bash
cargo run --release --example debug -- <command>
```

| Command           | Description                                              |
| ----------------- | -------------------------------------------------------- |
| `boot`            | Run boot test with progress reporting                    |
| `trace [steps]`   | Generate trace log for parity comparison (default: 100k) |
| `screen [output]` | Render screen to PNG after boot (default: screen.png)    |
| `vram`            | Analyze VRAM content (color histogram)                   |
| `compare <file>`  | Compare our trace with CEmu trace file                   |
| `help`            | Show help message                                        |

**Examples:**

```bash
# Generate 1M step trace for parity comparison
cargo run --release --example debug -- trace 1000000

# Save screenshot with custom name
cargo run --release --example debug -- screen boot.png

# Compare with CEmu trace
cargo run --release --example debug -- compare ../traces/cemu.log
```

### Boot Status

The emulator successfully boots the TI-84 CE OS:

- **3,609,969 steps** (~61.6M cycles) to reach OS idle
- Screen shows "RAM Cleared" message with full status bar
- CPU reaches idle loop at PC=0x085B7F (EI + HALT)

### Trace Comparison

For detailed parity testing with CEmu:

```bash
# 1. Generate CEmu trace
cd tools/cemu-test
./trace_gen ../../TI-84\ CE.rom -n 10000 -o cemu_trace.txt

# 2. Generate Rust trace
cd ../../core
cargo run --release --example debug -- trace 10000 > rust_trace.txt

# 3. Compare
diff ../tools/cemu-test/cemu_trace.txt rust_trace.txt | head -50

# Or use the built-in compare command
cargo run --release --example debug -- compare ../tools/cemu-test/cemu_trace.txt
```

## Usage

1. BYOR (Bring your own ROM) - Obtain a TI-84 Plus CE ROM file legally
2. Install the app
3. Use "Import ROM" to load your ROM file
4. Press Run to start emulation

## License

See LICENSE file.

## Legal Notice

This emulator does not include any copyrighted ROM or OS images. You must provide your own legally obtained ROM file.
