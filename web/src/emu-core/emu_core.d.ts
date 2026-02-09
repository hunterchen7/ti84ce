/* tslint:disable */
/* eslint-disable */

/**
 * WASM-friendly wrapper around the emulator.
 * Unlike the C FFI, this owns the emulator directly without mutex
 * since WASM is single-threaded.
 */
export class WasmEmu {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Get diagnostic info for debugging freezes.
     */
    debug_status(): string;
    /**
     * Get framebuffer height.
     */
    framebuffer_height(): number;
    /**
     * Get framebuffer width.
     */
    framebuffer_width(): number;
    /**
     * Get the backlight brightness level (0-255).
     */
    get_backlight(): number;
    /**
     * Copy framebuffer data to a Uint8ClampedArray for canvas rendering.
     * Returns RGBA8888 format suitable for ImageData.
     */
    get_framebuffer_rgba(): Uint8Array;
    /**
     * Check if device is off (sleeping).
     * Returns true when the OS has put the device to sleep.
     */
    is_device_off(): boolean;
    /**
     * Check if LCD is on (should display content).
     */
    is_lcd_on(): boolean;
    /**
     * Load ROM data into the emulator.
     * Returns 0 on success, negative error code on failure.
     * Does NOT auto power-on - call power_on() separately.
     */
    load_rom(data: Uint8Array): number;
    /**
     * Load emulator state from a byte array.
     * Returns 0 on success, negative error code on failure.
     */
    load_state(data: Uint8Array): number;
    /**
     * Create a new emulator instance.
     */
    constructor();
    /**
     * Power on the emulator (simulates ON key press).
     */
    power_on(): void;
    /**
     * Reset the emulator to initial state.
     */
    reset(): void;
    /**
     * Run the emulator for the specified number of cycles.
     * Returns the number of cycles actually executed.
     */
    run_cycles(cycles: number): number;
    /**
     * Save emulator state to a byte array.
     * Returns the state data or an empty array on failure.
     */
    save_state(): Uint8Array;
    /**
     * Get the size needed for a save state buffer.
     */
    save_state_size(): number;
    /**
     * Set key state.
     * row: 0-7, col: 0-7
     * down: true for pressed, false for released
     */
    set_key(row: number, col: number, down: boolean): void;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly emu_backend_count: () => number;
    readonly emu_backend_get_available: () => number;
    readonly emu_backend_set: (a: number) => number;
    readonly emu_create: () => number;
    readonly emu_destroy: (a: number) => void;
    readonly emu_framebuffer: (a: number, b: number, c: number) => number;
    readonly emu_get_backlight: (a: number) => number;
    readonly emu_is_lcd_on: (a: number) => number;
    readonly emu_load_rom: (a: number, b: number, c: number) => number;
    readonly emu_load_state: (a: number, b: number, c: number) => number;
    readonly emu_power_on: (a: number) => void;
    readonly emu_reset: (a: number) => void;
    readonly emu_run_cycles: (a: number, b: number) => number;
    readonly emu_save_state: (a: number, b: number, c: number) => number;
    readonly emu_save_state_size: (a: number) => number;
    readonly emu_set_key: (a: number, b: number, c: number, d: number) => void;
    readonly emu_set_log_callback: (a: number) => void;
    readonly emu_backend_get_current: () => number;
    readonly __wbg_wasmemu_free: (a: number, b: number) => void;
    readonly wasmemu_debug_status: (a: number) => [number, number];
    readonly wasmemu_framebuffer_height: (a: number) => number;
    readonly wasmemu_framebuffer_width: (a: number) => number;
    readonly wasmemu_get_backlight: (a: number) => number;
    readonly wasmemu_get_framebuffer_rgba: (a: number) => [number, number];
    readonly wasmemu_is_device_off: (a: number) => number;
    readonly wasmemu_is_lcd_on: (a: number) => number;
    readonly wasmemu_load_rom: (a: number, b: number, c: number) => number;
    readonly wasmemu_load_state: (a: number, b: number, c: number) => number;
    readonly wasmemu_new: () => number;
    readonly wasmemu_power_on: (a: number) => void;
    readonly wasmemu_reset: (a: number) => void;
    readonly wasmemu_run_cycles: (a: number, b: number) => number;
    readonly wasmemu_save_state: (a: number) => [number, number];
    readonly wasmemu_save_state_size: (a: number) => number;
    readonly wasmemu_set_key: (a: number, b: number, c: number, d: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
