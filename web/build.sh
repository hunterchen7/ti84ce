#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"

echo "Building WASM package..."
cd "$ROOT_DIR/core"
wasm-pack build --target web --release

echo "Copying WASM package to web app..."
rm -rf "$SCRIPT_DIR/src/emu-core"
cp -r "$ROOT_DIR/core/pkg" "$SCRIPT_DIR/src/emu-core"

echo "Building web app..."
cd "$SCRIPT_DIR"
npm run build

echo "Done! Output in dist/"
