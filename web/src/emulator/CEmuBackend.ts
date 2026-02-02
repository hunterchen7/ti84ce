// CEmu WASM emulator backend

import type { EmulatorBackend } from './types';

// CEmu Module type
interface CEmuModule {
  FS: {
    writeFile(path: string, data: Uint8Array): void;
    readdir(path: string): string[];
    chdir(path: string): void;
  };
  HEAPU8: Uint8Array;
  HEAPU32: Uint32Array;
  _malloc(size: number): number;
  _free(ptr: number): void;
  _emu_init(romPathPtr: number): number;
  _emu_step(frames: number): void;
  _emu_reset(): void;
  _lcd_get_frame(): number;
  _sendKey(keyCode: number): void;
}

// TI-84 CE key codes for CEmu (different from row/col format)
// CEmu uses a different key encoding - we need to map row/col to CEmu key codes
const ROW_COL_TO_CEMU_KEY: Record<string, number> = {
  // Format: "row,col" -> CEmu key code
  // These need to be verified against CEmu's keypad.h
  '0,0': 0x21, // Graph
  '0,1': 0x22, // Trace
  '0,2': 0x23, // Zoom
  '0,3': 0x24, // Window
  '0,4': 0x25, // Y=
  '0,5': 0x26, // 2nd
  '0,6': 0x27, // Mode
  '0,7': 0x28, // Del
  '1,0': 0x11, // Sto
  '1,1': 0x12, // Ln
  '1,2': 0x13, // Log
  '1,3': 0x14, // x²
  '1,4': 0x15, // x⁻¹
  '1,5': 0x16, // Math
  '1,6': 0x17, // Alpha
  '1,7': 0x18, // X,T,θ,n
  '2,0': 0x40, // 0
  '2,1': 0x41, // 1
  '2,2': 0x42, // 4
  '2,3': 0x43, // 7
  '2,4': 0x44, // ,
  '2,5': 0x45, // Sin
  '2,6': 0x46, // Apps
  '2,7': 0x47, // Stat (changed from X,T,θ,n)
  '3,0': 0x30, // .
  '3,1': 0x31, // 2
  '3,2': 0x32, // 5
  '3,3': 0x33, // 8
  '3,4': 0x34, // (
  '3,5': 0x35, // Cos
  '3,6': 0x36, // Prgm
  '3,7': 0x37, // Vars (changed from Stat)
  '4,0': 0x50, // (-)
  '4,1': 0x51, // 3
  '4,2': 0x52, // 6
  '4,3': 0x53, // 9
  '4,4': 0x54, // )
  '4,5': 0x55, // Tan
  '4,6': 0x56, // ×
  '4,7': 0x57, // ^
  '5,0': 0x09, // Enter
  '5,1': 0x5A, // +
  '5,2': 0x5B, // -
  '5,3': 0x5C, // *
  '5,4': 0x5D, // /
  '5,5': 0x5E, // Clear
  '5,6': 0x00, // Down
  '5,7': 0x02, // Right
  '6,0': 0x03, // Up
  '6,1': 0x01, // Left
  '6,5': 0x29, // ON
};

export class CEmuBackend implements EmulatorBackend {
  readonly name = 'CEmu (Reference)';
  private module: CEmuModule | null = null;
  private _isInitialized = false;
  private _isRomLoaded = false;
  private romData: Uint8Array | null = null;

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

    this._isInitialized = true;
  }

  destroy(): void {
    this.module = null;
    this._isInitialized = false;
    this._isRomLoaded = false;
    this.romData = null;
  }

  async loadRom(data: Uint8Array): Promise<number> {
    if (!this.module) throw new Error('Backend not initialized');

    this.romData = data;

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

    // CEmu framebuffer is 320x240 RGBA (but may be in different order)
    const width = 320;
    const height = 240;
    const result = new Uint8Array(width * height * 4);

    // Copy and convert from CEmu's format (ABGR) to RGBA
    const heapu32 = new Uint32Array(this.module.HEAPU8.buffer, framePtr, width * height);
    for (let i = 0; i < width * height; i++) {
      const pixel = heapu32[i];
      // CEmu uses ABGR format, we need RGBA
      result[i * 4 + 0] = (pixel >> 0) & 0xFF;  // R (from B position in ABGR)
      result[i * 4 + 1] = (pixel >> 8) & 0xFF;  // G
      result[i * 4 + 2] = (pixel >> 16) & 0xFF; // B (from R position in ABGR)
      result[i * 4 + 3] = 255; // A (always opaque)
    }

    return result;
  }

  setKey(row: number, col: number, down: boolean): void {
    if (!this.module) return;

    // CEmu uses sendKey with a key code
    // For now, we'll use the basic key event function
    // This needs proper mapping - for now just log it
    const key = `${row},${col}`;
    const keyCode = ROW_COL_TO_CEMU_KEY[key];

    if (keyCode !== undefined) {
      // CEmu's _sendKey sends a key press (press and release)
      // For key down/up we might need different handling
      // For now, only send on key down
      if (down) {
        this.module._sendKey(keyCode);
      }
    }
  }
}
