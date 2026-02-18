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
}

/// WASM-friendly wrapper around the emulator.
/// Unlike the C FFI, this owns the emulator directly without mutex
/// since WASM is single-threaded.
#[wasm_bindgen]
pub struct WasmEmu {
    inner: Emu,
    /// Counter for debug logging after state restore
    debug_frames: u32,
    /// Track last PC to detect resets
    last_pc: u32,
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
            debug_frames: 0,
            last_pc: 0,
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

    /// Send a .8xp/.8xv file to be injected into flash archive.
    /// Must be called after load_rom() and before power_on().
    /// Returns number of entries injected (>=0), or negative error code.
    #[wasm_bindgen]
    pub fn send_file(&mut self, data: &[u8]) -> i32 {
        log(&format!("[WASM] send_file: {} bytes", data.len()));
        match self.inner.send_file(data) {
            Ok(count) => {
                log(&format!("[WASM] send_file: injected {} entries", count));
                count as i32
            }
            Err(code) => {
                warn(&format!("[WASM] send_file: error {}", code));
                code
            }
        }
    }

    /// Send a file to the running emulator (live/hot reload).
    /// Injects into flash archive, invalidating any existing copy,
    /// then performs a soft reset so the OS discovers the new program.
    /// Returns number of entries injected (>=0), or negative error code.
    #[wasm_bindgen]
    pub fn send_file_live(&mut self, data: &[u8]) -> i32 {
        log(&format!("[WASM] send_file_live: {} bytes", data.len()));
        match self.inner.send_file_live(data) {
            Ok(count) => {
                log(&format!("[WASM] send_file_live: injected {} entries, soft reset done", count));
                count as i32
            }
            Err(code) => {
                warn(&format!("[WASM] send_file_live: error {}", code));
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
        let before_pc = self.inner.pc();
        let before_cycles = self.inner.total_cycles();
        let executed = self.inner.run_cycles(cycles as u32) as i32;
        self.inner.render_frame();

        // Log first 5 frames after state restore, and any anomalies
        if self.debug_frames > 0 {
            self.debug_frames -= 1;
            log(&format!(
                "[EMU] frame: pc={:06X}->{:06X} cycles={}->{}(+{}) halted={} nmi={} req={}",
                before_pc, self.inner.pc(),
                before_cycles, self.inner.total_cycles(), executed,
                self.inner.is_halted(), self.inner.nmi_pending(), cycles
            ));
        }

        // Detect anomalies
        if executed > cycles * 2 || executed < 0 {
            warn(&format!(
                "[EMU] ANOMALY: req={} exec={} pc={:06X} halted={}",
                cycles, executed, self.inner.pc(), self.inner.is_halted()
            ));
        }

        // Check if NMI fired during this frame
        let (nmi_count, nmi_pc, nmi_sp, vaddr, vpc) = self.inner.take_nmi_log();
        if nmi_count > 0 {
            warn(&format!(
                "[EMU] NMI fired {}x! pc={:06X} sp={:06X} write_addr={:06X} raw_pc={:06X} privileged={:06X} prot={:06X}-{:06X} stack_limit={:06X}",
                nmi_count, nmi_pc, nmi_sp, vaddr, vpc,
                self.inner.privileged_boundary(),
                self.inner.protected_start(),
                self.inner.protected_end(),
                self.inner.stack_limit(),
            ));
        }

        // Detect PC jump to reset vector (CPU was reset)
        let new_pc = self.inner.pc();
        if self.last_pc > 0x010000 && new_pc < 0x000100 {
            warn(&format!(
                "[EMU] RESET DETECTED: pc {:06X}->{:06X} nmi={} halted={}",
                self.last_pc, new_pc, self.inner.nmi_pending(), self.inner.is_halted()
            ));
        }
        self.last_pc = new_pc;

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
        log(&format!("[WASM] load_state: {} bytes", data.len()));
        match self.inner.load_state(data) {
            Ok(()) => {
                self.debug_frames = 10; // Log next 10 frames
                log(&format!(
                    "[WASM] load_state OK: pc={:06X} halted={} total_cycles={} lcd_on={} off={}",
                    self.inner.pc(), self.inner.is_halted(), self.inner.total_cycles(),
                    self.inner.is_lcd_on(), self.inner.is_off()
                ));
                log(&format!(
                    "[WASM] state details: iff1={} im={} sp={:06X} stack_limit={:06X} privileged={:06X} prot={:06X}-{:06X}",
                    self.inner.iff1(),
                    self.inner.im(),
                    self.inner.sp(),
                    self.inner.stack_limit(),
                    self.inner.privileged_boundary(),
                    self.inner.protected_start(),
                    self.inner.protected_end(),
                ));
                log(&format!(
                    "[WASM] scheduler: cpu_speed={} base_ticks={} bus_cycles={}",
                    self.inner.cpu_speed(),
                    self.inner.scheduler_base_ticks(),
                    self.inner.bus_cycles(),
                ));
                0
            }
            Err(code) => {
                warn(&format!("[WASM] load_state FAILED: error {}", code));
                code
            }
        }
    }

    /// Dump diagnostic state for debugging.
    #[wasm_bindgen]
    pub fn dump_state(&self) -> String {
        format!(
            "pc={:06X} sp={:06X} halted={} iff1={} im={} cycles={} lcd_on={} off={} \
             stack_limit={:06X} prot_start={:06X} prot_end={:06X} cpu_speed={} nmi={}",
            self.inner.pc(), self.inner.sp(),
            self.inner.is_halted(), self.inner.iff1(), self.inner.im(),
            self.inner.total_cycles(), self.inner.is_lcd_on(), self.inner.is_off(),
            self.inner.stack_limit(), self.inner.protected_start(), self.inner.protected_end(),
            self.inner.cpu_speed(), self.inner.nmi_pending(),
        )
    }
}

impl Default for WasmEmu {
    fn default() -> Self {
        Self::new()
    }
}
