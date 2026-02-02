// Common interface for emulator backends

export type BackendType = 'rust' | 'cemu';

export interface EmulatorBackend {
  // Lifecycle
  init(): Promise<void>;
  destroy(): void;

  // ROM loading
  loadRom(data: Uint8Array): Promise<number>;
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

  // Info
  readonly name: string;
  readonly isInitialized: boolean;
  readonly isRomLoaded: boolean;
}
