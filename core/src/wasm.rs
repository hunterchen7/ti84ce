//! WebAssembly bindings for the TI-84 Plus CE emulator
//!
//! This module provides JavaScript-friendly APIs using wasm-bindgen.

// Use wee_alloc as the global allocator for smaller code size and better WASM support
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

use wasm_bindgen::prelude::*;
use crate::emu::Emu;

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
    #[wasm_bindgen]
    pub fn load_rom(&mut self, data: &[u8]) -> i32 {
        match self.inner.load_rom(data) {
            Ok(()) => 0,
            Err(code) => code,
        }
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
        let executed = self.inner.run_cycles(cycles as u32) as i32;
        self.inner.render_frame();
        executed
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
        let (width, height) = self.inner.framebuffer_size();
        let ptr = self.inner.framebuffer_ptr();
        let len = width * height;

        // Convert from ARGB8888 to RGBA8888 for canvas
        let mut rgba = Vec::with_capacity(len * 4);
        for i in 0..len {
            let argb = unsafe { *ptr.add(i) };
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
