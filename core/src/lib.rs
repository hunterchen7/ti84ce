//! TI-84 Plus CE Emulator Core
//!
//! This crate provides a platform-agnostic emulator core with a stable C ABI.
//! No OS APIs are used - all I/O is done through byte buffers.
//!
//! # Architecture
//!
//! The emulator is organized into several modules:
//! - `memory`: Flash, RAM, and port memory implementations
//! - `bus`: Address decoding and memory access routing
//! - `cpu`: eZ80 CPU implementation
//! - `emu`: Main emulator orchestrator
//!
//! # Memory Map (24-bit eZ80 address space)
//!
//! | Address Range       | Region              |
//! |---------------------|---------------------|
//! | 0x000000 - 0x3FFFFF | Flash (4MB)         |
//! | 0x400000 - 0xCFFFFF | Unmapped            |
//! | 0xD00000 - 0xD657FF | RAM + VRAM          |
//! | 0xD65800 - 0xDFFFFF | Unmapped            |
//! | 0xE00000 - 0xFFFFFF | Memory-mapped I/O   |

pub mod memory;
pub mod bus;
pub mod cpu;
pub mod peripherals;
pub mod scheduler;
pub mod disasm;
pub mod ti_file;
mod emu;

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod keypad_integration_test;

#[cfg(test)]
mod calc_integration_test;

use std::os::raw::c_char;
use std::ptr;
use std::slice;
use std::sync::Mutex;

pub use emu::{Emu, LcdSnapshot, TimerSnapshot, StepInfo, log_event};
pub use bus::{IoTarget, IoOpType, IoRecord};
pub use disasm::{disassemble, DisasmResult};

/// Thread-safe wrapper for the emulator.
/// All FFI calls go through this mutex to prevent data races between
/// the UI thread (key events) and emulation thread (run_cycles).
/// This is an opaque type from C's perspective (used via void*).
pub struct SyncEmu {
    inner: Mutex<Emu>,
}

impl SyncEmu {
    fn new() -> Self {
        Self {
            inner: Mutex::new(Emu::new()),
        }
    }
}

/// Create a new emulator instance.
/// Returns null on allocation failure.
/// The returned pointer is thread-safe - all operations are synchronized.
#[cfg_attr(not(feature = "ios_prefixed"), no_mangle)]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_create")]
pub extern "C" fn emu_create() -> *mut SyncEmu {
    let emu = Box::new(SyncEmu::new());
    Box::into_raw(emu)
}

/// Destroy an emulator instance.
/// Safe to call with null pointer.
#[cfg_attr(not(feature = "ios_prefixed"), no_mangle)]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_destroy")]
pub extern "C" fn emu_destroy(emu: *mut SyncEmu) {
    if !emu.is_null() {
        unsafe {
            drop(Box::from_raw(emu));
        }
    }
}

/// Set an optional log callback for emulator events.
/// The callback is called with a null-terminated C string.
#[cfg_attr(not(feature = "ios_prefixed"), no_mangle)]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_set_log_callback")]
pub extern "C" fn emu_set_log_callback(cb: Option<extern "C" fn(*const c_char)>) {
    emu::set_log_callback(cb);
}

/// Load ROM data into the emulator.
/// Returns 0 on success, negative error code on failure.
#[cfg_attr(not(feature = "ios_prefixed"), no_mangle)]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_load_rom")]
pub extern "C" fn emu_load_rom(emu: *mut SyncEmu, data: *const u8, len: usize) -> i32 {
    if emu.is_null() || data.is_null() {
        return -1;
    }

    let sync_emu = unsafe { &*emu };
    let rom_data = unsafe { slice::from_raw_parts(data, len) };

    let mut emu = sync_emu.inner.lock().unwrap();
    match emu.load_rom(rom_data) {
        Ok(()) => 0,
        Err(code) => code,
    }
}

/// Reset the emulator to initial state.
#[cfg_attr(not(feature = "ios_prefixed"), no_mangle)]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_reset")]
pub extern "C" fn emu_reset(emu: *mut SyncEmu) {
    if emu.is_null() {
        return;
    }

    let sync_emu = unsafe { &*emu };
    let mut emu = sync_emu.inner.lock().unwrap();
    emu.reset();
}

