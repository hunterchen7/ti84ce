// Type declarations for CEmu WASM module

interface CEmuFS {
  writeFile(path: string, data: Uint8Array): void;
  readFile(path: string): Uint8Array;
  readdir(path: string): string[];
  chdir(path: string): void;
  mkdir(path: string): void;
  unlink(path: string): void;
}

interface CEmuModuleConfig {
  print?: (text: string) => void;
  printErr?: (text: string) => void;
  locateFile?: (path: string) => string;
  noExitRuntime?: boolean;
}

interface CEmuModule {
  FS: CEmuFS;
  HEAPU8: Uint8Array;
  HEAPU32: Uint32Array;
  _malloc(size: number): number;
  _free(ptr: number): void;
  _emu_init(romPathPtr: number): number;
  _emu_step(frames: number): void;
  _emu_reset(): void;
  _lcd_get_frame(): number;
  _sendKey(keyCode: number): void;
  callMain(args: string[]): number;
  ccall(name: string, returnType: string, argTypes: string[], args: unknown[]): unknown;
  cwrap(name: string, returnType: string, argTypes: string[]): (...args: unknown[]) => unknown;
}

declare function WebCEmu(config?: CEmuModuleConfig): Promise<CEmuModule>;

export default WebCEmu;
export type { CEmuModule, CEmuModuleConfig, CEmuFS };
