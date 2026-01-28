//! eZ80 CPU tests
//!
//! Test suite for the eZ80 CPU implementation, organized into:
//! - instructions.rs: Tests for individual instructions and instruction families
//! - modes.rs: Tests for ADL mode and Z80 mode specific behavior
//!
//! # References
//! - eZ80 CPU User Manual (Zilog UM0077)
//! - CEmu (https://github.com/CE-Programming/CEmu)

use super::*;
use crate::bus::Bus;

mod instructions;
mod modes;

// ========== Test Helpers ==========

/// Helper to set up CPU in Z80 mode for testing backward compatibility
#[allow(dead_code)]
fn setup_z80_mode(cpu: &mut Cpu) {
    cpu.adl = false;
    cpu.mbase = 0xD0; // RAM starts at 0xD00000
    cpu.pc = 0x0100; // Typical Z80 program start
    cpu.sp = 0xFFFF; // Top of 64KB space
}

/// Helper to assert flags match expected value with detailed output
#[allow(dead_code)]
fn assert_flags(cpu: &Cpu, expected: u8, context: &str) {
    assert_eq!(
        cpu.f, expected,
        "{}: flags mismatch. Expected {:08b}, got {:08b} (S={} Z={} F5={} H={} F3={} PV={} N={} C={})",
        context,
        expected, cpu.f,
        if cpu.flag_s() { 1 } else { 0 },
        if cpu.flag_z() { 1 } else { 0 },
        if cpu.f & flags::F5 != 0 { 1 } else { 0 },
        if cpu.flag_h() { 1 } else { 0 },
        if cpu.f & flags::F3 != 0 { 1 } else { 0 },
        if cpu.flag_pv() { 1 } else { 0 },
        if cpu.flag_n() { 1 } else { 0 },
        if cpu.flag_c() { 1 } else { 0 },
    );
}
