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
    /// Prefetch buffer - holds the byte that was prefetched during the previous fetch
    /// CEmu prefetches the NEXT byte during each fetch, which charges cycles for
    /// the next instruction's first byte as part of the current instruction.
    /// This is essential for cycle parity with CEmu.
    pub prefetch: u8,
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
            // Prefetch starts at 0 - will be initialized by reset() with bus access
            prefetch: 0,
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
        // Prefetch will be initialized by init_prefetch() when bus is available
        self.prefetch = 0;
    }

    /// Initialize the prefetch buffer after reset
    /// Must be called after reset() when the bus is available.
    /// CEmu prefetches the first byte at PC during cpu_prefetch(0, cpu.ADL) at init.
    pub fn init_prefetch(&mut self, bus: &mut Bus) {
        // Prefetch the first byte at PC=0 (with MBASE applied if in Z80 mode)
        let effective_pc = self.mask_addr_instr(self.pc);
        // Read the byte and store it in prefetch buffer
        // This charges cycles for the first instruction's first byte
        self.prefetch = bus.fetch_byte(effective_pc, self.pc);
    }

    // ========== Instruction Execution ==========

    /// Execute one instruction, returns cycles used
    ///
    /// Cycle counting: Returns the actual bus cycle delta, which includes
    /// both memory access cycles (flash/RAM/port reads/writes) and internal
    /// CPU processing cycles. This matches CEmu's cycle counting behavior.
    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        // Track cycles at start - we return the delta at the end
        // Use total_cycles() to include both CPU internal cycles and memory timing
        let start_cycles = bus.total_cycles();

        /// Helper to compute cycle delta, handling the case where the cycle counter
        /// was reset mid-instruction (e.g., when CPU speed changes via port 0x01 write).
        /// In that case, end_cycles < start_cycles, so we return end_cycles (cycles since reset).
        #[inline(always)]
        fn cycle_delta(start: u64, end: u64) -> u32 {
            if end >= start {
                (end - start) as u32
            } else {
                // Cycle counter was reset - return cycles accumulated since reset
                end as u32
            }
        }

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
            return cycle_delta(start_cycles, bus.total_cycles());
        }

        // Check for maskable interrupt
        if self.irq_pending && self.iff1 {
            self.irq_pending = false;
            self.handle_irq(bus);
            return cycle_delta(start_cycles, bus.total_cycles());
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
            // ON key can wake from HALT or from powered-off loop (running with interrupts disabled)
            // In both cases, if there's a pending interrupt, enable interrupts so it gets taken
            if self.irq_pending {
                self.iff1 = true;
                self.iff2 = true;
            }
            if self.halted {
                self.halted = false;
                bus.add_cycles(4); // Wake from halt cycle cost
                return cycle_delta(start_cycles, bus.total_cycles());
            }
            // If not halted, CPU is running in powered-off loop - interrupts now enabled,
            // so ON_KEY interrupt will fire on next iteration
        }

        // Check for any key wake - wakes CPU from HALT like CEmu's CPU_SIGNAL_ANY_KEY
        // This is separate from the interrupt mechanism - it just wakes the CPU so
        // the OS can poll the keypad registers and see the key press.
        if self.any_key_wake {
            if self.halted {
                self.any_key_wake = false;
                self.halted = false;
                bus.add_cycles(4); // Wake from halt cycle cost
                return cycle_delta(start_cycles, bus.total_cycles());
            }
            // Clear the flag even if not halted - signal has been processed
            self.any_key_wake = false;
        }

        if self.halted {
            // CPU is halted, just consume cycles
            // Interrupts can wake it (handled above on next call)
            bus.add_cycles(4); // Halted NOP cycle cost
            return cycle_delta(start_cycles, bus.total_cycles());
        }

        // eZ80 per-instruction mode handling:
        // L and IL are reset to ADL at the start of each instruction, UNLESS
        // the previous instruction was a suffix opcode that set them.
        if !self.suffix {
            self.l = self.adl;
            self.il = self.adl;
        }
        self.suffix = false;

        // Note: DD/FD prefixes are now executed immediately in execute_x3,
        // not deferred to the next step. This matches CEmu's trace behavior.

        // Opcode fetch loop - handles suffix opcodes that modify the following instruction
        // eZ80 suffix opcodes (.SIS, .LIS, .SIL, .LIL) are NOT separate instructions;
        // they modify the L/IL modes for the immediately following instruction.
        // CEmu executes the suffix + following instruction as a single step.
        loop {
            let opcode = self.fetch_byte(bus);

            // Decode using x-y-z-p-q decomposition
            let x = (opcode >> 6) & 0x03;
            let y = (opcode >> 3) & 0x07;
            let z = opcode & 0x07;
            let p = (y >> 1) & 0x03;
            let q = y & 0x01;

            // eZ80 suffix opcodes: .SIS (0x40), .LIS (0x49), .SIL (0x52), .LIL (0x5B)
            // These are encoded as LD r,r where y==z and z<4.
            // They set L and IL for the NEXT instruction and continue execution.
            // - Bit 0 (s): Sets L (data addressing mode)
            // - Bit 1 (r): Sets IL (instruction/index addressing mode)
            // CEmu: cpu.L = context.s, cpu.IL = context.r
            //
            // Note: We do NOT set suffix=true here because we're handling the suffix
            // atomically in this loop. The suffix only affects THIS loop iteration.
            // If the next instruction is a DD/FD prefix (which sets self.prefix),
            // the suffix modes should NOT persist to the execute_index call in the
            // next step() - that would be incorrect behavior.
            if x == 1 && y == z && z < 4 {
                let s = (opcode & 0x01) != 0; // L mode (bit 0)
                let r = (opcode & 0x02) != 0; // IL mode (bit 1)
                self.l = s;
                self.il = r;
                // Continue to fetch and execute the next instruction with modified modes
                // This matches CEmu's behavior where suffix + instruction are atomic
                continue;
            }

            // Execute instruction - the return values are legacy and ignored
            // since we now track cycles via bus.cycles()
            match x {
                0 => { self.execute_x0(bus, y, z, p, q); }
                1 => {
                    if y == 6 && z == 6 {
                        // HALT
                        bus.add_cycles(1); // CEmu: cpu.cycles++ before cpu_halt()
                        self.halted = true;
                    } else {
                        // LD r,r'
                        let val = self.get_reg8(z, bus);
                        self.set_reg8(y, val, bus);
                        // CEmu: cpu.cycles += z == 6 || y == 6 for (HL) operand
                        if z == 6 || y == 6 {
                            bus.add_cycles(1);
                        }
                    }
                }
                2 => {
                    // ALU A,r
                    let val = self.get_reg8(z, bus);
                    self.execute_alu(y, val);
                    // CEmu: cpu.cycles += z == 6 for (HL) operand
                    if z == 6 {
                        bus.add_cycles(1);
                    }
                }
                3 => { self.execute_x3(bus, y, z, p, q); }
                _ => {}
            }

            break;
        }

        cycle_delta(start_cycles, bus.total_cycles())
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
                self.prefetch(bus, 0x38); // Reload prefetch at ISR address
                self.pc = 0x38;
                13
            }
            InterruptMode::Mode1 => {
                // Mode 1: Fixed call to 0x0038
                self.push_addr(bus, self.pc);
                self.prefetch(bus, 0x38); // Reload prefetch at ISR address
                self.pc = 0x38;
                13
            }
            InterruptMode::Mode2 => {
                // Mode 2: On TI-84 CE (eZ80), this is NOT the standard Z80 vectored mode!
                // CEmu shows that IM 2 just jumps to 0x38, same as IM 1.
                // The vectored interrupt mode (using I register) is only used in IM 3
                // with the asic.im2 flag, which the TI-84 CE doesn't use.
                self.push_addr(bus, self.pc);
                self.prefetch(bus, 0x38); // Reload prefetch at ISR address
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
        self.prefetch(bus, 0x66); // Reload prefetch at NMI handler address
        self.pc = 0x66;
        11
    }
}

