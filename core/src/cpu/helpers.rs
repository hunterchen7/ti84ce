//! eZ80 CPU helper functions
//!
//! This module contains helper functions for the eZ80 CPU implementation including:
//! - Register accessors (b, c, d, e, h, l, ixh, ixl, iyh, iyl)
//! - Flag helpers (flag_c, set_flag_c, etc.)
//! - Address masking (mask_addr, wrap_pc)
//! - Instruction fetch (fetch_byte, fetch_word, fetch_addr)
//! - Stack operations (push_byte, pop_byte, push_word, pop_word, push_addr, pop_addr)
//! - ALU operations (alu_add, alu_sub, alu_and, alu_or, alu_xor, alu_inc, alu_dec)
//! - Register access by index (get_reg8, set_reg8, get_rp, set_rp)
//! - Register exchange (ex_af, exx, ex_de_hl)
//!
//! # References
//! - eZ80 CPU User Manual (Zilog UM0077)
//! - CEmu (https://github.com/CE-Programming/CEmu)

use super::flags;
use super::Cpu;
use crate::bus::Bus;

impl Cpu {
    // ========== Register Accessors ==========
    // Note: For 24-bit register pairs (BC, DE, HL), the 8-bit registers access
    // bits 15-8 (B/D/H) and bits 7-0 (C/E/L). Bits 23-16 are not directly
    // accessible via 8-bit register operations in Z80/eZ80 architecture.

    /// Get B register (bits 15-8 of BC)
    #[inline]
    pub fn b(&self) -> u8 {
        (self.bc >> 8) as u8
    }

    /// Set B register (bits 15-8 of BC)
    #[inline]
    pub fn set_b(&mut self, val: u8) {
        self.bc = (self.bc & 0xFF00FF) | ((val as u32) << 8);
    }

    /// Get C register (bits 7-0 of BC)
    #[inline]
    pub fn c(&self) -> u8 {
        self.bc as u8
    }

    /// Set C register (bits 7-0 of BC)
    #[inline]
    pub fn set_c(&mut self, val: u8) {
        self.bc = (self.bc & 0xFFFF00) | (val as u32);
    }

    /// Get D register (bits 15-8 of DE)
    #[inline]
    pub fn d(&self) -> u8 {
        (self.de >> 8) as u8
    }

    /// Set D register (bits 15-8 of DE)
    #[inline]
    pub fn set_d(&mut self, val: u8) {
        self.de = (self.de & 0xFF00FF) | ((val as u32) << 8);
    }

    /// Get E register (bits 7-0 of DE)
    #[inline]
    pub fn e(&self) -> u8 {
        self.de as u8
    }

    /// Set E register (bits 7-0 of DE)
    #[inline]
    pub fn set_e(&mut self, val: u8) {
        self.de = (self.de & 0xFFFF00) | (val as u32);
    }

    /// Get H register (bits 15-8 of HL)
    #[inline]
    pub fn h(&self) -> u8 {
        (self.hl >> 8) as u8
    }

    /// Set H register (bits 15-8 of HL)
    #[inline]
    pub fn set_h(&mut self, val: u8) {
        self.hl = (self.hl & 0xFF00FF) | ((val as u32) << 8);
    }

    /// Get L register (bits 7-0 of HL)
    #[inline]
    pub fn l(&self) -> u8 {
        self.hl as u8
    }

    /// Set L register (bits 7-0 of HL)
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
    /// Note: EX DE,HL is a simple register swap - no L-mode masking
    /// CEmu: EX(DE, cpu_read_index()) uses simple swap macro
    pub fn ex_de_hl(&mut self) {
        std::mem::swap(&mut self.de, &mut self.hl);
    }

    // ========== Address Masking ==========

    /// Mask address based on L mode (applies MBASE in 16-bit data mode)
    /// Use for memory operand addresses
    #[inline]
    pub fn mask_addr(&self, addr: u32) -> u32 {
        if self.l {
            addr & 0xFFFFFF // 24-bit in ADL mode
        } else {
            ((self.mbase as u32) << 16) | (addr & 0xFFFF) // 16-bit with MBASE
        }
    }

