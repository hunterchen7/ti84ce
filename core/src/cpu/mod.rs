//! eZ80 CPU implementation for TI-84 Plus CE
//!
//! The eZ80 is a Zilog Z80-compatible processor with extended 24-bit addressing.
//! The TI-84 Plus CE runs at 48 MHz in ADL (Address Data Long) mode.
//!
//! # Module Organization
//!
//! - `flags`: Flag bit constants for the F register
//! - `helpers`: Helper functions (register access, fetch, push/pop, ALU, flags)
//! - `execute`: Instruction execution functions (execute_x0, execute_cb, execute_ed, etc.)
//!
//! # Register Set
//!
//! In ADL mode, the main registers (BC, DE, HL, IX, IY) are 24-bit.
//! The CPU also has shadow registers (BC', DE', HL') for fast context switching.
//!
//! # References
//! - eZ80 CPU User Manual (Zilog UM0077)
//! - CEmu (https://github.com/CE-Programming/CEmu)

use crate::bus::Bus;

// Module declarations
mod execute;
pub mod flags;
mod helpers;

#[cfg(test)]
mod tests;

// Re-exports for API compatibility
pub use flags::*;

/// Interrupt modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InterruptMode {
    /// Mode 0: Execute instruction on data bus
    #[default]
    Mode0,
    /// Mode 1: Call to 0x0038
    Mode1,
    /// Mode 2: Vectored interrupts using I register
    Mode2,
}

/// eZ80 CPU state
pub struct Cpu {
    // Main registers - stored as 32-bit for 24-bit values
    /// Accumulator (8-bit)
    pub a: u8,
    /// Flags register (8-bit)
    pub f: u8,
    /// BC register pair (24-bit in ADL mode)
    pub bc: u32,
    /// DE register pair (24-bit in ADL mode)
    pub de: u32,
    /// HL register pair (24-bit in ADL mode)
    pub hl: u32,

    // Shadow registers (for EX AF,AF' and EXX)
    /// Shadow accumulator
    pub a_prime: u8,
    /// Shadow flags
    pub f_prime: u8,
    /// Shadow BC
    pub bc_prime: u32,
    /// Shadow DE
    pub de_prime: u32,
    /// Shadow HL
    pub hl_prime: u32,

    // Index registers (24-bit in ADL mode)
    /// IX index register
    pub ix: u32,
    /// IY index register
    pub iy: u32,

    // Special purpose registers
    /// Stack pointer (24-bit in ADL mode, SPL)
    pub sp: u32,
    /// Program counter (24-bit)
    pub pc: u32,
    /// Interrupt vector base (16-bit on eZ80)
    pub i: u16,
    /// Refresh register (7-bit, bit 7 preserved)
    pub r: u8,
    /// Memory base register (used in Z80 mode)
    pub mbase: u8,

    // CPU state flags
    /// Interrupt enable flip-flop 1
    pub iff1: bool,
    /// Interrupt enable flip-flop 2
    pub iff2: bool,
    /// Interrupt mode
    pub im: InterruptMode,
    /// ADL mode flag (true = 24-bit addressing)
    pub adl: bool,
    /// CPU is halted
    pub halted: bool,

    // Internal state for instruction execution
    /// Pending interrupt request
    pub irq_pending: bool,
    /// Pending NMI
    pub nmi_pending: bool,
}

impl Cpu {
    /// Create a new CPU in reset state
    pub fn new() -> Self {
        Self {
            // Main registers
            a: 0xFF,
            f: 0xFF,
            bc: 0,
            de: 0,
            hl: 0,

            // Shadow registers
            a_prime: 0,
            f_prime: 0,
            bc_prime: 0,
            de_prime: 0,
            hl_prime: 0,

            // Index registers
            ix: 0,
            iy: 0,

            // Special registers
            sp: 0xFFFF, // Stack starts at top of 16-bit range (or 24-bit in ADL)
            pc: 0,
            i: 0,
            r: 0,
            mbase: 0xD0, // Default MBASE for TI-84 CE

            // State
            iff1: false,
            iff2: false,
            im: InterruptMode::Mode0,
            adl: true, // TI-84 CE runs in ADL mode
            halted: false,

            // Interrupts
            irq_pending: false,
            nmi_pending: false,
        }
    }