// ========== State Persistence ==========

impl Cpu {
    /// Size of CPU state snapshot in bytes
    pub const SNAPSHOT_SIZE: usize = 64;

    /// Save CPU state to bytes for persistence
    pub fn to_bytes(&self) -> [u8; Self::SNAPSHOT_SIZE] {
        let mut buf = [0u8; Self::SNAPSHOT_SIZE];
        let mut pos = 0;

        // Main registers (11 bytes)
        buf[pos] = self.a; pos += 1;
        buf[pos] = self.f; pos += 1;
        buf[pos..pos+3].copy_from_slice(&self.bc.to_le_bytes()[..3]); pos += 3;
        buf[pos..pos+3].copy_from_slice(&self.de.to_le_bytes()[..3]); pos += 3;
        buf[pos..pos+3].copy_from_slice(&self.hl.to_le_bytes()[..3]); pos += 3;

        // Shadow registers (11 bytes)
        buf[pos] = self.a_prime; pos += 1;
        buf[pos] = self.f_prime; pos += 1;
        buf[pos..pos+3].copy_from_slice(&self.bc_prime.to_le_bytes()[..3]); pos += 3;
        buf[pos..pos+3].copy_from_slice(&self.de_prime.to_le_bytes()[..3]); pos += 3;
        buf[pos..pos+3].copy_from_slice(&self.hl_prime.to_le_bytes()[..3]); pos += 3;

        // Index registers (6 bytes)
        buf[pos..pos+3].copy_from_slice(&self.ix.to_le_bytes()[..3]); pos += 3;
        buf[pos..pos+3].copy_from_slice(&self.iy.to_le_bytes()[..3]); pos += 3;

        // Special registers (10 bytes)
        buf[pos..pos+3].copy_from_slice(&self.sp.to_le_bytes()[..3]); pos += 3;
        buf[pos..pos+3].copy_from_slice(&self.pc.to_le_bytes()[..3]); pos += 3;
        buf[pos..pos+2].copy_from_slice(&self.i.to_le_bytes()); pos += 2;
        buf[pos] = self.r; pos += 1;
        buf[pos] = self.mbase; pos += 1;

        // State flags as bitmask (1 byte)
        let mut flags = 0u8;
        if self.iff1 { flags |= 1 << 0; }
        if self.iff2 { flags |= 1 << 1; }
        if self.adl { flags |= 1 << 2; }
        if self.halted { flags |= 1 << 3; }
        if self.irq_pending { flags |= 1 << 4; }
        if self.nmi_pending { flags |= 1 << 5; }
        if self.on_key_wake { flags |= 1 << 6; }
        if self.any_key_wake { flags |= 1 << 7; }
        buf[pos] = flags; pos += 1;

        // IM mode (1 byte)
        buf[pos] = match self.im {
            InterruptMode::Mode0 => 0,
            InterruptMode::Mode1 => 1,
            InterruptMode::Mode2 => 2,
        }; pos += 1;

        // Internal state (5 bytes)
        buf[pos] = self.ei_delay; pos += 1;
        let mut mode_flags = 0u8;
        if self.l { mode_flags |= 1 << 0; }
        if self.il { mode_flags |= 1 << 1; }
        if self.suffix { mode_flags |= 1 << 2; }
        if self.madl { mode_flags |= 1 << 3; }
        buf[pos] = mode_flags; pos += 1;
        buf[pos] = self.prefix; pos += 1;
        buf[pos] = self.prefetch; pos += 1;

        // Padding to SNAPSHOT_SIZE
        let _ = pos; // Unused beyond here

        buf
    }

