#!/usr/bin/env node
// Test script to debug WASM emulator issues

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Timeout wrapper
function withTimeout(promise, ms, message) {
  return Promise.race([
    promise,
    new Promise((_, reject) =>
      setTimeout(() => reject(new Error(`Timeout: ${message}`)), ms)
    )
  ]);
}

async function main() {
  const romPath = process.argv[2] || path.join(__dirname, '..', 'TI-84 CE.rom');

  if (!fs.existsSync(romPath)) {
    console.error(`ROM file not found: ${romPath}`);
    process.exit(1);
  }

  console.log('Loading WASM module...');

  // Read the WASM file
  const wasmPath = path.join(__dirname, 'src', 'emu-core', 'emu_core_bg.wasm');
  const wasmBuffer = fs.readFileSync(wasmPath);

  let wasm;

  function getStringFromWasm(ptr, len) {
    if (!wasm) return '<wasm not initialized>';
    try {
      const bytes = new Uint8Array(wasm.memory.buffer, ptr, len);
      return new TextDecoder().decode(bytes);
    } catch (e) {
      return `<error reading string: ${e.message}>`;
    }
  }

  try {
    console.log('Compiling WASM...');
    const wasmModule = await WebAssembly.compile(wasmBuffer);

    const requiredImports = WebAssembly.Module.imports(wasmModule);
    console.log(`Required imports: ${requiredImports.length}`);

    // Build proper imports based on what the module needs
    const actualImports = {};
    for (const imp of requiredImports) {
      if (!actualImports[imp.module]) {
        actualImports[imp.module] = {};
      }
      if (imp.kind === 'function') {
        actualImports[imp.module][imp.name] = function(...args) {
          // For panic/error functions, try to extract the message
          if (imp.name.includes('new_') && args.length >= 2) {
            try {
              const msg = getStringFromWasm(args[0], args[1]);
              console.error('WASM Error:', msg);
            } catch (e) {}
          }
          if (imp.name.includes('throw')) {
            const msg = args.length >= 2 ? getStringFromWasm(args[0], args[1]) : 'unknown';
            throw new Error(`WASM throw: ${msg}`);
          }
          return 0;
        };
      }
    }

    console.log('Instantiating WASM...');
    const instance = await WebAssembly.instantiate(wasmModule, actualImports);
    wasm = instance.exports;

    console.log(`Memory: ${wasm.memory.buffer.byteLength / 1024 / 1024} MB`);

    // Initialize
    console.log('Initializing...');
    if (wasm.__wbindgen_start) {
      wasm.__wbindgen_start();
    }

    console.log('Creating WasmEmu...');
    const emuPtr = wasm.wasmemu_new();
    console.log(`WasmEmu ptr: 0x${emuPtr.toString(16)}`);

    // Load ROM
    console.log(`Loading ROM: ${romPath} (${fs.statSync(romPath).size} bytes)`);
    const romData = fs.readFileSync(romPath);

    const romPtr = wasm.__wbindgen_malloc(romData.length, 1);
    const memView = new Uint8Array(wasm.memory.buffer);
    memView.set(romData, romPtr);

    console.log('Calling load_rom...');
    const loadResult = wasm.wasmemu_load_rom(emuPtr, romPtr, romData.length);
    console.log(`load_rom result: ${loadResult}`);
    wasm.__wbindgen_free(romPtr, romData.length, 1);

    if (loadResult !== 0) {
      console.error('Failed to load ROM');
      process.exit(1);
    }

    console.log(`Memory after ROM: ${wasm.memory.buffer.byteLength / 1024 / 1024} MB`);

    // Power on
    console.log('Calling power_on...');
    wasm.wasmemu_power_on(emuPtr);
    console.log('power_on complete');

    // Run frames
    const cyclesPerFrame = Math.floor(48_000_000 / 60);
    console.log(`\nRunning 5 frames (${cyclesPerFrame} cycles each)...`);

    for (let frame = 0; frame < 5; frame++) {
      console.log(`Frame ${frame}...`);
      const executed = wasm.wasmemu_run_cycles(emuPtr, cyclesPerFrame);
      console.log(`  Executed: ${executed} cycles`);

      const width = wasm.wasmemu_framebuffer_width(emuPtr);
      const height = wasm.wasmemu_framebuffer_height(emuPtr);
      console.log(`  Framebuffer: ${width}x${height}`);

      // Test get_framebuffer_rgba
      console.log('  Getting RGBA...');
      const result = wasm.wasmemu_get_framebuffer_rgba(emuPtr);
      const [ptr, len] = result;
      console.log(`  RGBA: ptr=0x${ptr.toString(16)}, len=${len}`);

      if (ptr && len) {
        // Need to re-get buffer reference after potential growth
        const newView = new Uint8Array(wasm.memory.buffer);
        if (ptr + len <= newView.length) {
          const sample = Array.from(newView.slice(ptr, ptr + 16));
          console.log(`  Sample: [${sample.map(b => b.toString(16).padStart(2, '0')).join(' ')}]`);
        } else {
          console.log(`  Warning: ptr+len (${ptr + len}) > buffer size (${newView.length})`);
        }
        wasm.__wbindgen_free(ptr, len, 1);
      }
    }

    console.log('\nSuccess!');
    process.exit(0);

  } catch (e) {
    console.error('Error:', e.message);
    console.error(e.stack);
    process.exit(1);
  }
}

main();
