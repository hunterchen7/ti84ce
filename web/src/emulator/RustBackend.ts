// Rust WASM emulator backend

import type { EmulatorBackend } from './types';
import init, { WasmEmu } from '../emu-core/emu_core';

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
    await init();
    this.emu = new WasmEmu();
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
}
