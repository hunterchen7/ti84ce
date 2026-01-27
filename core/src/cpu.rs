//! eZ80 CPU implementation for TI-84 Plus CE
//!
//! The eZ80 is a Zilog Z80-compatible processor with extended 24-bit addressing.
//! The TI-84 Plus CE runs at 48 MHz in ADL (Address Data Long) mode.
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

/// Flag bit positions in the F register
pub mod flags {
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
}

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

    // ========== Register Access Helpers ==========

    /// Get B register (high byte of BC)
    #[inline]
    pub fn b(&self) -> u8 {
        (self.bc >> 8) as u8
    }

    /// Set B register
    #[inline]
    pub fn set_b(&mut self, val: u8) {
        self.bc = (self.bc & 0xFF00FF) | ((val as u32) << 8);
    }

    /// Get C register (low byte of BC)
    #[inline]
    pub fn c(&self) -> u8 {
        self.bc as u8
    }

    /// Set C register
    #[inline]
    pub fn set_c(&mut self, val: u8) {
        self.bc = (self.bc & 0xFFFF00) | (val as u32);
    }

    /// Get D register (high byte of DE)
    #[inline]
    pub fn d(&self) -> u8 {
        (self.de >> 8) as u8
    }

    /// Set D register
    #[inline]
    pub fn set_d(&mut self, val: u8) {
        self.de = (self.de & 0xFF00FF) | ((val as u32) << 8);
    }

    /// Get E register (low byte of DE)
    #[inline]
    pub fn e(&self) -> u8 {
        self.de as u8
    }

    /// Set E register
    #[inline]
    pub fn set_e(&mut self, val: u8) {
        self.de = (self.de & 0xFFFF00) | (val as u32);
    }

    /// Get H register (high byte of HL)
    #[inline]
    pub fn h(&self) -> u8 {
        (self.hl >> 8) as u8
    }

    /// Set H register
    #[inline]
    pub fn set_h(&mut self, val: u8) {
        self.hl = (self.hl & 0xFF00FF) | ((val as u32) << 8);
    }

    /// Get L register (low byte of HL)
    #[inline]
    pub fn l(&self) -> u8 {
        self.hl as u8
    }

    /// Set L register
    #[inline]
    pub fn set_l(&mut self, val: u8) {
        self.hl = (self.hl & 0xFFFF00) | (val as u32);
    }

    /// Get IXH register
    #[inline]
    pub fn ixh(&self) -> u8 {
        (self.ix >> 8) as u8
    }

    /// Set IXH register
    #[inline]
    pub fn set_ixh(&mut self, val: u8) {
        self.ix = (self.ix & 0xFF00FF) | ((val as u32) << 8);
    }

    /// Get IXL register
    #[inline]
    pub fn ixl(&self) -> u8 {
        self.ix as u8
    }

    /// Set IXL register
    #[inline]
    pub fn set_ixl(&mut self, val: u8) {
        self.ix = (self.ix & 0xFFFF00) | (val as u32);
    }

    /// Get IYH register
    #[inline]
    pub fn iyh(&self) -> u8 {
        (self.iy >> 8) as u8
    }

    /// Set IYH register
    #[inline]
    pub fn set_iyh(&mut self, val: u8) {
        self.iy = (self.iy & 0xFF00FF) | ((val as u32) << 8);
    }

    /// Get IYL register
    #[inline]
    pub fn iyl(&self) -> u8 {
        self.iy as u8
    }

    /// Set IYL register
    #[inline]
    pub fn set_iyl(&mut self, val: u8) {
        self.iy = (self.iy & 0xFFFF00) | (val as u32);
    }

    // ========== Flag Helpers ==========

    /// Check if carry flag is set
    #[inline]
    pub fn flag_c(&self) -> bool {
        self.f & flags::C != 0
    }

    /// Set or clear carry flag
    #[inline]
    pub fn set_flag_c(&mut self, val: bool) {
        if val {
            self.f |= flags::C;
        } else {
            self.f &= !flags::C;
        }
    }

    /// Check if zero flag is set
    #[inline]
    pub fn flag_z(&self) -> bool {
        self.f & flags::Z != 0
    }

    /// Set or clear zero flag
    #[inline]
    pub fn set_flag_z(&mut self, val: bool) {
        if val {
            self.f |= flags::Z;
        } else {
            self.f &= !flags::Z;
        }
    }

    /// Check if sign flag is set
    #[inline]
    pub fn flag_s(&self) -> bool {
        self.f & flags::S != 0
    }

    /// Set or clear sign flag
    #[inline]
    pub fn set_flag_s(&mut self, val: bool) {
        if val {
            self.f |= flags::S;
        } else {
            self.f &= !flags::S;
        }
    }

    /// Check if half-carry flag is set
    #[inline]
    pub fn flag_h(&self) -> bool {
        self.f & flags::H != 0
    }

    /// Set or clear half-carry flag
    #[inline]
    pub fn set_flag_h(&mut self, val: bool) {
        if val {
            self.f |= flags::H;
        } else {
            self.f &= !flags::H;
        }
    }

    /// Check if parity/overflow flag is set
    #[inline]
    pub fn flag_pv(&self) -> bool {
        self.f & flags::PV != 0
    }

    /// Set or clear parity/overflow flag
    #[inline]
    pub fn set_flag_pv(&mut self, val: bool) {
        if val {
            self.f |= flags::PV;
        } else {
            self.f &= !flags::PV;
        }
    }

    /// Check if subtract flag is set
    #[inline]
    pub fn flag_n(&self) -> bool {
        self.f & flags::N != 0
    }

    /// Set or clear subtract flag
    #[inline]
    pub fn set_flag_n(&mut self, val: bool) {
        if val {
            self.f |= flags::N;
        } else {
            self.f &= !flags::N;
        }
    }

    /// Set flags based on 8-bit result (S, Z, F5, F3)
    #[inline]
    pub fn set_sz_flags(&mut self, result: u8) {
        // Clear S, Z, F5, F3
        self.f &= !(flags::S | flags::Z | flags::F5 | flags::F3);
        // Set based on result
        if result == 0 {
            self.f |= flags::Z;
        }
        if result & 0x80 != 0 {
            self.f |= flags::S;
        }
        // Undocumented: copy bits 5 and 3 of result
        self.f |= result & (flags::F5 | flags::F3);
    }

    /// Calculate parity of a byte (true if even number of 1 bits)
    #[inline]
    pub fn parity(val: u8) -> bool {
        val.count_ones() % 2 == 0
    }

    // ========== Register Pair Exchange ==========

    /// Exchange AF with AF'
    pub fn ex_af(&mut self) {
        std::mem::swap(&mut self.a, &mut self.a_prime);
        std::mem::swap(&mut self.f, &mut self.f_prime);
    }

    /// Exchange BC, DE, HL with their shadow registers (EXX)
    pub fn exx(&mut self) {
        std::mem::swap(&mut self.bc, &mut self.bc_prime);
        std::mem::swap(&mut self.de, &mut self.de_prime);
        std::mem::swap(&mut self.hl, &mut self.hl_prime);
    }

    /// Exchange DE and HL
    pub fn ex_de_hl(&mut self) {
        std::mem::swap(&mut self.de, &mut self.hl);
    }

    // ========== Address Masking ==========

    /// Mask address based on ADL mode
    #[inline]
    pub fn mask_addr(&self, addr: u32) -> u32 {
        if self.adl {
            addr & 0xFFFFFF // 24-bit in ADL mode
        } else {
            ((self.mbase as u32) << 16) | (addr & 0xFFFF) // 16-bit with MBASE
        }
    }

    /// Get effective address width based on ADL mode
    #[inline]
    pub fn addr_width(&self) -> u8 {
        if self.adl { 3 } else { 2 }
    }

    // ========== Instruction Fetch ==========

    /// Fetch byte at PC and increment PC
    #[inline]
    pub fn fetch_byte(&mut self, bus: &mut Bus) -> u8 {
        let byte = bus.read_byte(self.pc);
        self.pc = self.mask_addr(self.pc.wrapping_add(1));
        self.r = (self.r & 0x80) | ((self.r.wrapping_add(1)) & 0x7F);
        byte
    }

    /// Fetch 16-bit word at PC (little-endian)
    #[inline]
    pub fn fetch_word(&mut self, bus: &mut Bus) -> u16 {
        let lo = self.fetch_byte(bus) as u16;
        let hi = self.fetch_byte(bus) as u16;
        lo | (hi << 8)
    }

    /// Fetch 24-bit address at PC (little-endian, for ADL mode)
    #[inline]
    pub fn fetch_addr(&mut self, bus: &mut Bus) -> u32 {
        if self.adl {
            let b0 = self.fetch_byte(bus) as u32;
            let b1 = self.fetch_byte(bus) as u32;
            let b2 = self.fetch_byte(bus) as u32;
            b0 | (b1 << 8) | (b2 << 16)
        } else {
            self.fetch_word(bus) as u32
        }
    }

    // ========== Stack Operations ==========

    /// Push a byte onto the stack
    #[inline]
    pub fn push_byte(&mut self, bus: &mut Bus, val: u8) {
        self.sp = self.mask_addr(self.sp.wrapping_sub(1));
        bus.write_byte(self.sp, val);
    }

    /// Pop a byte from the stack
    #[inline]
    pub fn pop_byte(&mut self, bus: &mut Bus) -> u8 {
        let val = bus.read_byte(self.sp);
        self.sp = self.mask_addr(self.sp.wrapping_add(1));
        val
    }

    /// Push a word (16-bit) onto the stack
    #[inline]
    pub fn push_word(&mut self, bus: &mut Bus, val: u16) {
        self.push_byte(bus, (val >> 8) as u8);
        self.push_byte(bus, val as u8);
    }

    /// Pop a word (16-bit) from the stack
    #[inline]
    pub fn pop_word(&mut self, bus: &mut Bus) -> u16 {
        let lo = self.pop_byte(bus) as u16;
        let hi = self.pop_byte(bus) as u16;
        lo | (hi << 8)
    }

    /// Push address (24-bit in ADL, 16-bit otherwise)
    #[inline]
    pub fn push_addr(&mut self, bus: &mut Bus, val: u32) {
        if self.adl {
            self.push_byte(bus, (val >> 16) as u8);
            self.push_byte(bus, (val >> 8) as u8);
            self.push_byte(bus, val as u8);
        } else {
            self.push_word(bus, val as u16);
        }
    }

    /// Pop address (24-bit in ADL, 16-bit otherwise)
    #[inline]
    pub fn pop_addr(&mut self, bus: &mut Bus) -> u32 {
        if self.adl {
            let lo = self.pop_byte(bus) as u32;
            let mid = self.pop_byte(bus) as u32;
            let hi = self.pop_byte(bus) as u32;
            lo | (mid << 8) | (hi << 16)
        } else {
            self.pop_word(bus) as u32
        }
    }

    // ========== ALU Operations ==========

    /// Add with flags (used by ADD and ADC)
    fn alu_add(&mut self, val: u8, carry: bool) -> u8 {
        let c = if carry && self.flag_c() { 1u16 } else { 0 };
        let result = self.a as u16 + val as u16 + c;

        // Half-carry: carry from bit 3 to bit 4
        let half = ((self.a & 0x0F) + (val & 0x0F) + c as u8) > 0x0F;

        // Overflow: sign of result differs from expected
        let overflow = ((self.a ^ val) & 0x80 == 0) && ((self.a ^ result as u8) & 0x80 != 0);

        self.f = 0;
        self.set_sz_flags(result as u8);
        self.set_flag_c(result > 0xFF);
        self.set_flag_h(half);
        self.set_flag_pv(overflow);
        self.set_flag_n(false);

        result as u8
    }

    /// Subtract with flags (used by SUB, SBC, CP)
    fn alu_sub(&mut self, val: u8, carry: bool, store: bool) -> u8 {
        let c = if carry && self.flag_c() { 1u16 } else { 0 };
        let result = (self.a as u16).wrapping_sub(val as u16).wrapping_sub(c);

        // Half-carry (borrow from bit 4)
        let half = (self.a & 0x0F) < (val & 0x0F) + c as u8;

        // Overflow
        let overflow = ((self.a ^ val) & 0x80 != 0) && ((self.a ^ result as u8) & 0x80 != 0);

        self.f = 0;
        if store {
            self.set_sz_flags(result as u8);
        } else {
            // For CP, set F5/F3 from operand, not result
            self.set_sz_flags(result as u8);
            self.f = (self.f & !(flags::F5 | flags::F3)) | (val & (flags::F5 | flags::F3));
        }
        self.set_flag_c(result > 0xFF);
        self.set_flag_h(half);
        self.set_flag_pv(overflow);
        self.set_flag_n(true);

        result as u8
    }

    /// AND operation
    fn alu_and(&mut self, val: u8) {
        self.a &= val;
        self.f = 0;
        self.set_sz_flags(self.a);
        self.set_flag_h(true);
        self.set_flag_pv(Self::parity(self.a));
    }

    /// OR operation
    fn alu_or(&mut self, val: u8) {
        self.a |= val;
        self.f = 0;
        self.set_sz_flags(self.a);
        self.set_flag_pv(Self::parity(self.a));
    }

    /// XOR operation
    fn alu_xor(&mut self, val: u8) {
        self.a ^= val;
        self.f = 0;
        self.set_sz_flags(self.a);
        self.set_flag_pv(Self::parity(self.a));
    }

    /// Increment 8-bit value with flags
    fn alu_inc(&mut self, val: u8) -> u8 {
        let result = val.wrapping_add(1);
        let half = (val & 0x0F) == 0x0F;
        let overflow = val == 0x7F;

        // Preserve carry, set other flags
        let old_c = self.flag_c();
        self.f = 0;
        self.set_sz_flags(result);
        self.set_flag_h(half);
        self.set_flag_pv(overflow);
        self.set_flag_c(old_c);

        result
    }

    /// Decrement 8-bit value with flags
    fn alu_dec(&mut self, val: u8) -> u8 {
        let result = val.wrapping_sub(1);
        let half = (val & 0x0F) == 0x00;
        let overflow = val == 0x80;

        // Preserve carry, set other flags
        let old_c = self.flag_c();
        self.f = 0;
        self.set_sz_flags(result);
        self.set_flag_h(half);
        self.set_flag_pv(overflow);
        self.set_flag_n(true);
        self.set_flag_c(old_c);

        result
    }

    // ========== Register Access by Index ==========

    /// Get 8-bit register by index (0=B, 1=C, 2=D, 3=E, 4=H, 5=L, 6=(HL), 7=A)
    fn get_reg8(&self, idx: u8, bus: &Bus) -> u8 {
        match idx {
            0 => self.b(),
            1 => self.c(),
            2 => self.d(),
            3 => self.e(),
            4 => self.h(),
            5 => self.l(),
            6 => bus.peek_byte(self.hl), // (HL)
            7 => self.a,
            _ => 0,
        }
    }

    /// Set 8-bit register by index
    fn set_reg8(&mut self, idx: u8, val: u8, bus: &mut Bus) {
        match idx {
            0 => self.set_b(val),
            1 => self.set_c(val),
            2 => self.set_d(val),
            3 => self.set_e(val),
            4 => self.set_h(val),
            5 => self.set_l(val),
            6 => bus.write_byte(self.hl, val), // (HL)
            7 => self.a = val,
            _ => {}
        }
    }

    /// Get 16/24-bit register pair by index (0=BC, 1=DE, 2=HL, 3=SP)
    fn get_rp(&self, idx: u8) -> u32 {
        match idx {
            0 => self.bc,
            1 => self.de,
            2 => self.hl,
            3 => self.sp,
            _ => 0,
        }
    }

    /// Set 16/24-bit register pair by index
    fn set_rp(&mut self, idx: u8, val: u32) {
        let masked = self.mask_addr(val);
        match idx {
            0 => self.bc = masked,
            1 => self.de = masked,
            2 => self.hl = masked,
            3 => self.sp = masked,
            _ => {}
        }
    }

    /// Get register pair for push/pop (0=BC, 1=DE, 2=HL, 3=AF)
    fn get_rp2(&self, idx: u8) -> u16 {
        match idx {
            0 => self.bc as u16,
            1 => self.de as u16,
            2 => self.hl as u16,
            3 => ((self.a as u16) << 8) | (self.f as u16),
            _ => 0,
        }
    }

    /// Set register pair for push/pop
    fn set_rp2(&mut self, idx: u8, val: u16) {
        match idx {
            0 => {
                self.set_b((val >> 8) as u8);
                self.set_c(val as u8);
            }
            1 => {
                self.set_d((val >> 8) as u8);
                self.set_e(val as u8);
            }
            2 => {
                self.set_h((val >> 8) as u8);
                self.set_l(val as u8);
            }
            3 => {
                self.a = (val >> 8) as u8;
                self.f = val as u8;
            }
            _ => {}
        }
    }

    /// Check condition code (0=NZ, 1=Z, 2=NC, 3=C, 4=PO, 5=PE, 6=P, 7=M)
    fn check_cc(&self, cc: u8) -> bool {
        match cc {
            0 => !self.flag_z(),  // NZ
            1 => self.flag_z(),   // Z
            2 => !self.flag_c(),  // NC
            3 => self.flag_c(),   // C
            4 => !self.flag_pv(), // PO (parity odd)
            5 => self.flag_pv(),  // PE (parity even)
            6 => !self.flag_s(),  // P (positive)
            7 => self.flag_s(),   // M (minus)
            _ => false,
        }
    }

    // ========== Instruction Execution ==========

    /// Execute one instruction, returns cycles used
    pub fn step(&mut self, bus: &mut Bus) -> u32 {
        if self.halted {
            // CPU is halted, just consume cycles
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
                    if y == 6 || z == 6 { 7 } else { 4 }
                }
            }
            2 => {
                // ALU A,r
                let val = self.get_reg8(z, bus);
                self.execute_alu(y, val);
                if z == 6 { 7 } else { 4 }
            }
            3 => self.execute_x3(bus, y, z, p, q),
            _ => 4, // Should not happen
        }
    }

    /// Execute x=0 opcodes
    fn execute_x0(&mut self, bus: &mut Bus, y: u8, z: u8, p: u8, q: u8) -> u32 {
        match z {
            0 => {
                match y {
                    0 => 4, // NOP
                    1 => {
                        // EX AF,AF'
                        self.ex_af();
                        4
                    }
                    2 => {
                        // DJNZ d
                        let d = self.fetch_byte(bus) as i8;
                        self.set_b(self.b().wrapping_sub(1));
                        if self.b() != 0 {
                            self.pc = self.mask_addr((self.pc as i32 + d as i32) as u32);
                            13
                        } else {
                            8
                        }
                    }
                    3 => {
                        // JR d
                        let d = self.fetch_byte(bus) as i8;
                        self.pc = self.mask_addr((self.pc as i32 + d as i32) as u32);
                        12
                    }
                    4..=7 => {
                        // JR cc,d
                        let d = self.fetch_byte(bus) as i8;
                        if self.check_cc(y - 4) {
                            self.pc = self.mask_addr((self.pc as i32 + d as i32) as u32);
                            12
                        } else {
                            7
                        }
                    }
                    _ => 4,
                }
            }
            1 => {
                if q == 0 {
                    // LD rp,nn
                    let nn = self.fetch_addr(bus);
                    self.set_rp(p, nn);
                    if self.adl { 10 } else { 10 }
                } else {
                    // ADD HL,rp
                    let hl = self.hl;
                    let rp = self.get_rp(p);
                    let result = hl.wrapping_add(rp);

                    // Set flags
                    let half = ((hl & 0xFFF) + (rp & 0xFFF)) > 0xFFF;
                    self.set_flag_h(half);
                    self.set_flag_n(false);
                    self.set_flag_c(result > if self.adl { 0xFFFFFF } else { 0xFFFF });

                    self.hl = self.mask_addr(result);
                    11
                }
            }
            2 => {
                match (p, q) {
                    (0, 0) => {
                        // LD (BC),A
                        bus.write_byte(self.bc, self.a);
                        7
                    }
                    (1, 0) => {
                        // LD (DE),A
                        bus.write_byte(self.de, self.a);
                        7
                    }
                    (2, 0) => {
                        // LD (nn),HL
                        let nn = self.fetch_addr(bus);
                        bus.write_byte(nn, self.l());
                        bus.write_byte(nn.wrapping_add(1), self.h());
                        if self.adl {
                            bus.write_byte(nn.wrapping_add(2), (self.hl >> 16) as u8);
                            20
                        } else {
                            16
                        }
                    }
                    (3, 0) => {
                        // LD (nn),A
                        let nn = self.fetch_addr(bus);
                        bus.write_byte(nn, self.a);
                        13
                    }
                    (0, 1) => {
                        // LD A,(BC)
                        self.a = bus.read_byte(self.bc);
                        7
                    }
                    (1, 1) => {
                        // LD A,(DE)
                        self.a = bus.read_byte(self.de);
                        7
                    }
                    (2, 1) => {
                        // LD HL,(nn)
                        let nn = self.fetch_addr(bus);
                        let l = bus.read_byte(nn);
                        let h = bus.read_byte(nn.wrapping_add(1));
                        self.set_l(l);
                        self.set_h(h);
                        if self.adl {
                            let u = bus.read_byte(nn.wrapping_add(2));
                            self.hl = (self.hl & 0xFFFF) | ((u as u32) << 16);
                            20
                        } else {
                            16
                        }
                    }
                    (3, 1) => {
                        // LD A,(nn)
                        let nn = self.fetch_addr(bus);
                        self.a = bus.read_byte(nn);
                        13
                    }
                    _ => 4,
                }
            }
            3 => {
                if q == 0 {
                    // INC rp
                    let rp = self.get_rp(p).wrapping_add(1);
                    self.set_rp(p, rp);
                    6
                } else {
                    // DEC rp
                    let rp = self.get_rp(p).wrapping_sub(1);
                    self.set_rp(p, rp);
                    6
                }
            }
            4 => {
                // INC r
                let val = self.get_reg8(y, bus);
                let result = self.alu_inc(val);
                self.set_reg8(y, result, bus);
                if y == 6 { 11 } else { 4 }
            }
            5 => {
                // DEC r
                let val = self.get_reg8(y, bus);
                let result = self.alu_dec(val);
                self.set_reg8(y, result, bus);
                if y == 6 { 11 } else { 4 }
            }
            6 => {
                // LD r,n
                let n = self.fetch_byte(bus);
                self.set_reg8(y, n, bus);
                if y == 6 { 10 } else { 7 }
            }
            7 => {
                match y {
                    0 => {
                        // RLCA
                        let c = (self.a >> 7) & 1;
                        self.a = (self.a << 1) | c;
                        self.set_flag_c(c != 0);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        self.f = (self.f & !(flags::F5 | flags::F3)) | (self.a & (flags::F5 | flags::F3));
                        4
                    }
                    1 => {
                        // RRCA
                        let c = self.a & 1;
                        self.a = (self.a >> 1) | (c << 7);
                        self.set_flag_c(c != 0);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        self.f = (self.f & !(flags::F5 | flags::F3)) | (self.a & (flags::F5 | flags::F3));
                        4
                    }
                    2 => {
                        // RLA
                        let old_c = if self.flag_c() { 1 } else { 0 };
                        let new_c = (self.a >> 7) & 1;
                        self.a = (self.a << 1) | old_c;
                        self.set_flag_c(new_c != 0);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        self.f = (self.f & !(flags::F5 | flags::F3)) | (self.a & (flags::F5 | flags::F3));
                        4
                    }
                    3 => {
                        // RRA
                        let old_c = if self.flag_c() { 0x80 } else { 0 };
                        let new_c = self.a & 1;
                        self.a = (self.a >> 1) | old_c;
                        self.set_flag_c(new_c != 0);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        self.f = (self.f & !(flags::F5 | flags::F3)) | (self.a & (flags::F5 | flags::F3));
                        4
                    }
                    4 => {
                        // DAA - Decimal Adjust Accumulator
                        let mut correction: u8 = 0;
                        let mut set_carry = false;

                        if self.flag_h() || (!self.flag_n() && (self.a & 0x0F) > 9) {
                            correction |= 0x06;
                        }

                        if self.flag_c() || (!self.flag_n() && self.a > 0x99) {
                            correction |= 0x60;
                            set_carry = true;
                        }

                        if self.flag_n() {
                            self.a = self.a.wrapping_sub(correction);
                        } else {
                            self.a = self.a.wrapping_add(correction);
                        }

                        self.set_sz_flags(self.a);
                        self.set_flag_h(false); // H is always cleared on Z80
                        self.set_flag_pv(Self::parity(self.a));
                        if set_carry {
                            self.set_flag_c(true);
                        }
                        4
                    }
                    5 => {
                        // CPL
                        self.a = !self.a;
                        self.set_flag_h(true);
                        self.set_flag_n(true);
                        self.f = (self.f & !(flags::F5 | flags::F3)) | (self.a & (flags::F5 | flags::F3));
                        4
                    }
                    6 => {
                        // SCF
                        self.set_flag_c(true);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        self.f = (self.f & !(flags::F5 | flags::F3)) | (self.a & (flags::F5 | flags::F3));
                        4
                    }
                    7 => {
                        // CCF
                        let old_c = self.flag_c();
                        self.set_flag_h(old_c);
                        self.set_flag_c(!old_c);
                        self.set_flag_n(false);
                        self.f = (self.f & !(flags::F5 | flags::F3)) | (self.a & (flags::F5 | flags::F3));
                        4
                    }
                    _ => 4,
                }
            }
            _ => 4,
        }
    }

    /// Execute ALU operation (x=2)
    fn execute_alu(&mut self, y: u8, val: u8) {
        match y {
            0 => self.a = self.alu_add(val, false),    // ADD
            1 => self.a = self.alu_add(val, true),     // ADC
            2 => self.a = self.alu_sub(val, false, true),  // SUB
            3 => self.a = self.alu_sub(val, true, true),   // SBC
            4 => self.alu_and(val),                    // AND
            5 => self.alu_xor(val),                    // XOR
            6 => self.alu_or(val),                     // OR
            7 => { self.alu_sub(val, false, false); }  // CP
            _ => {}
        }
    }

    /// Execute x=3 opcodes
    fn execute_x3(&mut self, bus: &mut Bus, y: u8, z: u8, p: u8, q: u8) -> u32 {
        match z {
            0 => {
                // RET cc
                if self.check_cc(y) {
                    self.pc = self.pop_addr(bus);
                    if self.adl { 12 } else { 11 }
                } else {
                    5
                }
            }
            1 => {
                if q == 0 {
                    // POP rp2
                    let val = self.pop_word(bus);
                    self.set_rp2(p, val);
                    10
                } else {
                    match p {
                        0 => {
                            // RET
                            self.pc = self.pop_addr(bus);
                            if self.adl { 10 } else { 10 }
                        }
                        1 => {
                            // EXX
                            self.exx();
                            4
                        }
                        2 => {
                            // JP (HL)
                            self.pc = self.hl;
                            4
                        }
                        3 => {
                            // LD SP,HL
                            self.sp = self.hl;
                            6
                        }
                        _ => 4,
                    }
                }
            }
            2 => {
                // JP cc,nn
                let nn = self.fetch_addr(bus);
                if self.check_cc(y) {
                    self.pc = nn;
                }
                10
            }
            3 => {
                match y {
                    0 => {
                        // JP nn
                        self.pc = self.fetch_addr(bus);
                        10
                    }
                    1 => {
                        // CB prefix (bit operations)
                        self.execute_cb(bus)
                    }
                    2 => {
                        // OUT (n),A - blocked on TI-84 CE
                        let _n = self.fetch_byte(bus);
                        11
                    }
                    3 => {
                        // IN A,(n) - blocked on TI-84 CE
                        let _n = self.fetch_byte(bus);
                        self.a = 0xFF; // Garbage
                        11
                    }
                    4 => {
                        // EX (SP),HL
                        let sp_val = if self.adl {
                            bus.read_addr24(self.sp)
                        } else {
                            bus.read_word(self.sp) as u32
                        };
                        if self.adl {
                            bus.write_addr24(self.sp, self.hl);
                        } else {
                            bus.write_word(self.sp, self.hl as u16);
                        }
                        self.hl = sp_val;
                        19
                    }
                    5 => {
                        // EX DE,HL
                        self.ex_de_hl();
                        4
                    }
                    6 => {
                        // DI
                        self.iff1 = false;
                        self.iff2 = false;
                        4
                    }
                    7 => {
                        // EI
                        self.iff1 = true;
                        self.iff2 = true;
                        4
                    }
                    _ => 4,
                }
            }
            4 => {
                // CALL cc,nn
                let nn = self.fetch_addr(bus);
                if self.check_cc(y) {
                    self.push_addr(bus, self.pc);
                    self.pc = nn;
                    if self.adl { 20 } else { 17 }
                } else {
                    if self.adl { 13 } else { 10 }
                }
            }
            5 => {
                if q == 0 {
                    // PUSH rp2
                    let val = self.get_rp2(p);
                    self.push_word(bus, val);
                    11
                } else {
                    match p {
                        0 => {
                            // CALL nn
                            let nn = self.fetch_addr(bus);
                            self.push_addr(bus, self.pc);
                            self.pc = nn;
                            if self.adl { 20 } else { 17 }
                        }
                        1 => {
                            // DD prefix (IX instructions)
                            self.execute_index(bus, true)
                        }
                        2 => {
                            // ED prefix (extended instructions)
                            self.execute_ed(bus)
                        }
                        3 => {
                            // FD prefix (IY instructions)
                            self.execute_index(bus, false)
                        }
                        _ => 4,
                    }
                }
            }
            6 => {
                // ALU A,n
                let n = self.fetch_byte(bus);
                self.execute_alu(y, n);
                7
            }
            7 => {
                // RST y*8
                self.push_addr(bus, self.pc);
                self.pc = (y as u32) * 8;
                11
            }
            _ => 4,
        }
    }

    // ========== CB Prefix (Bit Operations) ==========

    /// Execute CB-prefixed instruction (bit operations)
    fn execute_cb(&mut self, bus: &mut Bus) -> u32 {
        let opcode = self.fetch_byte(bus);
        let x = (opcode >> 6) & 0x03;
        let y = (opcode >> 3) & 0x07;
        let z = opcode & 0x07;

        let val = self.get_reg8(z, bus);
        let cycles = if z == 6 { 15 } else { 8 };

        match x {
            0 => {
                // Rotate/shift operations
                let result = self.execute_rot(y, val);
                self.set_reg8(z, result, bus);
                cycles
            }
            1 => {
                // BIT y, r[z] - test bit
                let mask = 1 << y;
                let result = val & mask;

                // Set flags: Z if bit is zero, S from bit 7 if testing bit 7
                self.f &= flags::C; // Preserve carry
                self.set_flag_z(result == 0);
                self.set_flag_h(true);
                self.set_flag_n(false);
                self.set_flag_pv(result == 0); // PV is same as Z for BIT
                if y == 7 && result != 0 {
                    self.f |= flags::S;
                }
                // F3/F5 are from the value being tested (for register) or from high byte of address (for (HL))
                if z == 6 {
                    // For (HL), F3/F5 come from high byte of HL
                    self.f = (self.f & !(flags::F5 | flags::F3)) | ((self.h() as u8) & (flags::F5 | flags::F3));
                } else {
                    self.f = (self.f & !(flags::F5 | flags::F3)) | (val & (flags::F5 | flags::F3));
                }
                if z == 6 { 12 } else { 8 }
            }
            2 => {
                // RES y, r[z] - reset bit
                let result = val & !(1 << y);
                self.set_reg8(z, result, bus);
                cycles
            }
            3 => {
                // SET y, r[z] - set bit
                let result = val | (1 << y);
                self.set_reg8(z, result, bus);
                cycles
            }
            _ => 8,
        }
    }

    /// Execute rotate/shift operation (CB prefix, x=0)
    fn execute_rot(&mut self, y: u8, val: u8) -> u8 {
        let result = match y {
            0 => {
                // RLC - rotate left circular
                let c = (val >> 7) & 1;
                self.set_flag_c(c != 0);
                (val << 1) | c
            }
            1 => {
                // RRC - rotate right circular
                let c = val & 1;
                self.set_flag_c(c != 0);
                (val >> 1) | (c << 7)
            }
            2 => {
                // RL - rotate left through carry
                let old_c = if self.flag_c() { 1 } else { 0 };
                self.set_flag_c((val >> 7) & 1 != 0);
                (val << 1) | old_c
            }
            3 => {
                // RR - rotate right through carry
                let old_c = if self.flag_c() { 0x80 } else { 0 };
                self.set_flag_c(val & 1 != 0);
                (val >> 1) | old_c
            }
            4 => {
                // SLA - shift left arithmetic
                self.set_flag_c((val >> 7) & 1 != 0);
                val << 1
            }
            5 => {
                // SRA - shift right arithmetic (preserve sign)
                self.set_flag_c(val & 1 != 0);
                (val >> 1) | (val & 0x80)
            }
            6 => {
                // SLL - shift left logical (undocumented, sets bit 0)
                self.set_flag_c((val >> 7) & 1 != 0);
                (val << 1) | 1
            }
            7 => {
                // SRL - shift right logical
                self.set_flag_c(val & 1 != 0);
                val >> 1
            }
            _ => val,
        };

        // Set S, Z, P flags
        self.set_flag_h(false);
        self.set_flag_n(false);
        self.set_sz_flags(result);
        self.set_flag_pv(Self::parity(result));

        result
    }

    // ========== ED Prefix (Extended Instructions) ==========

    /// Execute ED-prefixed instruction
    fn execute_ed(&mut self, bus: &mut Bus) -> u32 {
        let opcode = self.fetch_byte(bus);
        let x = (opcode >> 6) & 0x03;
        let y = (opcode >> 3) & 0x07;
        let z = opcode & 0x07;
        let p = (y >> 1) & 0x03;
        let q = y & 0x01;

        match x {
            1 => self.execute_ed_x1(bus, y, z, p, q),
            2 => {
                // Block instructions (y >= 4, z <= 3)
                if y >= 4 && z <= 3 {
                    self.execute_bli(bus, y, z)
                } else {
                    8 // NOP for invalid
                }
            }
            _ => 8, // x=0 and x=3 are NONI (no operation, no interrupt)
        }
    }

    /// Execute ED prefix x=1 opcodes
    fn execute_ed_x1(&mut self, bus: &mut Bus, y: u8, z: u8, p: u8, q: u8) -> u32 {
        match z {
            0 => {
                // IN r,(C) - returns garbage on TI-84 CE
                let val = 0xFF; // I/O blocked, return garbage
                if y != 6 {
                    self.set_reg8(y, val, bus);
                }
                // Set flags for IN (except for y=6 which is just IN F,(C))
                self.set_sz_flags(val);
                self.set_flag_h(false);
                self.set_flag_n(false);
                self.set_flag_pv(Self::parity(val));
                12
            }
            1 => {
                // OUT (C),r - blocked on TI-84 CE
                // Just consume cycles, no actual output
                12
            }
            2 => {
                if q == 0 {
                    // SBC HL,rp
                    let hl = self.hl;
                    let rp = self.get_rp(p);
                    let c = if self.flag_c() { 1u32 } else { 0 };
                    let result = hl.wrapping_sub(rp).wrapping_sub(c);

                    // Flags
                    let half = (hl & 0xFFF) < (rp & 0xFFF) + c;
                    let overflow = ((hl ^ rp) & 0x800000 != 0) && ((hl ^ result) & 0x800000 != 0);

                    self.hl = self.mask_addr(result);

                    self.f = 0;
                    self.set_flag_s((self.hl >> (if self.adl { 23 } else { 15 })) & 1 != 0);
                    self.set_flag_z(self.hl == 0);
                    self.set_flag_h(half);
                    self.set_flag_pv(overflow);
                    self.set_flag_n(true);
                    self.set_flag_c(hl < rp + c);
                    // F3/F5 from high byte of result
                    let high_byte = if self.adl { (self.hl >> 16) as u8 } else { (self.hl >> 8) as u8 };
                    self.f = (self.f & !(flags::F5 | flags::F3)) | (high_byte & (flags::F5 | flags::F3));
                    15
                } else {
                    // ADC HL,rp
                    let hl = self.hl;
                    let rp = self.get_rp(p);
                    let c = if self.flag_c() { 1u32 } else { 0 };
                    let result = hl.wrapping_add(rp).wrapping_add(c);

                    // Flags
                    let half = ((hl & 0xFFF) + (rp & 0xFFF) + c) > 0xFFF;
                    let overflow = ((hl ^ rp) & 0x800000 == 0) && ((hl ^ result) & 0x800000 != 0);
                    let max = if self.adl { 0xFFFFFF } else { 0xFFFF };

                    self.hl = self.mask_addr(result);

                    self.f = 0;
                    self.set_flag_s((self.hl >> (if self.adl { 23 } else { 15 })) & 1 != 0);
                    self.set_flag_z(self.hl == 0);
                    self.set_flag_h(half);
                    self.set_flag_pv(overflow);
                    self.set_flag_n(false);
                    self.set_flag_c(result > max);
                    let high_byte = if self.adl { (self.hl >> 16) as u8 } else { (self.hl >> 8) as u8 };
                    self.f = (self.f & !(flags::F5 | flags::F3)) | (high_byte & (flags::F5 | flags::F3));
                    15
                }
            }
            3 => {
                // LD (nn),rp / LD rp,(nn)
                let nn = self.fetch_addr(bus);
                if q == 0 {
                    // LD (nn),rp
                    let rp = self.get_rp(p);
                    if self.adl {
                        bus.write_addr24(nn, rp);
                        23
                    } else {
                        bus.write_word(nn, rp as u16);
                        20
                    }
                } else {
                    // LD rp,(nn)
                    let val = if self.adl {
                        bus.read_addr24(nn)
                    } else {
                        bus.read_word(nn) as u32
                    };
                    self.set_rp(p, val);
                    if self.adl { 23 } else { 20 }
                }
            }
            4 => {
                // NEG
                let old_a = self.a;
                self.a = 0u8.wrapping_sub(old_a);

                self.f = 0;
                self.set_sz_flags(self.a);
                self.set_flag_h((0 & 0x0F) < (old_a & 0x0F));
                self.set_flag_pv(old_a == 0x80);
                self.set_flag_n(true);
                self.set_flag_c(old_a != 0);
                8
            }
            5 => {
                if q == 0 {
                    // RETN
                    self.iff1 = self.iff2;
                    self.pc = self.pop_addr(bus);
                    14
                } else {
                    // RETI
                    self.pc = self.pop_addr(bus);
                    14
                }
            }
            6 => {
                // IM 0/1/2
                // ED 46 (y=0) -> IM 0
                // ED 56 (y=2) -> IM 1
                // ED 5E (y=3) -> IM 2
                // ED 66 (y=4) -> IM 0
                // ED 6E (y=5) -> IM 0/1 (undocumented, treat as IM 0)
                // ED 76 (y=6) -> IM 1
                // ED 7E (y=7) -> IM 2
                match y {
                    0 | 1 | 4 | 5 => self.im = InterruptMode::Mode0,
                    2 | 6 => self.im = InterruptMode::Mode1,
                    3 | 7 => self.im = InterruptMode::Mode2,
                    _ => {}
                }
                8
            }
            7 => {
                match y {
                    0 => {
                        // LD I,A
                        self.i = self.a as u16;
                        9
                    }
                    1 => {
                        // LD R,A
                        self.r = self.a;
                        9
                    }
                    2 => {
                        // LD A,I
                        self.a = self.i as u8;
                        self.set_sz_flags(self.a);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        self.set_flag_pv(self.iff2);
                        9
                    }
                    3 => {
                        // LD A,R
                        self.a = self.r;
                        self.set_sz_flags(self.a);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        self.set_flag_pv(self.iff2);
                        9
                    }
                    4 => {
                        // RRD
                        let mem = bus.read_byte(self.hl);
                        let new_mem = (self.a << 4) | (mem >> 4);
                        self.a = (self.a & 0xF0) | (mem & 0x0F);
                        bus.write_byte(self.hl, new_mem);

                        self.set_sz_flags(self.a);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        self.set_flag_pv(Self::parity(self.a));
                        18
                    }
                    5 => {
                        // RLD
                        let mem = bus.read_byte(self.hl);
                        let new_mem = (mem << 4) | (self.a & 0x0F);
                        self.a = (self.a & 0xF0) | (mem >> 4);
                        bus.write_byte(self.hl, new_mem);

                        self.set_sz_flags(self.a);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        self.set_flag_pv(Self::parity(self.a));
                        18
                    }
                    _ => 8, // NOP for 6,7
                }
            }
            _ => 8,
        }
    }

    /// Execute block instructions (ED prefix, x=2)
    fn execute_bli(&mut self, bus: &mut Bus, y: u8, z: u8) -> u32 {
        match (y, z) {
            // LDI - Load and increment
            (4, 0) => {
                let val = bus.read_byte(self.hl);
                bus.write_byte(self.de, val);
                self.hl = self.mask_addr(self.hl.wrapping_add(1));
                self.de = self.mask_addr(self.de.wrapping_add(1));
                self.bc = self.mask_addr(self.bc.wrapping_sub(1));

                self.set_flag_h(false);
                self.set_flag_n(false);
                self.set_flag_pv(self.bc != 0);
                let n = val.wrapping_add(self.a);
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);
                16
            }
            // LDD - Load and decrement
            (5, 0) => {
                let val = bus.read_byte(self.hl);
                bus.write_byte(self.de, val);
                self.hl = self.mask_addr(self.hl.wrapping_sub(1));
                self.de = self.mask_addr(self.de.wrapping_sub(1));
                self.bc = self.mask_addr(self.bc.wrapping_sub(1));

                self.set_flag_h(false);
                self.set_flag_n(false);
                self.set_flag_pv(self.bc != 0);
                let n = val.wrapping_add(self.a);
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);
                16
            }
            // LDIR - Load, increment, repeat
            (6, 0) => {
                let val = bus.read_byte(self.hl);
                bus.write_byte(self.de, val);
                self.hl = self.mask_addr(self.hl.wrapping_add(1));
                self.de = self.mask_addr(self.de.wrapping_add(1));
                self.bc = self.mask_addr(self.bc.wrapping_sub(1));

                self.set_flag_h(false);
                self.set_flag_n(false);
                self.set_flag_pv(false);
                let n = val.wrapping_add(self.a);
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);

                if self.bc != 0 {
                    self.pc = self.mask_addr(self.pc.wrapping_sub(2));
                    21
                } else {
                    16
                }
            }
            // LDDR - Load, decrement, repeat
            (7, 0) => {
                let val = bus.read_byte(self.hl);
                bus.write_byte(self.de, val);
                self.hl = self.mask_addr(self.hl.wrapping_sub(1));
                self.de = self.mask_addr(self.de.wrapping_sub(1));
                self.bc = self.mask_addr(self.bc.wrapping_sub(1));

                self.set_flag_h(false);
                self.set_flag_n(false);
                self.set_flag_pv(false);
                let n = val.wrapping_add(self.a);
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);

                if self.bc != 0 {
                    self.pc = self.mask_addr(self.pc.wrapping_sub(2));
                    21
                } else {
                    16
                }
            }
            // CPI - Compare and increment
            (4, 1) => {
                let val = bus.read_byte(self.hl);
                let result = self.a.wrapping_sub(val);
                self.hl = self.mask_addr(self.hl.wrapping_add(1));
                self.bc = self.mask_addr(self.bc.wrapping_sub(1));

                self.set_sz_flags(result);
                self.set_flag_h((self.a & 0x0F) < (val & 0x0F));
                self.set_flag_n(true);
                self.set_flag_pv(self.bc != 0);
                // F3/F5 handling for CPI is complex
                let n = result.wrapping_sub(if self.flag_h() { 1 } else { 0 });
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);
                16
            }
            // CPD - Compare and decrement
            (5, 1) => {
                let val = bus.read_byte(self.hl);
                let result = self.a.wrapping_sub(val);
                self.hl = self.mask_addr(self.hl.wrapping_sub(1));
                self.bc = self.mask_addr(self.bc.wrapping_sub(1));

                self.set_sz_flags(result);
                self.set_flag_h((self.a & 0x0F) < (val & 0x0F));
                self.set_flag_n(true);
                self.set_flag_pv(self.bc != 0);
                let n = result.wrapping_sub(if self.flag_h() { 1 } else { 0 });
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);
                16
            }
            // CPIR - Compare, increment, repeat
            (6, 1) => {
                let val = bus.read_byte(self.hl);
                let result = self.a.wrapping_sub(val);
                self.hl = self.mask_addr(self.hl.wrapping_add(1));
                self.bc = self.mask_addr(self.bc.wrapping_sub(1));

                self.set_sz_flags(result);
                self.set_flag_h((self.a & 0x0F) < (val & 0x0F));
                self.set_flag_n(true);
                self.set_flag_pv(self.bc != 0);
                let n = result.wrapping_sub(if self.flag_h() { 1 } else { 0 });
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);

                if self.bc != 0 && result != 0 {
                    self.pc = self.mask_addr(self.pc.wrapping_sub(2));
                    21
                } else {
                    16
                }
            }
            // CPDR - Compare, decrement, repeat
            (7, 1) => {
                let val = bus.read_byte(self.hl);
                let result = self.a.wrapping_sub(val);
                self.hl = self.mask_addr(self.hl.wrapping_sub(1));
                self.bc = self.mask_addr(self.bc.wrapping_sub(1));

                self.set_sz_flags(result);
                self.set_flag_h((self.a & 0x0F) < (val & 0x0F));
                self.set_flag_n(true);
                self.set_flag_pv(self.bc != 0);
                let n = result.wrapping_sub(if self.flag_h() { 1 } else { 0 });
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);

                if self.bc != 0 && result != 0 {
                    self.pc = self.mask_addr(self.pc.wrapping_sub(2));
                    21
                } else {
                    16
                }
            }
            // INI, IND, INIR, INDR - I/O blocked on TI-84 CE
            (4, 2) | (5, 2) | (6, 2) | (7, 2) => 16,
            // OUTI, OUTD, OTIR, OTDR - I/O blocked on TI-84 CE
            (4, 3) | (5, 3) | (6, 3) | (7, 3) => 16,
            _ => 8,
        }
    }

    // ========== DD/FD Prefix (IX/IY Instructions) ==========

    /// Execute DD/FD prefixed instruction (IX/IY indexed)
    /// use_ix: true for DD (IX), false for FD (IY)
    fn execute_index(&mut self, bus: &mut Bus, use_ix: bool) -> u32 {
        let opcode = self.fetch_byte(bus);

        // Handle DD CB / FD CB prefix (bit operations on indexed memory)
        if opcode == 0xCB {
            return self.execute_index_cb(bus, use_ix);
        }

        // Handle DD ED / FD ED - ED prefix ignores DD/FD
        if opcode == 0xED {
            return self.execute_ed(bus);
        }

        // Handle DD DD / FD FD / DD FD / FD DD - chain of prefixes
        // Just restart with the new prefix
        if opcode == 0xDD {
            return self.execute_index(bus, true);
        }
        if opcode == 0xFD {
            return self.execute_index(bus, false);
        }

        let x = (opcode >> 6) & 0x03;
        let y = (opcode >> 3) & 0x07;
        let z = opcode & 0x07;
        let p = (y >> 1) & 0x03;
        let q = y & 0x01;

        match x {
            0 => self.execute_index_x0(bus, y, z, p, q, use_ix),
            1 => {
                if y == 6 && z == 6 {
                    // HALT - not affected by prefix
                    self.halted = true;
                    4
                } else {
                    // LD r,r' with index register modifications
                    // If either y or z is 4, 5, or 6, we use indexed addressing
                    let src = self.get_index_reg8(z, bus, use_ix);
                    self.set_index_reg8(y, src, bus, use_ix);
                    if y == 6 || z == 6 { 19 } else { 8 }
                }
            }
            2 => {
                // ALU A,r with indexed addressing
                let val = self.get_index_reg8(z, bus, use_ix);
                self.execute_alu(y, val);
                if z == 6 { 19 } else { 8 }
            }
            3 => self.execute_index_x3(bus, y, z, p, q, use_ix),
            _ => 8,
        }
    }

    /// Get 8-bit register with IX/IY substitution
    /// 4=IXH/IYH, 5=IXL/IYL, 6=(IX+d)/(IY+d)
    fn get_index_reg8(&mut self, idx: u8, bus: &mut Bus, use_ix: bool) -> u8 {
        match idx {
            0 => self.b(),
            1 => self.c(),
            2 => self.d(),
            3 => self.e(),
            4 => {
                // H -> IXH/IYH
                if use_ix { self.ixh() } else { self.iyh() }
            }
            5 => {
                // L -> IXL/IYL
                if use_ix { self.ixl() } else { self.iyl() }
            }
            6 => {
                // (HL) -> (IX+d)/(IY+d)
                let d = self.fetch_byte(bus) as i8;
                let index_reg = if use_ix { self.ix } else { self.iy };
                let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
                bus.read_byte(addr)
            }
            7 => self.a,
            _ => 0,
        }
    }

    /// Set 8-bit register with IX/IY substitution
    fn set_index_reg8(&mut self, idx: u8, val: u8, bus: &mut Bus, use_ix: bool) {
        match idx {
            0 => self.set_b(val),
            1 => self.set_c(val),
            2 => self.set_d(val),
            3 => self.set_e(val),
            4 => {
                // H -> IXH/IYH
                if use_ix { self.set_ixh(val) } else { self.set_iyh(val) }
            }
            5 => {
                // L -> IXL/IYL
                if use_ix { self.set_ixl(val) } else { self.set_iyl(val) }
            }
            6 => {
                // (HL) -> (IX+d)/(IY+d)
                let d = self.fetch_byte(bus) as i8;
                let index_reg = if use_ix { self.ix } else { self.iy };
                let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
                bus.write_byte(addr, val);
            }
            7 => self.a = val,
            _ => {}
        }
    }

    /// Execute indexed x=0 opcodes
    fn execute_index_x0(&mut self, bus: &mut Bus, y: u8, z: u8, p: u8, q: u8, use_ix: bool) -> u32 {
        match z {
            0 => {
                // These don't use HL, just execute normally
                match y {
                    0 => 4, // NOP
                    1 => { self.ex_af(); 4 }
                    2 => {
                        // DJNZ d
                        let d = self.fetch_byte(bus) as i8;
                        self.set_b(self.b().wrapping_sub(1));
                        if self.b() != 0 {
                            self.pc = self.mask_addr((self.pc as i32 + d as i32) as u32);
                            13
                        } else {
                            8
                        }
                    }
                    3 => {
                        // JR d
                        let d = self.fetch_byte(bus) as i8;
                        self.pc = self.mask_addr((self.pc as i32 + d as i32) as u32);
                        12
                    }
                    4..=7 => {
                        // JR cc,d
                        let d = self.fetch_byte(bus) as i8;
                        if self.check_cc(y - 4) {
                            self.pc = self.mask_addr((self.pc as i32 + d as i32) as u32);
                            12
                        } else {
                            7
                        }
                    }
                    _ => 4,
                }
            }
            1 => {
                if q == 0 {
                    if p == 2 {
                        // LD IX/IY,nn
                        let nn = self.fetch_addr(bus);
                        if use_ix { self.ix = nn; } else { self.iy = nn; }
                        if self.adl { 14 } else { 14 }
                    } else {
                        // LD rp,nn (not affected by prefix for BC/DE/SP)
                        let nn = self.fetch_addr(bus);
                        self.set_rp(p, nn);
                        if self.adl { 10 } else { 10 }
                    }
                } else {
                    if p == 2 {
                        // ADD IX/IY,rp
                        let index_reg = if use_ix { self.ix } else { self.iy };
                        let rp = self.get_index_rp(p, use_ix);
                        let result = index_reg.wrapping_add(rp);

                        let half = ((index_reg & 0xFFF) + (rp & 0xFFF)) > 0xFFF;
                        self.set_flag_h(half);
                        self.set_flag_n(false);
                        self.set_flag_c(result > if self.adl { 0xFFFFFF } else { 0xFFFF });

                        if use_ix { self.ix = self.mask_addr(result); } else { self.iy = self.mask_addr(result); }
                        15
                    } else {
                        // ADD IX/IY,rp (for BC/DE/SP)
                        let index_reg = if use_ix { self.ix } else { self.iy };
                        let rp = self.get_rp(p);
                        let result = index_reg.wrapping_add(rp);

                        let half = ((index_reg & 0xFFF) + (rp & 0xFFF)) > 0xFFF;
                        self.set_flag_h(half);
                        self.set_flag_n(false);
                        self.set_flag_c(result > if self.adl { 0xFFFFFF } else { 0xFFFF });

                        if use_ix { self.ix = self.mask_addr(result); } else { self.iy = self.mask_addr(result); }
                        15
                    }
                }
            }
            2 => {
                match (p, q) {
                    (2, 0) => {
                        // LD (nn),IX/IY
                        let nn = self.fetch_addr(bus);
                        let index_reg = if use_ix { self.ix } else { self.iy };
                        if self.adl {
                            bus.write_addr24(nn, index_reg);
                            20
                        } else {
                            bus.write_word(nn, index_reg as u16);
                            16
                        }
                    }
                    (2, 1) => {
                        // LD IX/IY,(nn)
                        let nn = self.fetch_addr(bus);
                        let val = if self.adl {
                            bus.read_addr24(nn)
                        } else {
                            bus.read_word(nn) as u32
                        };
                        if use_ix { self.ix = val; } else { self.iy = val; }
                        if self.adl { 20 } else { 16 }
                    }
                    _ => {
                        // These don't use HL, execute normally
                        self.execute_x0(bus, y, z, p, q)
                    }
                }
            }
            3 => {
                if p == 2 {
                    if q == 0 {
                        // INC IX/IY
                        if use_ix {
                            self.ix = self.mask_addr(self.ix.wrapping_add(1));
                        } else {
                            self.iy = self.mask_addr(self.iy.wrapping_add(1));
                        }
                        10
                    } else {
                        // DEC IX/IY
                        if use_ix {
                            self.ix = self.mask_addr(self.ix.wrapping_sub(1));
                        } else {
                            self.iy = self.mask_addr(self.iy.wrapping_sub(1));
                        }
                        10
                    }
                } else {
                    // INC/DEC rp (not affected for BC/DE/SP)
                    if q == 0 {
                        let rp = self.get_rp(p).wrapping_add(1);
                        self.set_rp(p, rp);
                        6
                    } else {
                        let rp = self.get_rp(p).wrapping_sub(1);
                        self.set_rp(p, rp);
                        6
                    }
                }
            }
            4 => {
                // INC r with indexed addressing
                let val = self.get_index_reg8(y, bus, use_ix);
                let result = self.alu_inc(val);
                // For (IX+d), we already fetched the displacement, need to re-fetch for write
                if y == 6 {
                    // Rewind and use cached displacement approach
                    // Actually, for simplicity, we need to read and write with same address
                    // Let's handle this specially
                    self.pc = self.mask_addr(self.pc.wrapping_sub(1)); // Rewind displacement
                    let d = self.fetch_byte(bus) as i8;
                    let index_reg = if use_ix { self.ix } else { self.iy };
                    let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
                    bus.write_byte(addr, result);
                    23
                } else {
                    self.set_index_reg8_no_disp(y, result, use_ix);
                    8
                }
            }
            5 => {
                // DEC r with indexed addressing
                let val = self.get_index_reg8(y, bus, use_ix);
                let result = self.alu_dec(val);
                if y == 6 {
                    self.pc = self.mask_addr(self.pc.wrapping_sub(1));
                    let d = self.fetch_byte(bus) as i8;
                    let index_reg = if use_ix { self.ix } else { self.iy };
                    let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
                    bus.write_byte(addr, result);
                    23
                } else {
                    self.set_index_reg8_no_disp(y, result, use_ix);
                    8
                }
            }
            6 => {
                // LD r,n with indexed addressing
                if y == 6 {
                    // LD (IX+d),n - displacement before immediate
                    let d = self.fetch_byte(bus) as i8;
                    let n = self.fetch_byte(bus);
                    let index_reg = if use_ix { self.ix } else { self.iy };
                    let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
                    bus.write_byte(addr, n);
                    19
                } else {
                    let n = self.fetch_byte(bus);
                    self.set_index_reg8_no_disp(y, n, use_ix);
                    11
                }
            }
            7 => {
                // These don't use HL, execute normally
                self.execute_x0(bus, y, z, p, q)
            }
            _ => 8,
        }
    }

    /// Set 8-bit register without fetching displacement (for IXH/IXL/IYH/IYL)
    fn set_index_reg8_no_disp(&mut self, idx: u8, val: u8, use_ix: bool) {
        match idx {
            0 => self.set_b(val),
            1 => self.set_c(val),
            2 => self.set_d(val),
            3 => self.set_e(val),
            4 => { if use_ix { self.set_ixh(val) } else { self.set_iyh(val) } }
            5 => { if use_ix { self.set_ixl(val) } else { self.set_iyl(val) } }
            7 => self.a = val,
            _ => {}
        }
    }

    /// Get register pair for indexed ADD
    fn get_index_rp(&self, p: u8, use_ix: bool) -> u32 {
        match p {
            0 => self.bc,
            1 => self.de,
            2 => if use_ix { self.ix } else { self.iy },
            3 => self.sp,
            _ => 0,
        }
    }

    /// Execute indexed x=3 opcodes
    fn execute_index_x3(&mut self, bus: &mut Bus, y: u8, z: u8, p: u8, q: u8, use_ix: bool) -> u32 {
        match z {
            0 => {
                // RET cc - not affected
                if self.check_cc(y) {
                    self.pc = self.pop_addr(bus);
                    if self.adl { 12 } else { 11 }
                } else {
                    5
                }
            }
            1 => {
                if q == 0 {
                    // POP rp2
                    if p == 2 {
                        // POP IX/IY
                        let val = if self.adl {
                            self.pop_addr(bus)
                        } else {
                            self.pop_word(bus) as u32
                        };
                        if use_ix { self.ix = val; } else { self.iy = val; }
                        14
                    } else {
                        let val = self.pop_word(bus);
                        self.set_rp2(p, val);
                        10
                    }
                } else {
                    match p {
                        0 => {
                            // RET
                            self.pc = self.pop_addr(bus);
                            10
                        }
                        1 => {
                            // EXX - not affected
                            self.exx();
                            4
                        }
                        2 => {
                            // JP (IX)/(IY)
                            let index_reg = if use_ix { self.ix } else { self.iy };
                            self.pc = index_reg;
                            8
                        }
                        3 => {
                            // LD SP,IX/IY
                            let index_reg = if use_ix { self.ix } else { self.iy };
                            self.sp = index_reg;
                            10
                        }
                        _ => 4,
                    }
                }
            }
            2 => {
                // JP cc,nn - not affected
                let nn = self.fetch_addr(bus);
                if self.check_cc(y) {
                    self.pc = nn;
                }
                10
            }
            3 => {
                match y {
                    0 => {
                        // JP nn - not affected
                        self.pc = self.fetch_addr(bus);
                        10
                    }
                    1 => {
                        // DD CB / FD CB - handled at top of execute_index
                        8
                    }
                    4 => {
                        // EX (SP),IX/IY
                        let sp_val = if self.adl {
                            bus.read_addr24(self.sp)
                        } else {
                            bus.read_word(self.sp) as u32
                        };
                        let index_reg = if use_ix { self.ix } else { self.iy };
                        if self.adl {
                            bus.write_addr24(self.sp, index_reg);
                        } else {
                            bus.write_word(self.sp, index_reg as u16);
                        }
                        if use_ix { self.ix = sp_val; } else { self.iy = sp_val; }
                        23
                    }
                    _ => {
                        // Other z=3 instructions not affected by prefix
                        self.execute_x3(bus, y, z, p, q)
                    }
                }
            }
            4 => {
                // CALL cc,nn - not affected
                let nn = self.fetch_addr(bus);
                if self.check_cc(y) {
                    self.push_addr(bus, self.pc);
                    self.pc = nn;
                    if self.adl { 20 } else { 17 }
                } else {
                    if self.adl { 13 } else { 10 }
                }
            }
            5 => {
                if q == 0 {
                    // PUSH rp2
                    if p == 2 {
                        // PUSH IX/IY
                        let index_reg = if use_ix { self.ix } else { self.iy };
                        if self.adl {
                            self.push_addr(bus, index_reg);
                        } else {
                            self.push_word(bus, index_reg as u16);
                        }
                        15
                    } else {
                        let val = self.get_rp2(p);
                        self.push_word(bus, val);
                        11
                    }
                } else {
                    match p {
                        0 => {
                            // CALL nn - not affected
                            let nn = self.fetch_addr(bus);
                            self.push_addr(bus, self.pc);
                            self.pc = nn;
                            if self.adl { 20 } else { 17 }
                        }
                        1 | 3 => {
                            // DD DD / DD FD / FD DD / FD FD - already handled at top
                            8
                        }
                        2 => {
                            // DD ED / FD ED - already handled at top
                            8
                        }
                        _ => 8,
                    }
                }
            }
            6 => {
                // ALU A,n - not affected
                let n = self.fetch_byte(bus);
                self.execute_alu(y, n);
                7
            }
            7 => {
                // RST - not affected
                self.push_addr(bus, self.pc);
                self.pc = (y as u32) * 8;
                11
            }
            _ => 8,
        }
    }

    /// Execute DD CB / FD CB prefixed instruction (bit operations on indexed memory)
    fn execute_index_cb(&mut self, bus: &mut Bus, use_ix: bool) -> u32 {
        // Format is: DD CB d op (or FD CB d op)
        // Displacement comes BEFORE the opcode!
        let d = self.fetch_byte(bus) as i8;
        let opcode = self.fetch_byte(bus);

        let x = (opcode >> 6) & 0x03;
        let y = (opcode >> 3) & 0x07;
        let z = opcode & 0x07;

        let index_reg = if use_ix { self.ix } else { self.iy };
        let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
        let val = bus.read_byte(addr);

        match x {
            0 => {
                // Rotate/shift on (IX+d)/(IY+d), optionally copy to register
                let result = self.execute_rot(y, val);
                bus.write_byte(addr, result);
                // If z != 6, also copy to register (undocumented)
                if z != 6 {
                    self.set_reg8(z, result, bus);
                }
                23
            }
            1 => {
                // BIT y,(IX+d)/(IY+d)
                let mask = 1 << y;
                let result = val & mask;

                self.f &= flags::C;
                self.set_flag_z(result == 0);
                self.set_flag_h(true);
                self.set_flag_n(false);
                self.set_flag_pv(result == 0);
                if y == 7 && result != 0 {
                    self.f |= flags::S;
                }
                // F3/F5 from high byte of address
                self.f = (self.f & !(flags::F5 | flags::F3)) | (((addr >> 8) as u8) & (flags::F5 | flags::F3));
                20
            }
            2 => {
                // RES y,(IX+d)/(IY+d), optionally copy to register
                let result = val & !(1 << y);
                bus.write_byte(addr, result);
                if z != 6 {
                    self.set_reg8(z, result, bus);
                }
                23
            }
            3 => {
                // SET y,(IX+d)/(IY+d), optionally copy to register
                let result = val | (1 << y);
                bus.write_byte(addr, result);
                if z != 6 {
                    self.set_reg8(z, result, bus);
                }
                23
            }
            _ => 23,
        }
    }
}