    /// Mask address based on ADL mode (applies MBASE in Z80 instruction mode)
    /// Use for instruction fetch addresses (PC)
    #[inline]
    pub fn mask_addr_instr(&self, addr: u32) -> u32 {
        if self.adl {
            addr & 0xFFFFFF // 24-bit in ADL mode
        } else {
            ((self.mbase as u32) << 16) | (addr & 0xFFFF) // 16-bit with MBASE
        }
    }

    /// Wrap PC/SP to stay within address width (no MBASE added)
    /// Use for PC and SP modifications
    #[inline]
    pub fn wrap_pc(&self, addr: u32) -> u32 {
        if self.adl {
            addr & 0xFFFFFF // 24-bit in ADL mode
        } else {
            addr & 0xFFFF // 16-bit in Z80 mode (no MBASE)
        }
    }

    /// Wrap data register (HL, DE) to stay within address width based on L mode
    /// Use for data register modifications in block instructions (LDI, LDIR, etc.)
    /// CEmu: cpu_mask_mode(r->HL + delta, cpu.L)
    #[inline]
    pub fn wrap_data(&self, addr: u32) -> u32 {
        if self.l {
            addr & 0xFFFFFF // 24-bit in L mode
        } else {
            addr & 0xFFFF // 16-bit in Z80 data mode (no MBASE)
        }
    }

    /// Get effective address width based on ADL mode
    #[inline]
    pub fn addr_width(&self) -> u8 {
        if self.adl {
            3
        } else {
            2
        }
    }

    // ========== Instruction Fetch ==========