    /// Load CPU state from bytes
    pub fn from_bytes(&mut self, buf: &[u8]) -> Result<(), i32> {
        if buf.len() < Self::SNAPSHOT_SIZE {
            return Err(-105); // Buffer too small
        }

        let mut pos = 0;

        // Main registers
        self.a = buf[pos]; pos += 1;
        self.f = buf[pos]; pos += 1;
        self.bc = u32::from_le_bytes([buf[pos], buf[pos+1], buf[pos+2], 0]); pos += 3;
        self.de = u32::from_le_bytes([buf[pos], buf[pos+1], buf[pos+2], 0]); pos += 3;
        self.hl = u32::from_le_bytes([buf[pos], buf[pos+1], buf[pos+2], 0]); pos += 3;

        // Shadow registers
        self.a_prime = buf[pos]; pos += 1;
        self.f_prime = buf[pos]; pos += 1;
        self.bc_prime = u32::from_le_bytes([buf[pos], buf[pos+1], buf[pos+2], 0]); pos += 3;
        self.de_prime = u32::from_le_bytes([buf[pos], buf[pos+1], buf[pos+2], 0]); pos += 3;
        self.hl_prime = u32::from_le_bytes([buf[pos], buf[pos+1], buf[pos+2], 0]); pos += 3;

        // Index registers
        self.ix = u32::from_le_bytes([buf[pos], buf[pos+1], buf[pos+2], 0]); pos += 3;
        self.iy = u32::from_le_bytes([buf[pos], buf[pos+1], buf[pos+2], 0]); pos += 3;

        // Special registers
        self.sp = u32::from_le_bytes([buf[pos], buf[pos+1], buf[pos+2], 0]); pos += 3;
        self.pc = u32::from_le_bytes([buf[pos], buf[pos+1], buf[pos+2], 0]); pos += 3;
        self.i = u16::from_le_bytes([buf[pos], buf[pos+1]]); pos += 2;
        self.r = buf[pos]; pos += 1;
        self.mbase = buf[pos]; pos += 1;

        // State flags
        let flags = buf[pos]; pos += 1;
        self.iff1 = flags & (1 << 0) != 0;
        self.iff2 = flags & (1 << 1) != 0;
        self.adl = flags & (1 << 2) != 0;
        self.halted = flags & (1 << 3) != 0;
        self.irq_pending = flags & (1 << 4) != 0;
        self.nmi_pending = flags & (1 << 5) != 0;
        self.on_key_wake = flags & (1 << 6) != 0;
        self.any_key_wake = flags & (1 << 7) != 0;

        // IM mode
        self.im = match buf[pos] {
            0 => InterruptMode::Mode0,
            1 => InterruptMode::Mode1,
            _ => InterruptMode::Mode2,
        }; pos += 1;

        // Internal state
        self.ei_delay = buf[pos]; pos += 1;
        let mode_flags = buf[pos]; pos += 1;
        self.l = mode_flags & (1 << 0) != 0;
        self.il = mode_flags & (1 << 1) != 0;
        self.suffix = mode_flags & (1 << 2) != 0;
        self.madl = mode_flags & (1 << 3) != 0;
        self.prefix = buf[pos]; pos += 1;
        self.prefetch = buf[pos];

        Ok(())
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}