/// Power on the emulator (simulate ON key press+release).
/// Must be called after load_rom() to start execution.
#[no_mangle]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_power_on")]
pub extern "C" fn emu_power_on(emu: *mut SyncEmu) {
    if emu.is_null() {
        return;
    }

    let sync_emu = unsafe { &*emu };
    let mut emu = sync_emu.inner.lock().unwrap();
    emu.power_on();
}

/// Run the emulator for the specified number of cycles.
/// Returns the number of cycles actually executed.
/// Also updates the framebuffer with current VRAM contents.
#[cfg_attr(not(feature = "ios_prefixed"), no_mangle)]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_run_cycles")]
pub extern "C" fn emu_run_cycles(emu: *mut SyncEmu, cycles: i32) -> i32 {
    if emu.is_null() || cycles <= 0 {
        return 0;
    }

    let sync_emu = unsafe { &*emu };
    let mut emu = sync_emu.inner.lock().unwrap();
    let executed = emu.run_cycles(cycles as u32) as i32;
    emu.render_frame();
    executed
}

/// Get a pointer to the framebuffer.
/// The framebuffer is ARGB8888 format, owned by the emulator.
/// Writes width and height to the provided pointers if non-null.
/// Returns null if emulator pointer is null.
///
/// WARNING: The returned pointer is only valid while the mutex is held.
/// The caller should copy the framebuffer data immediately.
#[cfg_attr(not(feature = "ios_prefixed"), no_mangle)]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_framebuffer")]
pub extern "C" fn emu_framebuffer(emu: *const SyncEmu, w: *mut i32, h: *mut i32) -> *const u32 {
    if emu.is_null() {
        return ptr::null();
    }

    let sync_emu = unsafe { &*emu };
    let emu = sync_emu.inner.lock().unwrap();
    let (width, height) = emu.framebuffer_size();

    if !w.is_null() {
        unsafe { *w = width as i32 };
    }
    if !h.is_null() {
        unsafe { *h = height as i32 };
    }

    emu.framebuffer_ptr()
}

/// Set key state.
/// row: 0-7, col: 0-7
/// down: non-zero for pressed, zero for released
#[cfg_attr(not(feature = "ios_prefixed"), no_mangle)]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_set_key")]
pub extern "C" fn emu_set_key(emu: *mut SyncEmu, row: i32, col: i32, down: i32) {
    if emu.is_null() {
        return;
    }

    let sync_emu = unsafe { &*emu };
    let mut emu = sync_emu.inner.lock().unwrap();
    emu.set_key(row as usize, col as usize, down != 0);
}

/// Get the backlight brightness level (0-255).
/// Returns 0 if emulator pointer is null.
#[cfg_attr(not(feature = "ios_prefixed"), no_mangle)]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_get_backlight")]
pub extern "C" fn emu_get_backlight(emu: *const SyncEmu) -> u8 {
    if emu.is_null() {
        return 0;
    }

    let sync_emu = unsafe { &*emu };
    let emu = sync_emu.inner.lock().unwrap();
    emu.get_backlight()
}

/// Check if LCD is on (should display content).
/// Returns 1 if LCD is on, 0 if LCD is off.
/// LCD is off when either control port 0x05 bit 4 is clear OR lcd.control bit 11 is clear.
#[cfg_attr(not(feature = "ios_prefixed"), no_mangle)]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_is_lcd_on")]
pub extern "C" fn emu_is_lcd_on(emu: *const SyncEmu) -> i32 {
    if emu.is_null() {
        return 0;
    }

    let sync_emu = unsafe { &*emu };
    let emu = sync_emu.inner.lock().unwrap();
    if emu.is_lcd_on() { 1 } else { 0 }
}

/// Get the size needed for a save state buffer.
#[cfg_attr(not(feature = "ios_prefixed"), no_mangle)]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_save_state_size")]
pub extern "C" fn emu_save_state_size(emu: *const SyncEmu) -> usize {
    if emu.is_null() {
        return 0;
    }

    let sync_emu = unsafe { &*emu };
    let emu = sync_emu.inner.lock().unwrap();
    emu.save_state_size()
}

/// Save emulator state to a buffer.
/// Returns bytes written on success, negative error code on failure.
#[cfg_attr(not(feature = "ios_prefixed"), no_mangle)]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_save_state")]
pub extern "C" fn emu_save_state(emu: *const SyncEmu, out: *mut u8, cap: usize) -> i32 {
    if emu.is_null() || out.is_null() {
        return -1;
    }

    let sync_emu = unsafe { &*emu };
    let emu = sync_emu.inner.lock().unwrap();
    let buffer = unsafe { slice::from_raw_parts_mut(out, cap) };

    match emu.save_state(buffer) {
        Ok(size) => size as i32,
        Err(code) => code,
    }
}

