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

## Usage

1. Obtain a TI-84 Plus CE ROM file legally (dump from your own calculator)
2. Install the app
3. Use "Import ROM" to load your ROM file
4. Press Run to start emulation

## License

See LICENSE file.

## Legal Notice

This emulator does not include any copyrighted ROM or OS images. You must provide your own legally obtained ROM file.
