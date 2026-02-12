# TI-84 Plus CE Web Emulator

A browser-based TI-84 Plus CE graphing calculator emulator using WebAssembly.

## Prerequisites

- [Rust](https://rustup.rs/) with `wasm32-unknown-unknown` target
- [wasm-pack](https://rustwasm.github.io/wasm-pack/)
- Node.js 18+

Install WASM target and wasm-pack:
```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
```

## Development

1. Build the WASM package:
```bash
cd ../core
wasm-pack build --target web --release
```

2. Copy the package to the web app:
```bash
cp -r ../core/pkg src/emu-core
```

3. Install dependencies and run dev server:
```bash
npm install
npm run dev
```

## Production Build

Run the build script to compile WASM and build the web app:
```bash
chmod +x build.sh
./build.sh
```

The output will be in `dist/`.

## Usage

1. Open the web app in a browser
2. Select your TI-84 Plus CE ROM file (.rom)
3. Click "Run" to start emulation
4. Use keyboard controls to interact

### Keyboard Controls

| Key | Function |
|-----|----------|
| 0-9 | Number keys |
| + - * / | Math operations |
| ( ) | Parentheses |
| Enter | Enter |
| Backspace | Delete |
| Arrow keys | Navigation |
| Escape | Clear |
| Shift | 2nd |
| Alt | Alpha |
| O | ON key |
| Ctrl+R / Cmd+R | Resend last program file |

## Architecture

The emulator runs entirely in the browser using WebAssembly:

- **Rust Core** (`../core/`): eZ80 CPU and TI-84 CE hardware emulation
- **WASM Bindings** (`../core/src/wasm.rs`): JavaScript-friendly API via wasm-bindgen
- **React UI** (`src/`): Canvas rendering and keyboard handling

WASM size: ~105KB (42KB gzipped)
