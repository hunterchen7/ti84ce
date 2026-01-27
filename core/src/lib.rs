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
mod emu;

use std::ptr;
use std::slice;

pub use emu::Emu;

/// Create a new emulator instance.
/// Returns null on allocation failure.
#[no_mangle]
pub extern "C" fn emu_create() -> *mut Emu {
    let emu = Box::new(Emu::new());
    Box::into_raw(emu)
}

/// Destroy an emulator instance.
/// Safe to call with null pointer.
#[no_mangle]
pub extern "C" fn emu_destroy(emu: *mut Emu) {
    if !emu.is_null() {
        unsafe {
            drop(Box::from_raw(emu));
        }
    }
}

/// Load ROM data into the emulator.
/// Returns 0 on success, negative error code on failure.
#[no_mangle]
pub extern "C" fn emu_load_rom(emu: *mut Emu, data: *const u8, len: usize) -> i32 {
    if emu.is_null() || data.is_null() {
        return -1;
    }

    let emu = unsafe { &mut *emu };
    let rom_data = unsafe { slice::from_raw_parts(data, len) };

    match emu.load_rom(rom_data) {
        Ok(()) => 0,
        Err(code) => code,
    }
}

/// Reset the emulator to initial state.
#[no_mangle]
pub extern "C" fn emu_reset(emu: *mut Emu) {
    if emu.is_null() {
        return;
    }

    let emu = unsafe { &mut *emu };
    emu.reset();
}

/// Run the emulator for the specified number of cycles.
/// Returns the number of cycles actually executed.
#[no_mangle]
pub extern "C" fn emu_run_cycles(emu: *mut Emu, cycles: i32) -> i32 {
    if emu.is_null() || cycles <= 0 {
        return 0;
    }

    let emu = unsafe { &mut *emu };
    emu.run_cycles(cycles as u32) as i32
}

/// Get a pointer to the framebuffer.
/// The framebuffer is ARGB8888 format, owned by the emulator.
/// Writes width and height to the provided pointers if non-null.
/// Returns null if emulator pointer is null.
#[no_mangle]
pub extern "C" fn emu_framebuffer(emu: *const Emu, w: *mut i32, h: *mut i32) -> *const u32 {
    if emu.is_null() {
        return ptr::null();
    }

    let emu = unsafe { &*emu };
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
#[no_mangle]
pub extern "C" fn emu_set_key(emu: *mut Emu, row: i32, col: i32, down: i32) {
    if emu.is_null() {
        return;
    }

    let emu = unsafe { &mut *emu };
    emu.set_key(row as usize, col as usize, down != 0);
}

/// Get the size needed for a save state buffer.
#[no_mangle]
pub extern "C" fn emu_save_state_size(emu: *const Emu) -> usize {
    if emu.is_null() {
        return 0;
    }

    let emu = unsafe { &*emu };
    emu.save_state_size()
}

/// Save emulator state to a buffer.
/// Returns bytes written on success, negative error code on failure.
#[no_mangle]
pub extern "C" fn emu_save_state(emu: *const Emu, out: *mut u8, cap: usize) -> i32 {
    if emu.is_null() || out.is_null() {
        return -1;
    }

    let emu = unsafe { &*emu };
    let buffer = unsafe { slice::from_raw_parts_mut(out, cap) };

    match emu.save_state(buffer) {
        Ok(size) => size as i32,
        Err(code) => code,
    }
}

/// Load emulator state from a buffer.
/// Returns 0 on success, negative error code on failure.
#[no_mangle]
pub extern "C" fn emu_load_state(emu: *mut Emu, data: *const u8, len: usize) -> i32 {
    if emu.is_null() || data.is_null() {
        return -1;
    }

    let emu = unsafe { &mut *emu };
    let buffer = unsafe { slice::from_raw_parts(data, len) };

    match emu.load_state(buffer) {
        Ok(()) => 0,
        Err(code) => code,
    }
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
        let executed = emu_run_cycles(emu, 1000);
        assert_eq!(executed, 1000);
        emu_destroy(emu);
    }

    #[test]
    fn test_key_input() {
        let emu = emu_create();
        emu_set_key(emu, 0, 0, 1);
        emu_set_key(emu, 0, 0, 0);
        emu_destroy(emu);
    }
}
