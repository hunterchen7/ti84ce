/* @ts-self-types="./emu_core.d.ts" */

/**
 * WASM-friendly wrapper around the emulator.
 * Unlike the C FFI, this owns the emulator directly without mutex
 * since WASM is single-threaded.
 */
export class WasmEmu {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        WasmEmuFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_wasmemu_free(ptr, 0);
    }
    /**
     * Get diagnostic info for debugging freezes.
     * @returns {string}
     */
    debug_status() {
        let deferred1_0;
        let deferred1_1;
        try {
            const ret = wasm.wasmemu_debug_status(this.__wbg_ptr);
            deferred1_0 = ret[0];
            deferred1_1 = ret[1];
            return getStringFromWasm0(ret[0], ret[1]);
        } finally {
            wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
        }
    }
    /**
     * Get framebuffer height.
     * @returns {number}
     */
    framebuffer_height() {
        const ret = wasm.wasmemu_framebuffer_height(this.__wbg_ptr);
        return ret;
    }
    /**
     * Get framebuffer width.
     * @returns {number}
     */
    framebuffer_width() {
        const ret = wasm.wasmemu_framebuffer_width(this.__wbg_ptr);
        return ret;
    }
    /**
     * Get the backlight brightness level (0-255).
     * @returns {number}
     */
    get_backlight() {
        const ret = wasm.wasmemu_get_backlight(this.__wbg_ptr);
        return ret;
    }
    /**
     * Copy framebuffer data to a Uint8ClampedArray for canvas rendering.
     * Returns RGBA8888 format suitable for ImageData.
     * @returns {Uint8Array}
     */
    get_framebuffer_rgba() {
        const ret = wasm.wasmemu_get_framebuffer_rgba(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Check if device is off (sleeping).
     * Returns true when the OS has put the device to sleep.
     * @returns {boolean}
     */
    is_device_off() {
        const ret = wasm.wasmemu_is_device_off(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Check if LCD is on (should display content).
     * @returns {boolean}
     */
    is_lcd_on() {
        const ret = wasm.wasmemu_is_lcd_on(this.__wbg_ptr);
        return ret !== 0;
    }
    /**
     * Load ROM data into the emulator.
     * Returns 0 on success, negative error code on failure.
     * Does NOT auto power-on - call power_on() separately.
     * @param {Uint8Array} data
     * @returns {number}
     */
    load_rom(data) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmemu_load_rom(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * Load emulator state from a byte array.
     * Returns 0 on success, negative error code on failure.
     * @param {Uint8Array} data
     * @returns {number}
     */
    load_state(data) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmemu_load_state(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * Create a new emulator instance.
     */
    constructor() {
        const ret = wasm.wasmemu_new();
        this.__wbg_ptr = ret >>> 0;
        WasmEmuFinalization.register(this, this.__wbg_ptr, this);
        return this;
    }
    /**
     * Power on the emulator (simulates ON key press).
     */
    power_on() {
        wasm.wasmemu_power_on(this.__wbg_ptr);
    }
    /**
     * Reset the emulator to initial state.
     */
    reset() {
        wasm.wasmemu_reset(this.__wbg_ptr);
    }
    /**
     * Run the emulator for the specified number of cycles.
     * Returns the number of cycles actually executed.
     * @param {number} cycles
     * @returns {number}
     */
    run_cycles(cycles) {
        const ret = wasm.wasmemu_run_cycles(this.__wbg_ptr, cycles);
        return ret;
    }
    /**
     * Save emulator state to a byte array.
     * Returns the state data or an empty array on failure.
     * @returns {Uint8Array}
     */
    save_state() {
        const ret = wasm.wasmemu_save_state(this.__wbg_ptr);
        var v1 = getArrayU8FromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
        return v1;
    }
    /**
     * Get the size needed for a save state buffer.
     * @returns {number}
     */
    save_state_size() {
        const ret = wasm.wasmemu_save_state_size(this.__wbg_ptr);
        return ret >>> 0;
    }
    /**
     * Send a .8xp/.8xv file to be injected into flash archive.
     * Must be called after load_rom() and before power_on().
     * Returns number of entries injected (>=0), or negative error code.
     * @param {Uint8Array} data
     * @returns {number}
     */
    send_file(data) {
        const ptr0 = passArray8ToWasm0(data, wasm.__wbindgen_malloc);
        const len0 = WASM_VECTOR_LEN;
        const ret = wasm.wasmemu_send_file(this.__wbg_ptr, ptr0, len0);
        return ret;
    }
    /**
     * Set key state.
     * row: 0-7, col: 0-7
     * down: true for pressed, false for released
     * @param {number} row
     * @param {number} col
     * @param {boolean} down
     */
    set_key(row, col, down) {
        wasm.wasmemu_set_key(this.__wbg_ptr, row, col, down);
    }
}
if (Symbol.dispose) WasmEmu.prototype[Symbol.dispose] = WasmEmu.prototype.free;

function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg___wbindgen_throw_be289d5034ed271b: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_error_7534b8e9a36f1ab4: function(arg0, arg1) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.error(getStringFromWasm0(arg0, arg1));
            } finally {
                wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
            }
        },
        __wbg_log_8161e84ca5c180b5: function(arg0, arg1) {
            console.log(getStringFromWasm0(arg0, arg1));
        },
        __wbg_new_8a6f238a6ece86ea: function() {
            const ret = new Error();
            return ret;
        },
        __wbg_now_2f3496ca767ee9ef: function() {
            const ret = performance.now();
            return ret;
        },
        __wbg_stack_0ed75d68575b0f3c: function(arg0, arg1) {
            const ret = arg1.stack;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg_warn_69b069cf4bf37f1f: function(arg0, arg1) {
            console.warn(getStringFromWasm0(arg0, arg1));
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            const offset = table.grow(4);
            table.set(0, undefined);
            table.set(offset + 0, undefined);
            table.set(offset + 1, null);
            table.set(offset + 2, true);
            table.set(offset + 3, false);
        },
    };
    return {
        __proto__: null,
        "./emu_core_bg.js": import0,
    };
}

const WasmEmuFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_wasmemu_free(ptr >>> 0, 1));

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasm;
function __wbg_finalize_init(instance, module) {
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('emu_core_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
