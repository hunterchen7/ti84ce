#!/usr/bin/env node
// Test script for CEmu WASM

import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// CEmu expects these globals
globalThis.emul_is_inited = false;
globalThis.emul_is_paused = true;
globalThis.initFuncs = () => console.log('[JS] initFuncs called');
globalThis.initLCD = () => console.log('[JS] initLCD called');
globalThis.enableGUI = () => console.log('[JS] enableGUI called');
globalThis.disableGUI = () => console.log('[JS] disableGUI called');

async function main() {
  const romPath = process.argv[2] || path.join(__dirname, '..', 'TI-84 CE.rom');

  if (!fs.existsSync(romPath)) {
    console.error(`ROM file not found: ${romPath}`);
    process.exit(1);
  }

  console.log('Loading CEmu WASM module...');

  // Dynamically import the CEmu module
  const cemuPath = path.join(__dirname, 'src', 'cemu-core', 'WebCEmu.js');

  // Import the ES module
  const { default: WebCEmu } = await import(cemuPath);

  console.log('Initializing CEmu...');

  let mainLoopCallback = null;
  let mainLoopFps = 0;

  const Module = await WebCEmu({
    print: (text) => console.log('[CEmu]', text),
    printErr: (text) => console.error('[CEmu ERR]', text),
    locateFile: (path) => {
      if (path.endsWith('.wasm')) {
        return new URL('./src/cemu-core/WebCEmu.wasm', import.meta.url).pathname;
      }
      return path;
    },
    // Prevent Emscripten from entering the main loop
    noExitRuntime: true,
  });

  console.log('CEmu Module loaded!');

  // List available exports
  const funcs = Object.keys(Module).filter(k => typeof Module[k] === 'function');
  console.log(`\nExported functions (${funcs.length}):`, funcs.slice(0, 30).join(', '));

  // Check for FS
  if (!Module.FS) {
    console.error('FS not available!');
    process.exit(1);
  }

  // Load ROM into virtual filesystem as "CE.rom" (what CEmu expects)
  console.log(`\nLoading ROM: ${romPath}`);
  const romData = fs.readFileSync(romPath);
  console.log(`ROM size: ${romData.length} bytes`);

  // Write ROM to virtual FS with the name CEmu expects
  Module.FS.writeFile('/CE.rom', romData);
  console.log('ROM written to /CE.rom');

  // Change to root directory
  Module.FS.chdir('/');

  // List files
  console.log('Virtual FS root:', Module.FS.readdir('/'));

  // Initialize emulator using our custom init function (not callMain which has main loop issues)
  console.log('\nInitializing emulator...');
  if (Module._emu_init) {
    // Allocate string for ROM path and call init
    const romPathWasm = '/CE.rom';
    const romPathBytes = new TextEncoder().encode(romPathWasm + '\0');
    const romPathPtr = Module._malloc(romPathBytes.length);
    Module.HEAPU8.set(romPathBytes, romPathPtr);

    const initResult = Module._emu_init(romPathPtr);
    Module._free(romPathPtr);

    if (initResult === 0) {
      console.log('Emulator initialized successfully!');
    } else {
      console.error('Emulator init failed:', initResult);
      process.exit(1);
    }
  } else {
    console.error('_emu_init not available, falling back to callMain...');
    try {
      const result = Module.callMain([]);
      console.log('main() returned:', result);
    } catch (e) {
      if (e.message && e.message.includes('main loop')) {
        console.log('main() entered main loop (expected)');
      } else {
        console.error('main() error:', e.message);
        throw e;
      }
    }
  }

  // Check if emu_step is available
  if (Module._emu_step) {
    console.log('\n_emu_step is available - running emulator...');

    // Run many frames to let the calculator boot
    const totalFrames = 3000; // ~50 seconds at 60fps
    const batchSize = 300; // Report every 5 seconds

    console.log(`Running ${totalFrames} frames to boot calculator...`);
    for (let i = 0; i < totalFrames; i += batchSize) {
      const frames = Math.min(batchSize, totalFrames - i);
      process.stdout.write(`Frames ${i}-${i + frames}...`);
      try {
        Module._emu_step(frames);
        console.log(' done');
      } catch (e) {
        console.error(` error: ${e.message}`);
        break;
      }
    }
    console.log(`Completed running frames.`);
  } else {
    console.log('_emu_step not available');
  }

  // Check if lcd_get_frame is available
  if (Module._lcd_get_frame) {
    console.log('\n_lcd_get_frame is available');
    const framePtr = Module._lcd_get_frame();
    console.log(`Frame pointer: 0x${framePtr.toString(16)}`);

    // The frame is in panel.display which is 320x240 uint32_t
    if (framePtr && Module.HEAPU8) {
      const sample = [];
      for (let i = 0; i < 16; i++) {
        sample.push(Module.HEAPU8[framePtr + i].toString(16).padStart(2, '0'));
      }
      console.log(`First 16 bytes: [${sample.join(' ')}]`);

      // Count different pixel colors
      const HEAPU32 = new Uint32Array(Module.HEAPU8.buffer, framePtr, 320 * 240);
      const colorCounts = new Map();
      for (let i = 0; i < 320 * 240; i++) {
        const color = HEAPU32[i];
        colorCounts.set(color, (colorCounts.get(color) || 0) + 1);
      }

      console.log(`Unique colors: ${colorCounts.size}`);

      // Show top 5 most common colors
      const sorted = [...colorCounts.entries()].sort((a, b) => b[1] - a[1]);
      console.log('Top 5 colors:');
      for (let i = 0; i < Math.min(5, sorted.length); i++) {
        const [color, count] = sorted[i];
        const hex = color.toString(16).padStart(8, '0');
        console.log(`  0x${hex}: ${count} pixels (${(count / 76800 * 100).toFixed(1)}%)`);
      }

      // Save framebuffer as PPM (simple image format)
      const width = 320;
      const height = 240;
      const ppmPath = path.join(__dirname, 'cemu-screen.ppm');
      let ppmData = `P3\n${width} ${height}\n255\n`;
      for (let y = 0; y < height; y++) {
        for (let x = 0; x < width; x++) {
          const pixelIdx = y * width + x;
          const color = HEAPU32[pixelIdx];
          // ABGR format from CEmu -> RGB
          const r = (color >> 0) & 0xFF;
          const g = (color >> 8) & 0xFF;
          const b = (color >> 16) & 0xFF;
          ppmData += `${r} ${g} ${b} `;
        }
        ppmData += '\n';
      }
      fs.writeFileSync(ppmPath, ppmData);
      console.log(`\nSaved framebuffer to: ${ppmPath}`);
    } else {
      console.log('HEAPU8 not available:', !!Module.HEAPU8);
    }
  } else {
    console.log('_lcd_get_frame not available');
  }

  // Check for sendKey
  if (Module._sendKey) {
    console.log('\n_sendKey is available');
  }

  console.log('\nDone!');
}

main().catch(e => {
  console.error('Error:', e);
  process.exit(1);
});