    /// Fetch byte at PC and increment PC
    #[inline]
    pub fn fetch_byte(&mut self, bus: &mut Bus) -> u8 {
        // Apply MBASE for actual memory access in Z80 instruction mode
        let effective_pc = self.mask_addr_instr(self.pc);
        // Use fetch_byte which tracks instruction bytes for flash unlock sequence detection
        let byte = bus.fetch_byte(effective_pc, self.pc);
        // PC stays as 16-bit in Z80 mode, 24-bit in ADL mode (no MBASE added)
        self.pc = if self.adl {
            self.pc.wrapping_add(1) & 0xFFFFFF
        } else {
            self.pc.wrapping_add(1) & 0xFFFF
        };
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

    /// Fetch address at PC - 24-bit if IL mode, 16-bit otherwise
    /// Uses the IL (instruction/index) mode flag set by suffix opcodes.
    /// Returns raw value without MBASE (for PC/SP assignments)
    #[inline]
    pub fn fetch_addr(&mut self, bus: &mut Bus) -> u32 {
        if self.il {
            let b0 = self.fetch_byte(bus) as u32;
            let b1 = self.fetch_byte(bus) as u32;
            let b2 = self.fetch_byte(bus) as u32;
            b0 | (b1 << 8) | (b2 << 16)
        } else {
            // Z80 mode: just 16-bit, no MBASE (caller applies MBASE if needed for memory access)
            self.fetch_word(bus) as u32
        }
    }

    // ========== Stack Operations ==========
    // NOTE: CEmu uses cpu.L mode for stack operations, selecting between
    // SPS (16-bit) and SPL (24-bit) stack pointers. We use a single SP
    // but apply L mode masking to match CEmu behavior.

    /// Push a byte onto the stack
    /// CEmu: cpu_push_byte_mode(value, cpu.L) uses L mode for SP masking
    #[inline]
    pub fn push_byte(&mut self, bus: &mut Bus, val: u8) {
        self.sp = self.wrap_data(self.sp.wrapping_sub(1));
        bus.write_byte(self.mask_addr(self.sp), val);
    }

    /// Pop a byte from the stack
    /// CEmu: cpu_pop_byte_mode(cpu.L) uses L mode for SP masking
    #[inline]
    pub fn pop_byte(&mut self, bus: &mut Bus) -> u8 {
        let val = bus.read_byte(self.mask_addr(self.sp));
        self.sp = self.wrap_data(self.sp.wrapping_add(1));
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

    /// Push address (24-bit if L mode, 16-bit otherwise)
    /// Uses L mode (data addressing) which can be overridden by suffix opcodes
    #[inline]
    pub fn push_addr(&mut self, bus: &mut Bus, val: u32) {
        if self.l {
            self.push_byte(bus, (val >> 16) as u8);
            self.push_byte(bus, (val >> 8) as u8);
            self.push_byte(bus, val as u8);
        } else {
            self.push_word(bus, val as u16);
        }
    }

    /// Pop address (24-bit if L mode, 16-bit otherwise)
    /// Uses L mode (data addressing) which can be overridden by suffix opcodes
    #[inline]
    pub fn pop_addr(&mut self, bus: &mut Bus) -> u32 {
        if self.l {
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
    /// CEmu: preserves F3/F5 from previous F register (cpuflag_undef behavior)
    pub(super) fn alu_add(&mut self, val: u8, carry: bool) -> u8 {
        let c = if carry && self.flag_c() { 1u16 } else { 0 };
        let result = self.a as u16 + val as u16 + c;

        // Half-carry: carry from bit 3 to bit 4
        let half = ((self.a & 0x0F) + (val & 0x0F) + c as u8) > 0x0F;

        // Overflow: sign of result differs from expected
        let overflow = ((self.a ^ val) & 0x80 == 0) && ((self.a ^ result as u8) & 0x80 != 0);

        // Preserve F3/F5 from previous F (CEmu: cpuflag_undef(r->F))
        let old_f3f5 = self.f & (flags::F5 | flags::F3);
        self.f = 0;
        self.set_sz_flags(result as u8);
        self.f |= old_f3f5; // Restore F3/F5
        self.set_flag_c(result > 0xFF);
        self.set_flag_h(half);
        self.set_flag_pv(overflow);
        self.set_flag_n(false);

        result as u8
    }

    /// Subtract with flags (used by SUB, SBC, CP)
    /// CEmu: preserves F3/F5 from previous F register for ALL sub operations
    pub(super) fn alu_sub(&mut self, val: u8, carry: bool, _store: bool) -> u8 {
        let c = if carry && self.flag_c() { 1u16 } else { 0 };
        let result = (self.a as u16).wrapping_sub(val as u16).wrapping_sub(c);

        // Half-carry (borrow from bit 4)
        let half = (self.a & 0x0F) < (val & 0x0F) + c as u8;

        // Overflow
        let overflow = ((self.a ^ val) & 0x80 != 0) && ((self.a ^ result as u8) & 0x80 != 0);

        // Preserve F3/F5 from previous F (CEmu: cpuflag_undef(r->F))
        let old_f3f5 = self.f & (flags::F5 | flags::F3);
        self.f = 0;
        self.set_sz_flags(result as u8);
        self.f |= old_f3f5; // Restore F3/F5 for all sub operations
        self.set_flag_c(result > 0xFF);
        self.set_flag_h(half);
        self.set_flag_pv(overflow);
        self.set_flag_n(true);

        result as u8
    }

    /// AND operation
    /// CEmu preserves F3/F5 from existing F register
    pub(super) fn alu_and(&mut self, val: u8) {
        let old_f3f5 = self.f & (flags::F5 | flags::F3);
        self.a &= val;
        self.f = 0;
        if self.a == 0 {
            self.f |= flags::Z;
        }
        if self.a & 0x80 != 0 {
            self.f |= flags::S;
        }
        self.f |= old_f3f5; // Preserve F3/F5 from before
        self.set_flag_h(true);
        self.set_flag_pv(Self::parity(self.a));
    }

    /// OR operation
    /// CEmu preserves F3/F5 from existing F register
    pub(super) fn alu_or(&mut self, val: u8) {
        let old_f3f5 = self.f & (flags::F5 | flags::F3);
        self.a |= val;
        self.f = 0;
        if self.a == 0 {
            self.f |= flags::Z;
        }
        if self.a & 0x80 != 0 {
            self.f |= flags::S;
        }
        self.f |= old_f3f5; // Preserve F3/F5 from before
        self.set_flag_pv(Self::parity(self.a));
    }

    /// XOR operation
    /// CEmu preserves F3/F5 from existing F register
    pub(super) fn alu_xor(&mut self, val: u8) {
        let old_f3f5 = self.f & (flags::F5 | flags::F3);
        self.a ^= val;
        self.f = 0;
        if self.a == 0 {
            self.f |= flags::Z;
        }
        if self.a & 0x80 != 0 {
            self.f |= flags::S;
        }
        self.f |= old_f3f5; // Preserve F3/F5 from before
        self.set_flag_pv(Self::parity(self.a));
    }

    /// Increment 8-bit value with flags
    /// CEmu preserves F3/F5 from existing F register
    pub(super) fn alu_inc(&mut self, val: u8) -> u8 {
        let result = val.wrapping_add(1);
        let half = (val & 0x0F) == 0x0F;
        let overflow = val == 0x7F;

        // Preserve carry and F3/F5, set other flags
        let old_c = self.flag_c();
        let old_f3f5 = self.f & (flags::F5 | flags::F3);
        self.f = 0;
        // Set S, Z but not F3/F5
        if result == 0 {
            self.f |= flags::Z;
        }
        if result & 0x80 != 0 {
            self.f |= flags::S;
        }
        self.f |= old_f3f5; // Preserve F3/F5
        self.set_flag_h(half);
        self.set_flag_pv(overflow);
        self.set_flag_c(old_c);

        result
    }

    /// Decrement 8-bit value with flags
    /// CEmu preserves F3/F5 from existing F register
    pub(super) fn alu_dec(&mut self, val: u8) -> u8 {
        let result = val.wrapping_sub(1);
        let half = (val & 0x0F) == 0x00;
        let overflow = val == 0x80;

        // Preserve carry and F3/F5, set other flags
        let old_c = self.flag_c();
        let old_f3f5 = self.f & (flags::F5 | flags::F3);
        self.f = 0;
        // Set S, Z but not F3/F5
        if result == 0 {
            self.f |= flags::Z;
        }
        if result & 0x80 != 0 {
            self.f |= flags::S;
        }
        self.f |= old_f3f5; // Preserve F3/F5
        self.set_flag_h(half);
        self.set_flag_pv(overflow);
        self.set_flag_n(true);
        self.set_flag_c(old_c);

        result
    }

    // ========== Register Access by Index ==========

    /// Get 8-bit register by index (0=B, 1=C, 2=D, 3=E, 4=H, 5=L, 6=(HL), 7=A)
    /// Note: Uses read_byte for (HL) to properly account for cycles, matching CEmu behavior
    pub(super) fn get_reg8(&mut self, idx: u8, bus: &mut Bus) -> u8 {
        match idx {
            0 => self.b(),
            1 => self.c(),
            2 => self.d(),
            3 => self.e(),
            4 => self.h(),
            5 => self.l(),
            6 => bus.read_byte(self.mask_addr(self.hl)), // (HL) - apply MBASE in Z80 mode
            7 => self.a,
            _ => 0,
        }
    }

    /// Set 8-bit register by index
    pub(super) fn set_reg8(&mut self, idx: u8, val: u8, bus: &mut Bus) {
        match idx {
            0 => self.set_b(val),
            1 => self.set_c(val),
            2 => self.set_d(val),
            3 => self.set_e(val),
            4 => self.set_h(val),
            5 => self.set_l(val),
            6 => bus.write_byte(self.mask_addr(self.hl), val), // (HL) - apply MBASE in Z80 mode
            7 => self.a = val,
            _ => {}
        }
    }

    /// Get 16/24-bit register pair by index (0=BC, 1=DE, 2=HL, 3=SP)
    pub(super) fn get_rp(&self, idx: u8) -> u32 {
        let mask = if self.l { 0xFFFFFF } else { 0xFFFF };
        match idx {
            0 => self.bc & mask,
            1 => self.de & mask,
            2 => self.hl & mask,
            3 => self.sp & mask,
            _ => 0,
        }
    }

    /// Set 16/24-bit register pair by index
    /// Note: Uses wrap_pc-style masking (no MBASE) since register values
    /// should not have MBASE embedded - only memory addresses need MBASE
    pub(super) fn set_rp(&mut self, idx: u8, val: u32) {
        let masked = if self.l { val & 0xFFFFFF } else { val & 0xFFFF };
        match idx {
            0 => self.bc = masked,
            1 => self.de = masked,
            2 => self.hl = masked,
            3 => self.sp = masked,
            _ => {}
        }
    }

    /// Get register pair for push/pop (0=BC, 1=DE, 2=HL, 3=AF)
    pub(super) fn get_rp2(&self, idx: u8) -> u16 {
        match idx {
            0 => self.bc as u16,
            1 => self.de as u16,
            2 => self.hl as u16,
            3 => ((self.a as u16) << 8) | (self.f as u16),
            _ => 0,
        }
    }

    /// Set register pair for push/pop
    pub(super) fn set_rp2(&mut self, idx: u8, val: u16) {
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
    pub(super) fn check_cc(&self, cc: u8) -> bool {
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
}