    /// Reset the CPU to initial state
    pub fn reset(&mut self) {
        self.a = 0xFF;
        self.f = 0xFF;
        self.pc = 0;
        self.sp = 0xFFFF;
        self.i = 0;
        self.r = 0;
        self.iff1 = false;
        self.iff2 = false;
        self.im = InterruptMode::Mode0;
        self.adl = true;
        self.halted = false;
        self.irq_pending = false;
        self.nmi_pending = false;
        // Other registers are undefined after reset
    }

    // ========== Instruction Execution ==========

    /// Execute one instruction, returns cycles used
    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        // Check for NMI first (highest priority)
        if self.nmi_pending {
            self.nmi_pending = false;
            return self.handle_nmi(bus);
        }

        // Check for maskable interrupt
        if self.irq_pending && self.iff1 {
            self.irq_pending = false;
            return self.handle_irq(bus);
        }

        if self.halted {
            // CPU is halted, just consume cycles
            // Interrupts can wake it (handled above on next call)
            return 4;
        }

        let opcode = self.fetch_byte(bus);

        // Decode using x-y-z-p-q decomposition
        let x = (opcode >> 6) & 0x03;
        let y = (opcode >> 3) & 0x07;
        let z = opcode & 0x07;
        let p = (y >> 1) & 0x03;
        let q = y & 0x01;

        match x {
            0 => self.execute_x0(bus, y, z, p, q),
            1 => {
                if y == 6 && z == 6 {
                    // HALT
                    self.halted = true;
                    4
                } else {
                    // LD r,r'
                    let val = self.get_reg8(z, bus);
                    self.set_reg8(y, val, bus);
                    if y == 6 || z == 6 {
                        7
                    } else {
                        4
                    }
                }
            }
            2 => {
                // ALU A,r
                let val = self.get_reg8(z, bus);
                self.execute_alu(y, val);
                if z == 6 {
                    7
                } else {
                    4
                }
            }
            3 => self.execute_x3(bus, y, z, p, q),
            _ => 4, // Should not happen
        }
    }

    /// Handle maskable interrupt (IRQ)
    fn handle_irq(&mut self, bus: &mut Bus) -> u32 {
        // Wake from halt
        self.halted = false;

        // Disable interrupts
        self.iff1 = false;
        self.iff2 = false;

        match self.im {
            InterruptMode::Mode0 => {
                // Mode 0: Execute instruction on data bus
                // Typically RST 38H on TI calculators
                self.push_addr(bus, self.pc);
                self.pc = 0x38;
                13
            }
            InterruptMode::Mode1 => {
                // Mode 1: Fixed call to 0x0038
                self.push_addr(bus, self.pc);
                self.pc = 0x38;
                13
            }
            InterruptMode::Mode2 => {
                // Mode 2: Vectored interrupts
                // Vector address = (I register << 8) | data_bus_byte
                // TI-84 CE uses 0x00 as the data bus byte
                self.push_addr(bus, self.pc);
                let vector_addr = ((self.i as u32) << 8) | 0x00;
                // Read 24-bit handler address from vector table
                self.pc = if self.adl {
                    bus.read_addr24(vector_addr)
                } else {
                    let addr_with_mbase = ((self.mbase as u32) << 16) | vector_addr;
                    bus.read_word(addr_with_mbase) as u32
                };
                19
            }
        }
    }

    /// Handle non-maskable interrupt (NMI)
    fn handle_nmi(&mut self, bus: &mut Bus) -> u32 {
        // Wake from halt
        self.halted = false;

        // Save IFF1 to IFF2, disable IFF1
        self.iff2 = self.iff1;
        self.iff1 = false;

        // Jump to NMI handler at 0x0066
        self.push_addr(bus, self.pc);
        self.pc = 0x66;
        11
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}