/// Load emulator state from a buffer.
/// Returns 0 on success, negative error code on failure.
#[cfg_attr(not(feature = "ios_prefixed"), no_mangle)]
#[cfg_attr(feature = "ios_prefixed", export_name = "rust_emu_load_state")]
pub extern "C" fn emu_load_state(emu: *mut SyncEmu, data: *const u8, len: usize) -> i32 {
    if emu.is_null() || data.is_null() {
        return -1;
    }

    let sync_emu = unsafe { &*emu };
    let mut emu = sync_emu.inner.lock().unwrap();
    let buffer = unsafe { slice::from_raw_parts(data, len) };

    match emu.load_state(buffer) {
        Ok(()) => 0,
        Err(code) => code,
    }
}

// ============================================================
// Backend API (for single-backend builds without bridge)
// ============================================================

/// Get available backends (comma-separated list).
/// For Rust-only builds, returns "rust".
#[no_mangle]
#[cfg(not(feature = "ios_prefixed"))]
pub extern "C" fn emu_backend_get_available() -> *const c_char {
    static BACKENDS: &[u8] = b"rust\0";
    BACKENDS.as_ptr() as *const c_char
}

/// Get current backend name.
/// For Rust-only builds, returns "rust".
#[no_mangle]
#[cfg(not(feature = "ios_prefixed"))]
pub extern "C" fn emu_backend_get_current() -> *const c_char {
    static RUST: &[u8] = b"rust\0";
    RUST.as_ptr() as *const c_char
}

/// Set backend by name.
/// For Rust-only builds, only "rust" is valid.
/// Returns 0 on success, -1 on failure.
#[no_mangle]
#[cfg(not(feature = "ios_prefixed"))]
pub extern "C" fn emu_backend_set(name: *const c_char) -> i32 {
    if name.is_null() {
        return -1;
    }
    let name_str = unsafe { std::ffi::CStr::from_ptr(name) };
    if name_str.to_bytes() == b"rust" {
        0
    } else {
        -1
    }
}

/// Get number of available backends.
/// For Rust-only builds, returns 1.
#[no_mangle]
#[cfg(not(feature = "ios_prefixed"))]
pub extern "C" fn emu_backend_count() -> i32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_destroy() {
        let emu = emu_create();
        assert!(!emu.is_null());
        emu_destroy(emu);
    }

    #[test]
    fn test_framebuffer() {
        let emu = emu_create();
        let mut w: i32 = 0;
        let mut h: i32 = 0;
        let fb = emu_framebuffer(emu, &mut w, &mut h);

        assert!(!fb.is_null());
        assert_eq!(w, 320);
        assert_eq!(h, 240);

        emu_destroy(emu);
    }

    #[test]
    fn test_run_cycles() {
        let emu = emu_create();
        // Without ROM, should return 0
        let executed = emu_run_cycles(emu, 1000);
        assert_eq!(executed, 0);
        emu_destroy(emu);
    }

    #[test]
    fn test_key_input() {
        let emu = emu_create();
        emu_set_key(emu, 0, 0, 1);
        emu_set_key(emu, 0, 0, 0);
        emu_destroy(emu);
    }

    #[test]
    fn test_thread_safety() {
        use std::thread;

        let emu = emu_create();

        // Load a minimal ROM so we can run cycles
        let rom = vec![0x00, 0x00, 0x76]; // NOP, NOP, HALT
        emu_load_rom(emu, rom.as_ptr(), rom.len());

        // Wrap in Arc for sharing across threads
        let emu_ptr = emu as usize; // Convert to usize for Send

        // Spawn threads that access the emulator concurrently
        let handles: Vec<_> = (0..4).map(|i| {
            thread::spawn(move || {
                let emu = emu_ptr as *mut SyncEmu;
                for _ in 0..100 {
                    if i % 2 == 0 {
                        emu_set_key(emu, (i % 8) as i32, 0, 1);
                        emu_set_key(emu, (i % 8) as i32, 0, 0);
                    } else {
                        emu_run_cycles(emu, 10);
                    }
                }
            })
        }).collect();

        // Wait for all threads
        for h in handles {
            h.join().unwrap();
        }

        emu_destroy(emu);
    }
}
