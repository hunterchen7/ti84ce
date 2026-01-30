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
    /// ON key wake signal - wakes CPU from HALT even with interrupts disabled
    /// This is a TI-84 CE specific feature where the ON key can always wake the calculator
    pub on_key_wake: bool,
    /// Any key wake signal - wakes CPU from HALT when any key is pressed
    /// Like CEmu's CPU_SIGNAL_ANY_KEY, this allows keys to wake the CPU
    pub any_key_wake: bool,
    /// EI delay counter - EI enables interrupts after the NEXT instruction
    /// When EI is executed, this is set to 2. It decrements each step, and when
    /// it reaches 0, IFF1/IFF2 are set to true.
    ei_delay: u8,

    // Per-instruction mode flags (eZ80 suffix support)
    // These are reset to ADL at the start of each instruction, but can be
    // overridden by suffix opcodes (.SIS, .LIS, .SIL, .LIL)
    /// L mode - data addressing mode for current instruction
    /// When true, use 24-bit addresses for data operations
    pub l: bool,
    /// IL mode - instruction/index addressing mode for current instruction
    /// When true, use 24-bit addresses for instruction word fetches
    /// JP/CALL/RET with IL=true set ADL=true permanently
    pub il: bool,
    /// Whether current instruction was prefixed by a suffix opcode
    suffix: bool,
    /// MADL - Mixed memory mode ADL
    /// When set by STMIX, enables mixed memory mode where MBASE affects execution
    /// Cleared by RSMIX
    pub madl: bool,
    /// Pending DD/FD prefix: 0=none, 2=DD (IX), 3=FD (IY)
    /// When set, the next step() will execute an indexed instruction
    /// This matches CEmu's behavior where prefixes count as separate steps
    prefix: u8,
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
            sp: 0x000000, // Matches CEmu reset; ROM sets stack later
            pc: 0,
            i: 0,
            r: 0,
            mbase: 0x00, // CEmu resets MBASE to 0; ROM sets it later

            // State
            iff1: false,
            iff2: false,
            im: InterruptMode::Mode0,
            adl: false, // CEmu resets in Z80 mode; ROM enables ADL
            halted: false,

            // Interrupts
            irq_pending: false,
            nmi_pending: false,
            on_key_wake: false,
            any_key_wake: false,
            ei_delay: 0,

            // Per-instruction modes (reset to ADL at start of each instruction)
            l: false,
            il: false,
            suffix: false,
            madl: false,
            prefix: 0,
        }
    }

    /// Reset the CPU to initial state
    /// CEmu zeroes all registers on reset (memset(&cpu, 0, sizeof(cpu)))
    pub fn reset(&mut self) {
        // Main registers - CEmu zeroes everything
        self.a = 0x00;
        self.f = 0x00;
        self.bc = 0;
        self.de = 0;
        self.hl = 0;

        // Shadow registers
        self.a_prime = 0;
        self.f_prime = 0;
        self.bc_prime = 0;
        self.de_prime = 0;
        self.hl_prime = 0;

        // Index registers
        self.ix = 0;
        self.iy = 0;

        // Special registers
        self.pc = 0;
        self.sp = 0;
        self.i = 0;
        self.r = 0;
        self.mbase = 0x00;

        // CPU state flags
        self.iff1 = false;
        self.iff2 = false;
        self.im = InterruptMode::Mode0;
        self.adl = false;
        self.halted = false;

        // Internal state
        self.irq_pending = false;
        self.nmi_pending = false;
        self.on_key_wake = false;
        self.any_key_wake = false;
        self.ei_delay = 0;
        self.l = false;
        self.il = false;
        self.suffix = false;
        self.madl = false;
        self.prefix = 0;
    }

    // ========== Instruction Execution ==========

    /// Execute one instruction, returns cycles used
    ///
    /// Cycle counting: Returns the actual bus cycle delta, which includes
    /// both memory access cycles (flash/RAM/port reads/writes) and internal
    /// CPU processing cycles. This matches CEmu's cycle counting behavior.
    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        // Track cycles at start - we return the delta at the end
        let start_cycles = bus.cycles();

        // Process EI delay - interrupts enable AFTER the instruction following EI
        // This happens BEFORE we check for interrupts, so that:
        // 1. EI is executed, sets ei_delay = 2
        // 2. Next instruction executes, ei_delay decrements to 1
        // 3. Following instruction: ei_delay decrements to 0, IFF1/IFF2 set true
        if self.ei_delay > 0 {
            self.ei_delay -= 1;
            if self.ei_delay == 0 {
                self.iff1 = true;
                self.iff2 = true;
            }
        }

        // Check for NMI first (highest priority)
        if self.nmi_pending {
            self.nmi_pending = false;
            self.handle_nmi(bus);
            return (bus.cycles() - start_cycles) as u32;
        }

        // Check for maskable interrupt
        if self.irq_pending && self.iff1 {
            self.irq_pending = false;
            self.handle_irq(bus);
            return (bus.cycles() - start_cycles) as u32;
        }

        // Check for ON key wake - can wake CPU even with interrupts disabled
        // This is a TI-84 CE specific feature
        //
        // On the real TI-84 CE, the ON key can wake the CPU from HALT regardless of IFF1.
        // When woken, if there's a pending interrupt (ON_KEY or WAKE), we need to
        // enable interrupts so the interrupt handler can run properly.
        //
        // Without this, the ROM code path after HALT expects to have been entered
        // via interrupt (so RETI has a valid return address), but with DI active
        // no interrupt is taken and RETI pops garbage.
        //
        // Solution: When ON key wakes with a pending interrupt, enable IFF1/IFF2
        // so the interrupt is taken on the next step() call.
        if self.on_key_wake {
            self.on_key_wake = false;
            if self.halted {
                self.halted = false;
                // If there's a pending interrupt, enable interrupts so it gets taken
                // This matches TI-84 CE behavior where ON key wake triggers the interrupt
                if self.irq_pending {
                    self.iff1 = true;
                    self.iff2 = true;
                }
                bus.add_cycles(4); // Wake from halt cycle cost
                return (bus.cycles() - start_cycles) as u32;
            }
        }

        // Check for any key wake - wakes CPU from HALT like CEmu's CPU_SIGNAL_ANY_KEY
        // This is separate from the interrupt mechanism - it just wakes the CPU so
        // the OS can poll the keypad registers and see the key press.
        if self.any_key_wake {
            self.any_key_wake = false;
            if self.halted {
                crate::log_event(&format!(
                    "ANY_KEY_WAKE: waking CPU from HALT at PC=0x{:06X}, iff1={}",
                    self.pc, self.iff1
                ));
                self.halted = false;
                bus.add_cycles(4); // Wake from halt cycle cost
                return (bus.cycles() - start_cycles) as u32;
            }
        }

        if self.halted {
            // CPU is halted, just consume cycles
            // Interrupts can wake it (handled above on next call)
            bus.add_cycles(4); // Halted NOP cycle cost
            return (bus.cycles() - start_cycles) as u32;
        }

        // eZ80 per-instruction mode handling:
        // L and IL are reset to ADL at the start of each instruction, UNLESS
        // the previous instruction was a suffix opcode that set them.
        if !self.suffix {
            self.l = self.adl;
            self.il = self.adl;
        }
        self.suffix = false;

        // Check for pending DD/FD prefix from previous step
        // CEmu counts DD/FD prefixes as separate instruction steps
        if self.prefix != 0 {
            let use_ix = self.prefix == 2; // 2=DD (IX), 3=FD (IY)
            self.prefix = 0;
            return self.execute_index(bus, use_ix);
        }

        let opcode = self.fetch_byte(bus);

        // Decode using x-y-z-p-q decomposition
        let x = (opcode >> 6) & 0x03;
        let y = (opcode >> 3) & 0x07;
        let z = opcode & 0x07;
        let p = (y >> 1) & 0x03;
        let q = y & 0x01;

        // eZ80 suffix opcodes: .SIS (0x40), .LIS (0x49), .SIL (0x52), .LIL (0x5B)
        // These are encoded as LD r,r where y==z and z<4.
        // They set L and IL for the NEXT instruction.
        // - Bit 0 (s): Sets L (data addressing mode)
        // - Bit 1 (r): Sets IL (instruction/index addressing mode)
        // CEmu: cpu.L = context.s, cpu.IL = context.r
        // Note: CEmu loops and fetches the next instruction immediately, but we
        // return here and let the caller call step() again. This matches CEmu's
        // trace behavior where the suffix is counted as a separate instruction.
        if x == 1 && y == z && z < 4 {
            let s = (opcode & 0x01) != 0; // L mode (bit 0)
            let r = (opcode & 0x02) != 0; // IL mode (bit 1)
            self.l = s;
            self.il = r;
            self.suffix = true;
            // Return - the next step() call will execute with the suffix modes
            // The suffix flag prevents L/IL from being reset to ADL
            // Note: The opcode fetch already added cycles
            return (bus.cycles() - start_cycles) as u32;
        }

        // Execute instruction - the return values are legacy and ignored
        // since we now track cycles via bus.cycles()
        match x {
            0 => { self.execute_x0(bus, y, z, p, q); }
            1 => {
                if y == 6 && z == 6 {
                    // HALT
                    self.halted = true;
                } else {
                    // LD r,r'
                    let val = self.get_reg8(z, bus);
                    self.set_reg8(y, val, bus);
                }
            }
            2 => {
                // ALU A,r
                let val = self.get_reg8(z, bus);
                self.execute_alu(y, val);
            }
            3 => { self.execute_x3(bus, y, z, p, q); }
            _ => {}
        }

        (bus.cycles() - start_cycles) as u32
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
                // Mode 2: On TI-84 CE (eZ80), this is NOT the standard Z80 vectored mode!
                // CEmu shows that IM 2 just jumps to 0x38, same as IM 1.
                // The vectored interrupt mode (using I register) is only used in IM 3
                // with the asic.im2 flag, which the TI-84 CE doesn't use.
                self.push_addr(bus, self.pc);
                self.pc = 0x38;
                13
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