impl Default for Cpu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_cpu() {
        let cpu = Cpu::new();
        assert_eq!(cpu.pc, 0);
        assert!(cpu.adl);
        assert!(!cpu.halted);
        assert!(!cpu.iff1);
    }

    #[test]
    fn test_reset() {
        let mut cpu = Cpu::new();
        cpu.pc = 0x1234;
        cpu.halted = true;
        cpu.iff1 = true;
        cpu.reset();
        assert_eq!(cpu.pc, 0);
        assert!(!cpu.halted);
        assert!(!cpu.iff1);
    }

    #[test]
    fn test_register_b_c() {
        let mut cpu = Cpu::new();
        cpu.bc = 0x123456;
        assert_eq!(cpu.b(), 0x34);
        assert_eq!(cpu.c(), 0x56);

        cpu.set_b(0xAB);
        assert_eq!(cpu.bc, 0x12AB56);

        cpu.set_c(0xCD);
        assert_eq!(cpu.bc, 0x12ABCD);
    }

    #[test]
    fn test_register_d_e() {
        let mut cpu = Cpu::new();
        cpu.de = 0xAABBCC;
        assert_eq!(cpu.d(), 0xBB);
        assert_eq!(cpu.e(), 0xCC);

        cpu.set_d(0x11);
        cpu.set_e(0x22);
        assert_eq!(cpu.de, 0xAA1122);
    }

    #[test]
    fn test_register_h_l() {
        let mut cpu = Cpu::new();
        cpu.hl = 0xD01234;
        assert_eq!(cpu.h(), 0x12);
        assert_eq!(cpu.l(), 0x34);

        cpu.set_h(0x56);
        cpu.set_l(0x78);
        assert_eq!(cpu.hl, 0xD05678);
    }

    #[test]
    fn test_flags() {
        let mut cpu = Cpu::new();
        cpu.f = 0;

        cpu.set_flag_c(true);
        assert!(cpu.flag_c());
        assert_eq!(cpu.f, flags::C);

        cpu.set_flag_z(true);
        assert!(cpu.flag_z());

        cpu.set_flag_s(true);
        assert!(cpu.flag_s());

        cpu.set_flag_c(false);
        assert!(!cpu.flag_c());
    }

    #[test]
    fn test_sz_flags() {
        let mut cpu = Cpu::new();

        // Test zero
        cpu.f = 0;
        cpu.set_sz_flags(0);
        assert!(cpu.flag_z());
        assert!(!cpu.flag_s());

        // Test negative (sign bit set)
        cpu.f = 0;
        cpu.set_sz_flags(0x80);
        assert!(!cpu.flag_z());
        assert!(cpu.flag_s());

        // Test positive non-zero
        cpu.f = 0;
        cpu.set_sz_flags(0x42);
        assert!(!cpu.flag_z());
        assert!(!cpu.flag_s());
    }

    #[test]
    fn test_parity() {
        assert!(Cpu::parity(0x00)); // 0 bits set - even
        assert!(!Cpu::parity(0x01)); // 1 bit set - odd
        assert!(Cpu::parity(0x03)); // 2 bits set - even
        assert!(!Cpu::parity(0x07)); // 3 bits set - odd
        assert!(Cpu::parity(0xFF)); // 8 bits set - even
    }

    #[test]
    fn test_ex_af() {
        let mut cpu = Cpu::new();
        cpu.a = 0x12;
        cpu.f = 0x34;
        cpu.a_prime = 0xAB;
        cpu.f_prime = 0xCD;

        cpu.ex_af();

        assert_eq!(cpu.a, 0xAB);
        assert_eq!(cpu.f, 0xCD);
        assert_eq!(cpu.a_prime, 0x12);
        assert_eq!(cpu.f_prime, 0x34);
    }

    #[test]
    fn test_exx() {
        let mut cpu = Cpu::new();
        cpu.bc = 0x111111;
        cpu.de = 0x222222;
        cpu.hl = 0x333333;
        cpu.bc_prime = 0xAAAAAA;
        cpu.de_prime = 0xBBBBBB;
        cpu.hl_prime = 0xCCCCCC;

        cpu.exx();

        assert_eq!(cpu.bc, 0xAAAAAA);
        assert_eq!(cpu.de, 0xBBBBBB);
        assert_eq!(cpu.hl, 0xCCCCCC);
        assert_eq!(cpu.bc_prime, 0x111111);
        assert_eq!(cpu.de_prime, 0x222222);
        assert_eq!(cpu.hl_prime, 0x333333);
    }

    #[test]
    fn test_ex_de_hl() {
        let mut cpu = Cpu::new();
        cpu.de = 0x123456;
        cpu.hl = 0xABCDEF;

        cpu.ex_de_hl();

        assert_eq!(cpu.de, 0xABCDEF);
        assert_eq!(cpu.hl, 0x123456);
    }

    #[test]
    fn test_mask_addr_adl() {
        let cpu = Cpu::new();
        assert!(cpu.adl);
        assert_eq!(cpu.mask_addr(0x123456), 0x123456);
        assert_eq!(cpu.mask_addr(0xFF123456), 0x123456); // Upper bits masked
    }

    #[test]
    fn test_mask_addr_z80() {
        let mut cpu = Cpu::new();
        cpu.adl = false;
        cpu.mbase = 0xD0;
        assert_eq!(cpu.mask_addr(0x1234), 0xD01234);
        assert_eq!(cpu.mask_addr(0xABCDEF), 0xD0CDEF); // Upper bits replaced with MBASE
    }

    #[test]
    fn test_index_registers() {
        let mut cpu = Cpu::new();
        cpu.ix = 0xABCDEF;
        assert_eq!(cpu.ixh(), 0xCD);
        assert_eq!(cpu.ixl(), 0xEF);

        cpu.set_ixh(0x11);
        cpu.set_ixl(0x22);
        assert_eq!(cpu.ix, 0xAB1122);

        cpu.iy = 0x123456;
        assert_eq!(cpu.iyh(), 0x34);
        assert_eq!(cpu.iyl(), 0x56);
    }

    // ========== Instruction Execution Tests ==========

    #[test]
    fn test_nop() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        // NOP at address 0
        bus.poke_byte(0, 0x00); // NOP

        let cycles = cpu.step(&mut bus);
        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc, 1);
    }

    #[test]
    fn test_ld_reg_imm() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        // LD A,0x42 (opcode 0x3E, then immediate)
        bus.poke_byte(0, 0x3E);
        bus.poke_byte(1, 0x42);

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0x42);
        assert_eq!(cpu.pc, 2);
    }

    #[test]
    fn test_ld_reg_reg() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0x55;
        // LD B,A (opcode 0x47)
        bus.poke_byte(0, 0x47);

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 0x55);
    }

    #[test]
    fn test_ld_rp_imm() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        // LD BC,0x123456 (opcode 0x01, then 24-bit immediate in ADL mode)
        bus.poke_byte(0, 0x01);
        bus.poke_byte(1, 0x56); // Low
        bus.poke_byte(2, 0x34); // Mid
        bus.poke_byte(3, 0x12); // High

        cpu.step(&mut bus);
        assert_eq!(cpu.bc, 0x123456);
        assert_eq!(cpu.pc, 4);
    }

    #[test]
    fn test_add_a_reg() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0x10;
        cpu.set_b(0x05);
        // ADD A,B (opcode 0x80)
        bus.poke_byte(0, 0x80);

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0x15);
        assert!(!cpu.flag_z());
        assert!(!cpu.flag_c());
    }

    #[test]
    fn test_add_overflow() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0xFF;
        cpu.set_b(0x01);
        // ADD A,B
        bus.poke_byte(0, 0x80);

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0x00);
        assert!(cpu.flag_z());
        assert!(cpu.flag_c());
    }

    #[test]
    fn test_sub_a_reg() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0x15;
        cpu.set_b(0x05);
        // SUB B (opcode 0x90)
        bus.poke_byte(0, 0x90);

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0x10);
        assert!(cpu.flag_n()); // Subtract flag set
        assert!(!cpu.flag_z());
    }

    #[test]
    fn test_and_a_reg() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0xFF;
        cpu.set_b(0x0F);
        // AND B (opcode 0xA0)
        bus.poke_byte(0, 0xA0);

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0x0F);
        assert!(cpu.flag_h()); // AND sets H
    }

    #[test]
    fn test_or_a_reg() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0xF0;
        cpu.set_b(0x0F);
        // OR B (opcode 0xB0)
        bus.poke_byte(0, 0xB0);

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0xFF);
    }

    #[test]
    fn test_xor_a_reg() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0xFF;
        cpu.set_b(0xFF);
        // XOR B (opcode 0xA8)
        bus.poke_byte(0, 0xA8);

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0x00);
        assert!(cpu.flag_z());
    }

    #[test]
    fn test_cp_a_reg() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0x42;
        cpu.set_b(0x42);
        // CP B (opcode 0xB8)
        bus.poke_byte(0, 0xB8);

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0x42); // A unchanged
        assert!(cpu.flag_z());  // Z set because equal
    }

    #[test]
    fn test_inc_reg() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.set_b(0x0F);
        // INC B (opcode 0x04)
        bus.poke_byte(0, 0x04);

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 0x10);
        assert!(cpu.flag_h()); // Half-carry from 0x0F to 0x10
    }

    #[test]
    fn test_dec_reg() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.set_b(0x10);
        // DEC B (opcode 0x05)
        bus.poke_byte(0, 0x05);

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 0x0F);
        assert!(cpu.flag_n()); // DEC sets N
    }

    #[test]
    fn test_inc_rp() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.bc = 0x00FFFF;
        // INC BC (opcode 0x03)
        bus.poke_byte(0, 0x03);

        cpu.step(&mut bus);
        assert_eq!(cpu.bc, 0x010000);
    }

    #[test]
    fn test_jp() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        // JP 0x001234 (opcode 0xC3, then 24-bit address)
        bus.poke_byte(0, 0xC3);
        bus.poke_byte(1, 0x34);
        bus.poke_byte(2, 0x12);
        bus.poke_byte(3, 0x00);

        cpu.step(&mut bus);
        assert_eq!(cpu.pc, 0x001234);
    }

    #[test]
    fn test_jr() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        // JR +5 (opcode 0x18, offset 5)
        bus.poke_byte(0, 0x18);
        bus.poke_byte(1, 0x05);

        cpu.step(&mut bus);
        // PC was 2 after fetch, then +5 = 7
        assert_eq!(cpu.pc, 7);
    }

    #[test]
    fn test_jr_negative() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.pc = 0x100;
        // JR -3 (opcode 0x18, offset -3 = 0xFD)
        bus.poke_byte(0x100, 0x18);
        bus.poke_byte(0x101, 0xFD);

        cpu.step(&mut bus);
        // PC was 0x102 after fetch, then -3 = 0xFF
        assert_eq!(cpu.pc, 0xFF);
    }

    #[test]
    fn test_call_ret() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.sp = 0xD00100;
        // CALL 0x001234
        bus.poke_byte(0, 0xCD);
        bus.poke_byte(1, 0x34);
        bus.poke_byte(2, 0x12);
        bus.poke_byte(3, 0x00);

        cpu.step(&mut bus);
        assert_eq!(cpu.pc, 0x001234);
        // Return address (4) should be on stack
        // Stack grows down, so check at sp

        // RET at 0x001234
        bus.poke_byte(0x001234, 0xC9);
        cpu.step(&mut bus);
        assert_eq!(cpu.pc, 4);
    }

    #[test]
    fn test_push_pop() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.sp = 0xD00100;
        cpu.bc = 0x001234;

        // PUSH BC (opcode 0xC5)
        bus.poke_byte(0, 0xC5);
        cpu.step(&mut bus);

        // POP DE (opcode 0xD1)
        bus.poke_byte(1, 0xD1);
        cpu.step(&mut bus);

        assert_eq!(cpu.de & 0xFFFF, 0x1234);
    }

    #[test]
    fn test_halt() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        // HALT (opcode 0x76)
        bus.poke_byte(0, 0x76);

        cpu.step(&mut bus);
        assert!(cpu.halted);
        assert_eq!(cpu.pc, 1);

        // Subsequent steps should just consume cycles
        let cycles = cpu.step(&mut bus);
        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc, 1); // PC doesn't advance
    }

    #[test]
    fn test_di_ei() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.iff1 = true;
        cpu.iff2 = true;

        // DI (opcode 0xF3)
        bus.poke_byte(0, 0xF3);
        cpu.step(&mut bus);
        assert!(!cpu.iff1);
        assert!(!cpu.iff2);

        // EI (opcode 0xFB)
        bus.poke_byte(1, 0xFB);
        cpu.step(&mut bus);
        assert!(cpu.iff1);
        assert!(cpu.iff2);
    }

    #[test]
    fn test_djnz() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.set_b(3);
        // DJNZ -2 (loop back)
        bus.poke_byte(0, 0x10);
        bus.poke_byte(1, 0xFE); // -2, so back to 0

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 2);
        assert_eq!(cpu.pc, 0); // Jumped back

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 1);
        assert_eq!(cpu.pc, 0);

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 0);
        assert_eq!(cpu.pc, 2); // Fell through
    }

    #[test]
    fn test_jr_conditional() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.f = 0;
        cpu.set_flag_z(true);

        // JR Z,+5 (opcode 0x28)
        bus.poke_byte(0, 0x28);
        bus.poke_byte(1, 0x05);

        cpu.step(&mut bus);
        assert_eq!(cpu.pc, 7); // 2 + 5

        // Reset and test JR NZ when Z is set (should not jump)
        cpu.pc = 0;
        cpu.set_flag_z(true);
        // JR NZ,+5 (opcode 0x20)
        bus.poke_byte(0, 0x20);
        bus.poke_byte(1, 0x05);

        cpu.step(&mut bus);
        assert_eq!(cpu.pc, 2); // Did not jump
    }

    #[test]
    fn test_rst() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.sp = 0xD00100;
        // RST 0x38 (opcode 0xFF)
        bus.poke_byte(0, 0xFF);

        cpu.step(&mut bus);
        assert_eq!(cpu.pc, 0x38);
    }

    // ========== CB Prefix Tests ==========

    #[test]
    fn test_rlc() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.set_b(0x85); // 10000101
        // RLC B (CB 00)
        bus.poke_byte(0, 0xCB);
        bus.poke_byte(1, 0x00);

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 0x0B); // 00001011 (bit 7 rotated to bit 0)
        assert!(cpu.flag_c()); // Carry set from bit 7
    }

    #[test]
    fn test_rrc() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.set_b(0x85); // 10000101
        // RRC B (CB 08)
        bus.poke_byte(0, 0xCB);
        bus.poke_byte(1, 0x08);

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 0xC2); // 11000010 (bit 0 rotated to bit 7)
        assert!(cpu.flag_c()); // Carry set from bit 0
    }

    #[test]
    fn test_rl() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.set_b(0x85);
        cpu.set_flag_c(true);
        // RL B (CB 10)
        bus.poke_byte(0, 0xCB);
        bus.poke_byte(1, 0x10);

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 0x0B); // 00001011 (carry shifted in to bit 0)
        assert!(cpu.flag_c()); // New carry from old bit 7
    }

    #[test]
    fn test_rr() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.set_b(0x85);
        cpu.set_flag_c(true);
        // RR B (CB 18)
        bus.poke_byte(0, 0xCB);
        bus.poke_byte(1, 0x18);

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 0xC2); // 11000010 (carry shifted in to bit 7)
        assert!(cpu.flag_c()); // New carry from old bit 0
    }

    #[test]
    fn test_sla() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.set_b(0x85); // 10000101
        // SLA B (CB 20)
        bus.poke_byte(0, 0xCB);
        bus.poke_byte(1, 0x20);

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 0x0A); // 00001010 (shifted left, bit 0 = 0)
        assert!(cpu.flag_c()); // Carry from bit 7
    }

    #[test]
    fn test_sra() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.set_b(0x85); // 10000101
        // SRA B (CB 28)
        bus.poke_byte(0, 0xCB);
        bus.poke_byte(1, 0x28);

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 0xC2); // 11000010 (shifted right, sign preserved)
        assert!(cpu.flag_c()); // Carry from bit 0
    }

    #[test]
    fn test_srl() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.set_b(0x85); // 10000101
        // SRL B (CB 38)
        bus.poke_byte(0, 0xCB);
        bus.poke_byte(1, 0x38);

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 0x42); // 01000010 (shifted right, bit 7 = 0)
        assert!(cpu.flag_c()); // Carry from bit 0
    }

    #[test]
    fn test_bit() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.set_b(0x80); // Bit 7 set
        // BIT 7,B (CB 78)
        bus.poke_byte(0, 0xCB);
        bus.poke_byte(1, 0x78);

        cpu.step(&mut bus);
        assert!(!cpu.flag_z()); // Z clear because bit 7 is set

        // BIT 0,B (CB 40)
        cpu.pc = 0;
        bus.poke_byte(0, 0xCB);
        bus.poke_byte(1, 0x40);

        cpu.step(&mut bus);
        assert!(cpu.flag_z()); // Z set because bit 0 is clear
    }

    #[test]
    fn test_res() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.set_b(0xFF);
        // RES 7,B (CB B8)
        bus.poke_byte(0, 0xCB);
        bus.poke_byte(1, 0xB8);

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 0x7F); // Bit 7 cleared
    }

    #[test]
    fn test_set() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.set_b(0x00);
        // SET 7,B (CB F8)
        bus.poke_byte(0, 0xCB);
        bus.poke_byte(1, 0xF8);

        cpu.step(&mut bus);
        assert_eq!(cpu.b(), 0x80); // Bit 7 set
    }

    #[test]
    fn test_cb_memory() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.hl = 0xD00100;
        bus.poke_byte(0xD00100, 0x00);

        // SET 7,(HL) (CB FE)
        bus.poke_byte(0, 0xCB);
        bus.poke_byte(1, 0xFE);

        cpu.step(&mut bus);
        assert_eq!(bus.peek_byte(0xD00100), 0x80);
    }

    // ========== ED Prefix Tests ==========

    #[test]
    fn test_neg() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0x01;
        // NEG (ED 44)
        bus.poke_byte(0, 0xED);
        bus.poke_byte(1, 0x44);

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0xFF);
        assert!(cpu.flag_n());
        assert!(cpu.flag_c());
    }

    #[test]
    fn test_neg_zero() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0x00;
        // NEG (ED 44)
        bus.poke_byte(0, 0xED);
        bus.poke_byte(1, 0x44);

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0x00);
        assert!(cpu.flag_z());
        assert!(!cpu.flag_c());
    }

    #[test]
    fn test_sbc_hl() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.hl = 0x001000;
        cpu.bc = 0x000100;
        cpu.set_flag_c(false);

        // SBC HL,BC (ED 42)
        bus.poke_byte(0, 0xED);
        bus.poke_byte(1, 0x42);

        cpu.step(&mut bus);
        assert_eq!(cpu.hl, 0x000F00);
        assert!(cpu.flag_n());
    }

    #[test]
    fn test_adc_hl() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.hl = 0x001000;
        cpu.bc = 0x000100;
        cpu.set_flag_c(true);

        // ADC HL,BC (ED 4A)
        bus.poke_byte(0, 0xED);
        bus.poke_byte(1, 0x4A);

        cpu.step(&mut bus);
        assert_eq!(cpu.hl, 0x001101); // 0x1000 + 0x100 + 1
        assert!(!cpu.flag_n());
    }

    #[test]
    fn test_ld_rp_nn_indirect() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.bc = 0x123456;

        // LD (0xD00200),BC (ED 43 00 02 D0)
        bus.poke_byte(0, 0xED);
        bus.poke_byte(1, 0x43);
        bus.poke_byte(2, 0x00);
        bus.poke_byte(3, 0x02);
        bus.poke_byte(4, 0xD0);

        cpu.step(&mut bus);
        assert_eq!(bus.peek_byte(0xD00200), 0x56);
        assert_eq!(bus.peek_byte(0xD00201), 0x34);
        assert_eq!(bus.peek_byte(0xD00202), 0x12);
    }

    #[test]
    fn test_ld_a_i() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.i = 0x42;
        cpu.iff2 = true;

        // LD A,I (ED 57)
        bus.poke_byte(0, 0xED);
        bus.poke_byte(1, 0x57);

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0x42);
        assert!(cpu.flag_pv()); // PV reflects IFF2
    }

    #[test]
    fn test_im_modes() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        // IM 1 (ED 56)
        bus.poke_byte(0, 0xED);
        bus.poke_byte(1, 0x56);
        cpu.step(&mut bus);
        assert_eq!(cpu.im, InterruptMode::Mode1);

        // IM 2 (ED 5E)
        bus.poke_byte(2, 0xED);
        bus.poke_byte(3, 0x5E);
        cpu.step(&mut bus);
        assert_eq!(cpu.im, InterruptMode::Mode2);
    }

    #[test]
    fn test_ldi() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.hl = 0xD00100;
        cpu.de = 0xD00200;
        cpu.bc = 0x000003;
        bus.poke_byte(0xD00100, 0x42);

        // LDI (ED A0)
        bus.poke_byte(0, 0xED);
        bus.poke_byte(1, 0xA0);

        cpu.step(&mut bus);
        assert_eq!(bus.peek_byte(0xD00200), 0x42);
        assert_eq!(cpu.hl, 0xD00101);
        assert_eq!(cpu.de, 0xD00201);
        assert_eq!(cpu.bc, 0x000002);
        assert!(cpu.flag_pv()); // BC != 0
    }

    #[test]
    fn test_ldd() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.hl = 0xD00105;
        cpu.de = 0xD00205;
        cpu.bc = 0x000003;
        bus.poke_byte(0xD00105, 0x55);

        // LDD (ED A8)
        bus.poke_byte(0, 0xED);
        bus.poke_byte(1, 0xA8);

        cpu.step(&mut bus);
        assert_eq!(bus.peek_byte(0xD00205), 0x55);
        assert_eq!(cpu.hl, 0xD00104);
        assert_eq!(cpu.de, 0xD00204);
        assert_eq!(cpu.bc, 0x000002);
    }

    #[test]
    fn test_ldir() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.hl = 0xD00100;
        cpu.de = 0xD00200;
        cpu.bc = 0x000003;
        bus.poke_byte(0xD00100, 0x11);
        bus.poke_byte(0xD00101, 0x22);
        bus.poke_byte(0xD00102, 0x33);

        // LDIR (ED B0)
        bus.poke_byte(0, 0xED);
        bus.poke_byte(1, 0xB0);

        // First iteration
        cpu.step(&mut bus);
        assert_eq!(cpu.bc, 0x000002);
        assert_eq!(cpu.pc, 0); // Loops back

        // Second iteration
        cpu.step(&mut bus);
        assert_eq!(cpu.bc, 0x000001);
        assert_eq!(cpu.pc, 0);

        // Third iteration
        cpu.step(&mut bus);
        assert_eq!(cpu.bc, 0x000000);
        assert_eq!(cpu.pc, 2); // Done, advances

        // Check memory was copied
        assert_eq!(bus.peek_byte(0xD00200), 0x11);
        assert_eq!(bus.peek_byte(0xD00201), 0x22);
        assert_eq!(bus.peek_byte(0xD00202), 0x33);
    }

    #[test]
    fn test_cpi() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0x42;
        cpu.hl = 0xD00100;
        cpu.bc = 0x000003;
        bus.poke_byte(0xD00100, 0x42);

        // CPI (ED A1)
        bus.poke_byte(0, 0xED);
        bus.poke_byte(1, 0xA1);

        cpu.step(&mut bus);
        assert!(cpu.flag_z()); // A == (HL)
        assert_eq!(cpu.hl, 0xD00101);
        assert_eq!(cpu.bc, 0x000002);
    }

    #[test]
    fn test_rrd() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0x12;
        cpu.hl = 0xD00100;
        bus.poke_byte(0xD00100, 0x34);

        // RRD (ED 67)
        bus.poke_byte(0, 0xED);
        bus.poke_byte(1, 0x67);

        cpu.step(&mut bus);
        // A = (A & 0xF0) | (mem & 0x0F) = 0x10 | 0x04 = 0x14
        // mem = (A << 4) | (mem >> 4) = 0x20 | 0x03 = 0x23
        assert_eq!(cpu.a, 0x14);
        assert_eq!(bus.peek_byte(0xD00100), 0x23);
    }

    #[test]
    fn test_rld() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0x12;
        cpu.hl = 0xD00100;
        bus.poke_byte(0xD00100, 0x34);

        // RLD (ED 6F)
        bus.poke_byte(0, 0xED);
        bus.poke_byte(1, 0x6F);

        cpu.step(&mut bus);
        // A = (A & 0xF0) | (mem >> 4) = 0x10 | 0x03 = 0x13
        // mem = (mem << 4) | (A & 0x0F) = 0x40 | 0x02 = 0x42
        assert_eq!(cpu.a, 0x13);
        assert_eq!(bus.peek_byte(0xD00100), 0x42);
    }

    // ========== DD/FD Prefix Tests ==========

    #[test]
    fn test_ld_ix_imm() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        // LD IX,0x123456 (DD 21 56 34 12)
        bus.poke_byte(0, 0xDD);
        bus.poke_byte(1, 0x21);
        bus.poke_byte(2, 0x56);
        bus.poke_byte(3, 0x34);
        bus.poke_byte(4, 0x12);

        cpu.step(&mut bus);
        assert_eq!(cpu.ix, 0x123456);
    }

    #[test]
    fn test_ld_iy_imm() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        // LD IY,0xABCDEF (FD 21 EF CD AB)
        bus.poke_byte(0, 0xFD);
        bus.poke_byte(1, 0x21);
        bus.poke_byte(2, 0xEF);
        bus.poke_byte(3, 0xCD);
        bus.poke_byte(4, 0xAB);

        cpu.step(&mut bus);
        assert_eq!(cpu.iy, 0xABCDEF);
    }

    #[test]
    fn test_ld_indexed_mem() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.ix = 0xD00100;
        cpu.a = 0x42;

        // LD (IX+5),A (DD 77 05)
        bus.poke_byte(0, 0xDD);
        bus.poke_byte(1, 0x77);
        bus.poke_byte(2, 0x05);

        cpu.step(&mut bus);
        assert_eq!(bus.peek_byte(0xD00105), 0x42);
    }

    #[test]
    fn test_ld_from_indexed_mem() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.ix = 0xD00100;
        bus.poke_byte(0xD00105, 0x55);

        // LD A,(IX+5) (DD 7E 05)
        bus.poke_byte(0, 0xDD);
        bus.poke_byte(1, 0x7E);
        bus.poke_byte(2, 0x05);

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0x55);
    }

    #[test]
    fn test_indexed_negative_offset() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.iy = 0xD00110;
        bus.poke_byte(0xD00100, 0x77);

        // LD A,(IY-16) (FD 7E F0) where F0 = -16
        bus.poke_byte(0, 0xFD);
        bus.poke_byte(1, 0x7E);
        bus.poke_byte(2, 0xF0); // -16 as signed byte

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0x77);
    }

    #[test]
    fn test_add_ix_bc() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.ix = 0x001000;
        cpu.bc = 0x000234;

        // ADD IX,BC (DD 09)
        bus.poke_byte(0, 0xDD);
        bus.poke_byte(1, 0x09);

        cpu.step(&mut bus);
        assert_eq!(cpu.ix, 0x001234);
    }

    #[test]
    fn test_inc_ix() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.ix = 0x00FFFF;

        // INC IX (DD 23)
        bus.poke_byte(0, 0xDD);
        bus.poke_byte(1, 0x23);

        cpu.step(&mut bus);
        assert_eq!(cpu.ix, 0x010000);
    }

    #[test]
    fn test_push_pop_ix() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.sp = 0xD00200;
        cpu.ix = 0x123456;

        // PUSH IX (DD E5)
        bus.poke_byte(0, 0xDD);
        bus.poke_byte(1, 0xE5);
        cpu.step(&mut bus);
        assert_eq!(cpu.pc, 2);

        // Change IX
        cpu.ix = 0;

        // POP IX (DD E1) - at position 2 (where PC is now)
        bus.poke_byte(2, 0xDD);
        bus.poke_byte(3, 0xE1);
        cpu.step(&mut bus);

        assert_eq!(cpu.ix, 0x123456);
    }

    #[test]
    fn test_jp_ix() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.ix = 0x001234;

        // JP (IX) (DD E9)
        bus.poke_byte(0, 0xDD);
        bus.poke_byte(1, 0xE9);

        cpu.step(&mut bus);
        assert_eq!(cpu.pc, 0x001234);
    }

    #[test]
    fn test_ld_ixh() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        // LD IXH,0x42 (DD 26 42)
        bus.poke_byte(0, 0xDD);
        bus.poke_byte(1, 0x26);
        bus.poke_byte(2, 0x42);

        cpu.step(&mut bus);
        assert_eq!(cpu.ixh(), 0x42);
    }

    #[test]
    fn test_indexed_cb_bit() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.ix = 0xD00100;
        bus.poke_byte(0xD00105, 0x80); // Bit 7 set

        // BIT 7,(IX+5) (DD CB 05 7E)
        bus.poke_byte(0, 0xDD);
        bus.poke_byte(1, 0xCB);
        bus.poke_byte(2, 0x05);
        bus.poke_byte(3, 0x7E);

        cpu.step(&mut bus);
        assert!(!cpu.flag_z()); // Bit 7 is set
    }

    #[test]
    fn test_indexed_cb_set() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.ix = 0xD00100;
        bus.poke_byte(0xD00105, 0x00);

        // SET 7,(IX+5) (DD CB 05 FE)
        bus.poke_byte(0, 0xDD);
        bus.poke_byte(1, 0xCB);
        bus.poke_byte(2, 0x05);
        bus.poke_byte(3, 0xFE);

        cpu.step(&mut bus);
        assert_eq!(bus.peek_byte(0xD00105), 0x80);
    }

    #[test]
    fn test_indexed_cb_res() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.iy = 0xD00100;
        bus.poke_byte(0xD00105, 0xFF);

        // RES 0,(IY+5) (FD CB 05 86)
        bus.poke_byte(0, 0xFD);
        bus.poke_byte(1, 0xCB);
        bus.poke_byte(2, 0x05);
        bus.poke_byte(3, 0x86);

        cpu.step(&mut bus);
        assert_eq!(bus.peek_byte(0xD00105), 0xFE);
    }

    #[test]
    fn test_inc_indexed_mem() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.ix = 0xD00100;
        bus.poke_byte(0xD00105, 0x41);

        // INC (IX+5) (DD 34 05)
        bus.poke_byte(0, 0xDD);
        bus.poke_byte(1, 0x34);
        bus.poke_byte(2, 0x05);

        cpu.step(&mut bus);
        assert_eq!(bus.peek_byte(0xD00105), 0x42);
    }

    #[test]
    fn test_add_a_indexed_mem() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        cpu.a = 0x10;
        cpu.ix = 0xD00100;
        bus.poke_byte(0xD00105, 0x05);

        // ADD A,(IX+5) (DD 86 05)
        bus.poke_byte(0, 0xDD);
        bus.poke_byte(1, 0x86);
        bus.poke_byte(2, 0x05);

        cpu.step(&mut bus);
        assert_eq!(cpu.a, 0x15);
    }

    // ========== DAA Tests ==========

    #[test]
    fn test_daa_after_add() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        // Add 15 + 27 = 42 in BCD
        cpu.a = 0x15;
        cpu.set_b(0x27);
        cpu.set_flag_n(false);

        // ADD A,B (0x80)
        bus.poke_byte(0, 0x80);
        cpu.step(&mut bus);
        // A = 0x15 + 0x27 = 0x3C (binary)

        // DAA (0x27)
        bus.poke_byte(1, 0x27);
        cpu.step(&mut bus);
        // After DAA, should be 0x42 (BCD for 15+27=42)
        assert_eq!(cpu.a, 0x42);
    }

    #[test]
    fn test_daa_with_carry() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        // Add 99 + 1 = 100 in BCD (should set carry)
        cpu.a = 0x99;
        cpu.set_b(0x01);
        cpu.set_flag_n(false);

        // ADD A,B
        bus.poke_byte(0, 0x80);
        cpu.step(&mut bus);
        // A = 0x99 + 0x01 = 0x9A

        // DAA
        bus.poke_byte(1, 0x27);
        cpu.step(&mut bus);
        // After DAA: 0x9A needs lower nibble correction (A > 9), add 0x06 = 0xA0
        // Then upper nibble needs correction (A > 9), add 0x60 = 0x00 with carry
        assert_eq!(cpu.a, 0x00);
        assert!(cpu.flag_c()); // Carry set for BCD overflow
    }

    #[test]
    fn test_daa_after_sub() {
        let mut cpu = Cpu::new();
        let mut bus = Bus::new();

        // Sub 42 - 15 = 27 in BCD
        cpu.a = 0x42;
        cpu.set_b(0x15);

        // SUB B (0x90)
        bus.poke_byte(0, 0x90);
        cpu.step(&mut bus);
        // A = 0x42 - 0x15 = 0x2D, N flag set, H flag set (borrow from nibble)

        // DAA
        bus.poke_byte(1, 0x27);
        cpu.step(&mut bus);
        // After DAA with N set and H set, should subtract 0x06
        assert_eq!(cpu.a, 0x27);
    }
}
