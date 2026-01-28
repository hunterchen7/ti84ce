//! eZ80 CPU flag bit definitions
//!
//! Flag bit positions in the F (flags) register for the eZ80 processor.
//!
//! # References
//! - eZ80 CPU User Manual (Zilog UM0077)
//! - CEmu (https://github.com/CE-Programming/CEmu)

/// Carry flag (bit 0)
pub const C: u8 = 0b0000_0001;
/// Add/Subtract flag (bit 1) - set for subtraction
pub const N: u8 = 0b0000_0010;
/// Parity/Overflow flag (bit 2)
pub const PV: u8 = 0b0000_0100;
/// Undocumented flag (bit 3) - copy of bit 3 of result
pub const F3: u8 = 0b0000_1000;
/// Half-carry flag (bit 4)
pub const H: u8 = 0b0001_0000;
/// Undocumented flag (bit 5) - copy of bit 5 of result
pub const F5: u8 = 0b0010_0000;
/// Zero flag (bit 6)
pub const Z: u8 = 0b0100_0000;
/// Sign flag (bit 7)
pub const S: u8 = 0b1000_0000;
