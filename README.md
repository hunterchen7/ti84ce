# TI-84 Plus CE Emulator

An Android-first (iOS-ready) emulator for TI-84 Plus CE calculators.

## Project Structure

```
calc/
  core/           # Rust portable emulator core (C ABI)
  android/        # Android app (Kotlin + Jetpack Compose)
  docs/           # Architecture and milestone documentation
```

## Building

### Prerequisites

- Rust toolchain with Android targets
- Android Studio with NDK
- Android SDK (API 24+)

### Core (Rust)

```bash
cd core
cargo build --release
```

### Core for Android

The Android app links against the Rust core compiled for Android targets. The Gradle build will compile Rust automatically via CMake, but you can also build manually:

**One-time setup** - Install Android targets:

```bash
rustup target add aarch64-linux-android    # ARM64 (most devices)
rustup target add armv7-linux-androideabi  # ARM32 (older devices)
rustup target add x86_64-linux-android     # x86_64 emulator
rustup target add i686-linux-android       # x86 emulator
```

**Manual build** (useful when iterating on Rust code):

```bash
cd core
cargo build --target aarch64-linux-android --release
```

The compiled library goes to `core/target/<target>/release/libemu_core.a`.

**Note**: After changing Rust code, you must rebuild for Android before the changes appear in the app. The Gradle build caches the native library, so either:
- Run `./gradlew clean` before building, or
- Manually rebuild the Rust target as shown above

### Android

```bash
cd android
./gradlew assembleDebug
```

### Development Workflow

For rapid iteration on Android:

```bash
# Terminal 1: Watch for Kotlin changes and auto-deploy
cd android
./watch.sh

# When changing Rust code, rebuild for Android first:
cd core && cargo build --target aarch64-linux-android --release
# Then touch a Kotlin file or run ./gradlew installDebug
```

The `watch.sh` script requires `fswatch` (`brew install fswatch`).

## Testing

### Core (Rust)

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

| Command | Description |
|---------|-------------|
| `boot` | Run boot test with progress reporting |
| `trace [steps]` | Generate trace log for parity comparison (default: 100k) |
| `screen [output]` | Render screen to PNG after boot (default: screen.png) |
| `vram` | Analyze VRAM content (color histogram) |
| `compare <file>` | Compare our trace with CEmu trace file |
| `help` | Show help message |

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

1. Generate our trace: `cargo run --release --example debug -- trace 1000000`
2. Generate CEmu trace (requires cemu-ref/ clone with trace_cli)
3. Compare: `cargo run --release --example debug -- compare traces/cemu.log`

## Usage

1. BYOR (Bring your own ROM) - Obtain a TI-84 Plus CE ROM file legally
2. Install the app
3. Use "Import ROM" to load your ROM file
4. Press Run to start emulation

## License

See LICENSE file.

## Legal Notice

This emulator does not include any copyrighted ROM or OS images. You must provide your own legally obtained ROM file.
