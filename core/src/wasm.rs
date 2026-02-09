//! WebAssembly bindings for the TI-84 Plus CE emulator
//!
//! This module provides JavaScript-friendly APIs using wasm-bindgen.

use wasm_bindgen::prelude::*;
use crate::emu::Emu;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn warn(s: &str);
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
    #[wasm_bindgen(js_namespace = performance)]
    fn now() -> f64;
}

macro_rules! console_warn {
    ($($arg:tt)*) => {
        warn(&format!($($arg)*))
    };
}

/// WASM-friendly wrapper around the emulator.
/// Unlike the C FFI, this owns the emulator directly without mutex
/// since WASM is single-threaded.
#[wasm_bindgen]
pub struct WasmEmu {
    inner: Emu,
}

#[wasm_bindgen]
impl WasmEmu {
    /// Create a new emulator instance.
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmEmu {
        // Set up console panic hook for better error messages
        console_error_panic_hook::set_once();

        WasmEmu {
            inner: Emu::new(),
        }
    }

    /// Load ROM data into the emulator.
    /// Returns 0 on success, negative error code on failure.
    /// Does NOT auto power-on - call power_on() separately.
    #[wasm_bindgen]
    pub fn load_rom(&mut self, data: &[u8]) -> i32 {
        log(&format!("[WASM] load_rom: {} bytes", data.len()));
        match self.inner.load_rom(data) {
            Ok(()) => {
                log("[WASM] load_rom: success");
                0
            }
            Err(code) => {
                warn(&format!("[WASM] load_rom: error {}", code));
                code
            }
        }
    }

    /// Power on the emulator (simulates ON key press).
    #[wasm_bindgen]
    pub fn power_on(&mut self) {
        log("[WASM] power_on");
        self.inner.power_on();
    }

    /// Reset the emulator to initial state.
    #[wasm_bindgen]
    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Run the emulator for the specified number of cycles.
    /// Returns the number of cycles actually executed.
    #[wasm_bindgen]
    pub fn run_cycles(&mut self, cycles: i32) -> i32 {
        if cycles <= 0 {
            return 0;
        }
        let t0 = now();
        let executed = self.inner.run_cycles(cycles as u32) as i32;
        let t1 = now();
        self.inner.render_frame();
        let t2 = now();

        let emu_ms = t1 - t0;
        let render_ms = t2 - t1;

        // Warn if frame takes too long (>50ms means we're eating into frame budget)
        if emu_ms > 50.0 || render_ms > 50.0 {
            console_warn!(
                "[EMU] Slow: emu={:.1}ms render={:.1}ms req={} exec={} halted={} pc={:06X} cycles={}",
                emu_ms, render_ms, cycles, executed, self.inner.is_halted(), self.inner.pc(), self.inner.total_cycles()
            );
        }

        // Diagnostic: warn if executed cycles diverge wildly from requested
        if executed > cycles * 2 || executed < 0 {
            console_warn!(
                "[EMU] run_cycles anomaly: requested={} executed={} halted={} pc={:06X} total_cycles={}",
                cycles, executed, self.inner.is_halted(), self.inner.pc(), self.inner.total_cycles()
            );
        }

        executed
    }

    /// Get diagnostic info for debugging freezes.
    #[wasm_bindgen]
    pub fn debug_status(&self) -> String {
        format!(
            "pc={:06X} halted={} total_cycles={} stop={:?} iff1={} irq={} nmi={}",
            self.inner.pc(),
            self.inner.is_halted(),
            self.inner.total_cycles(),
            self.inner.last_stop_reason(),
            self.inner.iff1(),
            self.inner.irq_pending(),
            self.inner.nmi_pending(),
        )
    }

    /// Get framebuffer width.
    #[wasm_bindgen]
    pub fn framebuffer_width(&self) -> i32 {
        let (width, _) = self.inner.framebuffer_size();
        width as i32
    }

    /// Get framebuffer height.
    #[wasm_bindgen]
    pub fn framebuffer_height(&self) -> i32 {
        let (_, height) = self.inner.framebuffer_size();
        height as i32
    }

    /// Copy framebuffer data to a Uint8ClampedArray for canvas rendering.
    /// Returns RGBA8888 format suitable for ImageData.
    #[wasm_bindgen]
    pub fn get_framebuffer_rgba(&self) -> Vec<u8> {
        let framebuffer = self.inner.framebuffer_data();
        let len = framebuffer.len();

        // Convert from ARGB8888 to RGBA8888 for canvas
        let mut rgba = Vec::with_capacity(len * 4);
        for &argb in framebuffer {
            let a = ((argb >> 24) & 0xFF) as u8;
            let r = ((argb >> 16) & 0xFF) as u8;
            let g = ((argb >> 8) & 0xFF) as u8;
            let b = (argb & 0xFF) as u8;
            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }
        rgba
    }

    /// Set key state.
    /// row: 0-7, col: 0-7
    /// down: true for pressed, false for released
    #[wasm_bindgen]
    pub fn set_key(&mut self, row: i32, col: i32, down: bool) {
        self.inner.set_key(row as usize, col as usize, down);
    }

    /// Get the backlight brightness level (0-255).
    #[wasm_bindgen]
    pub fn get_backlight(&self) -> u8 {
        self.inner.get_backlight()
    }

    /// Check if LCD is on (should display content).
    #[wasm_bindgen]
    pub fn is_lcd_on(&self) -> bool {
        self.inner.is_lcd_on()
    }

    /// Check if device is off (sleeping).
    /// Returns true when the OS has put the device to sleep.
    #[wasm_bindgen]
    pub fn is_device_off(&self) -> bool {
        self.inner.is_off()
    }

    /// Get the size needed for a save state buffer.
    #[wasm_bindgen]
    pub fn save_state_size(&self) -> usize {
        self.inner.save_state_size()
    }

    /// Save emulator state to a byte array.
    /// Returns the state data or an empty array on failure.
    #[wasm_bindgen]
    pub fn save_state(&self) -> Vec<u8> {
        let size = self.inner.save_state_size();
        let mut buffer = vec![0u8; size];
        match self.inner.save_state(&mut buffer) {
            Ok(written) => {
                buffer.truncate(written);
                buffer
            }
            Err(_) => Vec::new(),
        }
    }

    /// Load emulator state from a byte array.
    /// Returns 0 on success, negative error code on failure.
    #[wasm_bindgen]
    pub fn load_state(&mut self, data: &[u8]) -> i32 {
        match self.inner.load_state(data) {
            Ok(()) => 0,
            Err(code) => code,
        }
    }
}

impl Default for WasmEmu {
    fn default() -> Self {
        Self::new()
    }
}
