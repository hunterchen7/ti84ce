// Rust WASM emulator backend

import type { EmulatorBackend } from './types';
import init, { WasmEmu } from '../emu-core/emu_core';

// Singleton promise so concurrent init() calls (e.g. React StrictMode
// double-mount) don't race and corrupt the shared WASM module state.
// On failure, reset so the next attempt retries the WASM load.
let wasmInitPromise: Promise<void> | null = null;

function initWasm(): Promise<void> {
  if (!wasmInitPromise) {
    wasmInitPromise = init().then(() => {}).catch((err) => {
      wasmInitPromise = null;
      throw err;
    });
  }
  return wasmInitPromise!;
}

export class RustBackend implements EmulatorBackend {
  readonly name = 'Rust (Custom)';
  private emu: WasmEmu | null = null;
  private _isInitialized = false;
  private _isRomLoaded = false;

  get isInitialized(): boolean {
    return this._isInitialized;
  }

  get isRomLoaded(): boolean {
    return this._isRomLoaded;
  }

  async init(): Promise<void> {
    await initWasm();
    try {
      this.emu = new WasmEmu();
    } catch (e) {
      // Retry once — handles stale WASM state after HMR or StrictMode
      console.warn('RustBackend: WasmEmu creation failed, retrying:', e);
      await new Promise((r) => setTimeout(r, 0));
      this.emu = new WasmEmu();
    }
    this._isInitialized = true;
  }

  destroy(): void {
    if (this.emu) {
      // Try to free, but handle the case where it's still borrowed
      // This can happen if an animation frame was in progress
      try {
        this.emu.free();
      } catch (e) {
        // Ignore errors during cleanup - the GC will handle it
        console.warn('RustBackend: error during cleanup (safe to ignore):', e);
      }
      this.emu = null;
    }
    this._isInitialized = false;
    this._isRomLoaded = false;
  }

  async loadRom(data: Uint8Array): Promise<number> {
    if (!this.emu) throw new Error('Backend not initialized');
    const result = this.emu.load_rom(data);
    if (result === 0) {
      this._isRomLoaded = true;
    }
    return result;
  }

  powerOn(): void {
    if (!this.emu) throw new Error('Backend not initialized');
    this.emu.power_on();
  }

  reset(): void {
    if (!this.emu) throw new Error('Backend not initialized');
    this.emu.reset();
  }

  runCycles(cycles: number): number {
    if (!this.emu) throw new Error('Backend not initialized');
    return this.emu.run_cycles(cycles);
  }

  runFrame(): void {
    // At 48MHz and 60fps, that's 800,000 cycles per frame
    this.runCycles(800_000);
  }

  getFramebufferWidth(): number {
    if (!this.emu) return 320;
    return this.emu.framebuffer_width();
  }

  getFramebufferHeight(): number {
    if (!this.emu) return 240;
    return this.emu.framebuffer_height();
  }

  getFramebufferRGBA(): Uint8Array {
    if (!this.emu) throw new Error('Backend not initialized');
    return this.emu.get_framebuffer_rgba();
  }

  setKey(row: number, col: number, down: boolean): void {
    if (!this.emu) return;
    this.emu.set_key(row, col, down);
  }

  isLcdOn(): boolean {
    if (!this.emu) return false;
    return this.emu.is_lcd_on();
  }

  saveState(): Uint8Array | null {
    if (!this.emu || !this._isRomLoaded) return null;
    const data = this.emu.save_state();
    return data.length > 0 ? data : null;
  }

  loadState(data: Uint8Array): boolean {
    if (!this.emu) return false;
    const result = this.emu.load_state(data);
    if (result === 0) {
      this._isRomLoaded = true;
      return true;
    }
    return false;
  }
}
