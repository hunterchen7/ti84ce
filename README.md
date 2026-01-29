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

### Android

```bash
cd android
./gradlew assembleDebug
```

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

### Boot Trace

To capture an execution trace during ROM boot (useful for comparing with CEmu):

```bash
cargo run --example trace_boot --manifest-path core/Cargo.toml > trace_ours.log
```

This runs the emulator with a ROM file (`TI-84 CE.rom` in project root) and outputs:

- CPU state at each instruction (PC, registers, flags)
- Memory reads/writes
- Port I/O operations

### Boot Test

To run the boot test with progress reporting:

```bash
cargo run --example boot_test --manifest-path core/Cargo.toml
```

This shows boot progress, LCD state, and screen analysis.

## Usage

1. BYOR (Bring your own ROM) - Obtain a TI-84 Plus CE ROM file legally
2. Install the app
3. Use "Import ROM" to load your ROM file
4. Press Run to start emulation

## License

See LICENSE file.

## Legal Notice

This emulator does not include any copyrighted ROM or OS images. You must provide your own legally obtained ROM file.
