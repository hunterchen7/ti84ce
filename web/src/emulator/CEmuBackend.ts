// CEmu WASM emulator backend

import type { EmulatorBackend } from './types';

// CEmu Module type
interface CEmuModule {
  FS: {
    writeFile(path: string, data: Uint8Array): void;
    readdir(path: string): string[];
    chdir(path: string): void;
    mkdir(path: string): void;
  };
  HEAPU8: Uint8Array;
  HEAPU32: Uint32Array;
  _malloc(size: number): number;
  _free(ptr: number): void;
  _emu_init(romPathPtr: number): number;
  _emu_step(frames: number): void;
  _emu_reset(): void;
  _lcd_get_frame(): number;
  _emu_keypad_event(row: number, col: number, press: boolean): void;
  _emu_save_state_size(): number;
  _emu_save_state(bufferPtr: number, bufferSize: number): number;
  _emu_load_state(bufferPtr: number, size: number): number;
}

export class CEmuBackend implements EmulatorBackend {
  readonly name = 'CEmu (Reference)';
  private module: CEmuModule | null = null;
  private _isInitialized = false;
  private _isRomLoaded = false;

  get isInitialized(): boolean {
    return this._isInitialized;
  }

  get isRomLoaded(): boolean {
    return this._isRomLoaded;
  }

  async init(): Promise<void> {
    // Set up globals that CEmu expects
    (globalThis as any).emul_is_inited = false;
    (globalThis as any).emul_is_paused = true;
    (globalThis as any).initFuncs = () => {};
    (globalThis as any).initLCD = () => {};
    (globalThis as any).enableGUI = () => {};
    (globalThis as any).disableGUI = () => {};

    // Dynamically import CEmu module
    const { default: WebCEmu } = await import('../cemu-core/WebCEmu.js');

    this.module = await WebCEmu({
      print: (text: string) => console.log('[CEmu]', text),
      printErr: (text: string) => console.error('[CEmu]', text),
      locateFile: (path: string) => {
        if (path.endsWith('.wasm')) {
          return new URL('../cemu-core/WebCEmu.wasm', import.meta.url).href;
        }
        return path;
      },
      noExitRuntime: true,
    }) as CEmuModule;

    // Create /tmp directory for state file operations
    try {
      this.module.FS.mkdir('/tmp');
    } catch {
      // Directory may already exist
    }

    this._isInitialized = true;
  }

  destroy(): void {
    this.module = null;
    this._isInitialized = false;
    this._isRomLoaded = false;
  }

  async loadRom(data: Uint8Array): Promise<number> {
    if (!this.module) throw new Error('Backend not initialized');

    // Write ROM to virtual filesystem
    this.module.FS.writeFile('/CE.rom', data);
    this.module.FS.chdir('/');

    // Initialize emulator with ROM
    const romPath = '/CE.rom';
    const romPathBytes = new TextEncoder().encode(romPath + '\0');
    const romPathPtr = this.module._malloc(romPathBytes.length);
    this.module.HEAPU8.set(romPathBytes, romPathPtr);

    const result = this.module._emu_init(romPathPtr);
    this.module._free(romPathPtr);

    if (result === 0) {
      this._isRomLoaded = true;
    }

    return result;
  }

  sendFile(_data: Uint8Array): number {
    // TODO: CEmu backend doesn't support send_file yet
    console.warn('[CEmu] sendFile not implemented for CEmu backend');
    return -1;
  }

  sendFileLive(_data: Uint8Array): number {
    // TODO: CEmu backend doesn't support send_file_live yet
    console.warn('[CEmu] sendFileLive not implemented for CEmu backend');
    return -1;
  }

  powerOn(): void {
    // CEmu handles power on during init/reset
  }

  reset(): void {
    if (!this.module) throw new Error('Backend not initialized');
    this.module._emu_reset();
  }

  runCycles(_cycles: number): number {
    // CEmu uses frame-based stepping, not cycle-based
    // Run approximately 1 frame worth
    this.runFrame();
    return _cycles;
  }

  runFrame(): void {
    if (!this.module) throw new Error('Backend not initialized');
    this.module._emu_step(1);
  }

  getFramebufferWidth(): number {
    return 320;
  }

  getFramebufferHeight(): number {
    return 240;
  }

  getFramebufferRGBA(): Uint8Array {
    if (!this.module) throw new Error('Backend not initialized');

    const framePtr = this.module._lcd_get_frame();
    if (!framePtr) {
      return new Uint8Array(320 * 240 * 4);
    }

    // CEmu panel.display is 320x240 ARGB8888 (A=bits 31-24, R=23-16, G=15-8, B=7-0)
    const width = 320;
    const height = 240;
    const result = new Uint8Array(width * height * 4);

    // Convert from CEmu's ARGB8888 to canvas RGBA
    const heapu32 = new Uint32Array(this.module.HEAPU8.buffer, framePtr, width * height);
    for (let i = 0; i < width * height; i++) {
      const pixel = heapu32[i];
      result[i * 4 + 0] = (pixel >> 16) & 0xFF; // R
      result[i * 4 + 1] = (pixel >> 8) & 0xFF;  // G
      result[i * 4 + 2] = (pixel >> 0) & 0xFF;  // B
      result[i * 4 + 3] = 255; // A (always opaque)
    }

    return result;
  }

  setKey(row: number, col: number, down: boolean): void {
    if (!this.module) return;
    // Use emu_keypad_event which takes row, col directly
    this.module._emu_keypad_event(row, col, down);
  }

  isLcdOn(): boolean {
    // CEmu backend doesn't expose LCD state yet — assume on
    return true;
  }

  saveState(): Uint8Array | null {
    if (!this.module || !this._isRomLoaded) return null;

    const bufferSize = this.module._emu_save_state_size();
    const bufferPtr = this.module._malloc(bufferSize);

    try {
      const result = this.module._emu_save_state(bufferPtr, bufferSize);
      if (result <= 0) {
        console.error('[CEmu] Failed to save state:', result);
        return null;
      }

      // Copy data from WASM memory
      const stateData = new Uint8Array(result);
      stateData.set(this.module.HEAPU8.subarray(bufferPtr, bufferPtr + result));
      return stateData;
    } finally {
      this.module._free(bufferPtr);
    }
  }

  loadState(data: Uint8Array): boolean {
    if (!this.module) return false;

    const bufferPtr = this.module._malloc(data.length);

    try {
      // Copy data to WASM memory
      this.module.HEAPU8.set(data, bufferPtr);

      const result = this.module._emu_load_state(bufferPtr, data.length);
      if (result !== 0) {
        console.error('[CEmu] Failed to load state:', result);
        return false;
      }

      this._isRomLoaded = true;
      return true;
    } finally {
      this.module._free(bufferPtr);
    }
  }
}
