// Common interface for emulator backends

export type BackendType = 'rust' | 'cemu';

export interface EmulatorBackend {
  // Lifecycle
  init(): Promise<void>;
  destroy(): void;

  // ROM loading
  loadRom(data: Uint8Array): Promise<number>;
  sendFile(data: Uint8Array): number;
  sendFileLive(data: Uint8Array): number;
  powerOn(): void;
  reset(): void;

  // Execution
  runCycles(cycles: number): number;
  runFrame(): void;

  // Display
  getFramebufferWidth(): number;
  getFramebufferHeight(): number;
  getFramebufferRGBA(): Uint8Array;

  // Input
  setKey(row: number, col: number, down: boolean): void;

  // State persistence
  saveState(): Uint8Array | null;
  loadState(data: Uint8Array): boolean;

  // State queries
  isLcdOn(): boolean;

  // Info
  readonly name: string;
  readonly isInitialized: boolean;
  readonly isRomLoaded: boolean;
}
