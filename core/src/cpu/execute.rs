//! eZ80 CPU instruction execution
//!
//! This module contains all instruction execution functions for the eZ80 CPU including:
//! - execute_x0: Base instruction decoding (x=0 category)
//! - execute_alu: ALU operations (ADD, SUB, AND, OR, XOR, CP, INC, DEC)
//! - execute_x3: Control flow and I/O instructions (x=3 category)
//! - execute_cb: CB prefix instructions (rotate, shift, bit operations)
//! - execute_rot: Rotate/shift operation implementation
//! - execute_ed: ED prefix instructions (extended operations)
//! - execute_ed_x1: ED prefix x=1 category (block operations, special loads)
//! - execute_bli: Block instruction execution (LDI, LDIR, LDD, LDDR, CPI, CPIR, etc.)
//! - execute_index: DD/FD prefix instructions (IX/IY indexed operations)
//! - execute_index_x0, execute_index_x3, execute_index_cb: Index instruction categories
//!
//! # References
//! - eZ80 CPU User Manual (Zilog UM0077)
//! - CEmu (https://github.com/CE-Programming/CEmu)

use super::flags;
use super::Cpu;
use super::InterruptMode;
use crate::bus::Bus;

impl Cpu {
    pub fn execute_x0(&mut self, bus: &mut Bus, y: u8, z: u8, p: u8, q: u8) -> u32 {
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
                            self.pc = self.wrap_pc((self.pc as i32 + d as i32) as u32);
                            13
                        } else {
                            8
                        }
                    }
                    3 => {
                        // JR d
                        let d = self.fetch_byte(bus) as i8;
                        self.pc = self.wrap_pc((self.pc as i32 + d as i32) as u32);
                        12
                    }
                    4..=7 => {
                        // JR cc,d
                        let d = self.fetch_byte(bus) as i8;
                        if self.check_cc(y - 4) {
                            self.pc = self.wrap_pc((self.pc as i32 + d as i32) as u32);
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
                    // Note: Cycle count doesn't differ by mode - the timing difference
                    // comes from memory wait states when fetching the 3-byte (ADL) vs
                    // 2-byte (Z80) immediate, which is handled by the Bus.
                    let nn = self.fetch_addr(bus);
                    self.set_rp(p, nn);
                    10
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

                    self.hl = self.wrap_pc(result);
                    11
                }
            }
            2 => {
                match (p, q) {
                    (0, 0) => {
                        // LD (BC),A
                        bus.write_byte(self.mask_addr(self.bc), self.a);
                        7
                    }
                    (1, 0) => {
                        // LD (DE),A
                        bus.write_byte(self.mask_addr(self.de), self.a);
                        7
                    }
                    (2, 0) => {
                        // LD (nn),HL
                        let addr = self.fetch_addr(bus);
                        let nn = self.mask_addr(addr);
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
                        let addr = self.fetch_addr(bus);
                        let nn = self.mask_addr(addr);
                        bus.write_byte(nn, self.a);
                        13
                    }
                    (0, 1) => {
                        // LD A,(BC)
                        let addr = self.mask_addr(self.bc);
                        self.a = bus.read_byte(addr);
                        7
                    }
                    (1, 1) => {
                        // LD A,(DE)
                        let addr = self.mask_addr(self.de);
                        self.a = bus.read_byte(addr);
                        7
                    }
                    (2, 1) => {
                        // LD HL,(nn)
                        let addr = self.fetch_addr(bus);
                        let nn = self.mask_addr(addr);
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
                        let addr = self.fetch_addr(bus);
                        let nn = self.mask_addr(addr);
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
                if y == 6 {
                    11
                } else {
                    4
                }
            }
            5 => {
                // DEC r
                let val = self.get_reg8(y, bus);
                let result = self.alu_dec(val);
                self.set_reg8(y, result, bus);
                if y == 6 {
                    11
                } else {
                    4
                }
            }
            6 => {
                // LD r,n
                let n = self.fetch_byte(bus);
                self.set_reg8(y, n, bus);
                if y == 6 {
                    10
                } else {
                    7
                }
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
                        self.f = (self.f & !(flags::F5 | flags::F3))
                            | (self.a & (flags::F5 | flags::F3));
                        4
                    }
                    1 => {
                        // RRCA
                        let c = self.a & 1;
                        self.a = (self.a >> 1) | (c << 7);
                        self.set_flag_c(c != 0);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        self.f = (self.f & !(flags::F5 | flags::F3))
                            | (self.a & (flags::F5 | flags::F3));
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
                        self.f = (self.f & !(flags::F5 | flags::F3))
                            | (self.a & (flags::F5 | flags::F3));
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
                        self.f = (self.f & !(flags::F5 | flags::F3))
                            | (self.a & (flags::F5 | flags::F3));
                        4
                    }
                    4 => {
                        // DAA - Decimal Adjust Accumulator
                        let mut correction: u8 = 0;
                        let mut set_carry = false;
                        let old_a = self.a;
                        let old_h = self.flag_h();
                        let lower_nibble_adjust = old_h || (!self.flag_n() && (old_a & 0x0F) > 9);

                        if lower_nibble_adjust {
                            correction |= 0x06;
                        }

                        if self.flag_c() || (!self.flag_n() && old_a > 0x99) {
                            correction |= 0x60;
                            set_carry = true;
                        }

                        if self.flag_n() {
                            self.a = self.a.wrapping_sub(correction);
                            // After SUB: H set if half-borrow occurred
                            self.set_flag_h(old_h && (old_a & 0x0F) < 6);
                        } else {
                            self.a = self.a.wrapping_add(correction);
                            // After ADD: H set if lower nibble carry occurred
                            self.set_flag_h((old_a & 0x0F) + (correction & 0x0F) > 0x0F);
                        }

                        self.set_sz_flags(self.a);
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
                        self.f = (self.f & !(flags::F5 | flags::F3))
                            | (self.a & (flags::F5 | flags::F3));
                        4
                    }
                    6 => {
                        // SCF
                        self.set_flag_c(true);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        self.f = (self.f & !(flags::F5 | flags::F3))
                            | (self.a & (flags::F5 | flags::F3));
                        4
                    }
                    7 => {
                        // CCF
                        let old_c = self.flag_c();
                        self.set_flag_h(old_c);
                        self.set_flag_c(!old_c);
                        self.set_flag_n(false);
                        self.f = (self.f & !(flags::F5 | flags::F3))
                            | (self.a & (flags::F5 | flags::F3));
                        4
                    }
                    _ => 4,
                }
            }
            _ => 4,
        }
    }

    /// Execute ALU operation (x=2)
    pub fn execute_alu(&mut self, y: u8, val: u8) {
        match y {
            0 => self.a = self.alu_add(val, false),       // ADD
            1 => self.a = self.alu_add(val, true),        // ADC
            2 => self.a = self.alu_sub(val, false, true), // SUB
            3 => self.a = self.alu_sub(val, true, true),  // SBC
            4 => self.alu_and(val),                       // AND
            5 => self.alu_xor(val),                       // XOR
            6 => self.alu_or(val),                        // OR
            7 => {
                self.alu_sub(val, false, false);
            } // CP
            _ => {}
        }
    }

    /// Execute x=3 opcodes
    pub fn execute_x3(&mut self, bus: &mut Bus, y: u8, z: u8, p: u8, q: u8) -> u32 {
        match z {
            0 => {
                // RET cc
                if self.check_cc(y) {
                    self.pc = self.pop_addr(bus);
                    if self.adl {
                        12
                    } else {
                        11
                    }
                } else {
                    5
                }
            }
            1 => {
                if q == 0 {
                    // POP rp2 - BC/DE/HL are 24-bit in ADL mode, AF is always 16-bit
                    if p == 3 {
                        // AF is always 16-bit
                        let val = self.pop_word(bus);
                        self.a = (val >> 8) as u8;
                        self.f = val as u8;
                    } else if self.adl {
                        // BC/DE/HL are 24-bit in ADL mode (matches CEmu cpu_push_word)
                        let val = self.pop_addr(bus);
                        match p {
                            0 => self.bc = val,
                            1 => self.de = val,
                            2 => self.hl = val,
                            _ => {}
                        }
                    } else {
                        let val = self.pop_word(bus);
                        self.set_rp2(p, val);
                    }
                    10
                } else {
                    match p {
                        0 => {
                            // RET
                            // Note: Cycle count doesn't differ by mode - the timing difference
                            // comes from memory wait states when popping 3 bytes (ADL) vs
                            // 2 bytes (Z80), which is handled by the Bus.
                            self.pc = self.pop_addr(bus);
                            10
                        }
                        1 => {
                            // EXX
                            self.exx();
                            4
                        }
                        2 => {
                            // JP (HL)
                            self.pc = self.wrap_pc(self.hl);
                            4
                        }
                        3 => {
                            // LD SP,HL
                            self.sp = self.wrap_pc(self.hl);
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
                        let sp_addr = self.mask_addr(self.sp);
                        let sp_val = if self.adl {
                            bus.read_addr24(sp_addr)
                        } else {
                            bus.read_word(sp_addr) as u32
                        };
                        if self.adl {
                            bus.write_addr24(sp_addr, self.hl);
                        } else {
                            bus.write_word(sp_addr, self.hl as u16);
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
                    if self.adl {
                        20
                    } else {
                        17
                    }
                } else {
                    if self.adl {
                        13
                    } else {
                        10
                    }
                }
            }
            5 => {
                if q == 0 {
                    // PUSH rp2 - BC/DE/HL are 24-bit in ADL mode, AF is always 16-bit
                    if p == 3 {
                        // AF is always 16-bit
                        let val = ((self.a as u16) << 8) | (self.f as u16);
                        self.push_word(bus, val);
                    } else if self.adl {
                        // BC/DE/HL are 24-bit in ADL mode (matches CEmu cpu_push_word)
                        let val = match p {
                            0 => self.bc,
                            1 => self.de,
                            2 => self.hl,
                            _ => 0,
                        };
                        self.push_addr(bus, val);
                    } else {
                        let val = self.get_rp2(p);
                        self.push_word(bus, val);
                    }
                    11
                } else {
                    match p {
                        0 => {
                            // CALL nn
                            let nn = self.fetch_addr(bus);
                            self.push_addr(bus, self.pc);
                            self.pc = nn;
                            if self.adl {
                                20
                            } else {
                                17
                            }
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
    pub fn execute_cb(&mut self, bus: &mut Bus) -> u32 {
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
                    self.f = (self.f & !(flags::F5 | flags::F3))
                        | ((self.h() as u8) & (flags::F5 | flags::F3));
                } else {
                    self.f = (self.f & !(flags::F5 | flags::F3)) | (val & (flags::F5 | flags::F3));
                }
                if z == 6 {
                    12
                } else {
                    8
                }
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
    pub fn execute_rot(&mut self, y: u8, val: u8) -> u8 {
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
    pub fn execute_ed(&mut self, bus: &mut Bus) -> u32 {
        let opcode = self.fetch_byte(bus);
        let x = (opcode >> 6) & 0x03;
        let y = (opcode >> 3) & 0x07;
        let z = opcode & 0x07;
        let p = (y >> 1) & 0x03;
        let q = y & 0x01;

        match x {
            0 => self.execute_ed_x0(bus, y, z),
            1 => self.execute_ed_x1(bus, y, z, p, q),
            2 => {
                // Block instructions (y >= 4, z <= 3)
                if y >= 4 && z <= 3 {
                    self.execute_bli(bus, y, z)
                } else {
                    8 // NOP for invalid
                }
            }
            _ => 8, // x=3 is NONI (no operation, no interrupt)
        }
    }

    /// Execute ED prefix x=0 opcodes (eZ80-specific I/O instructions)
    pub fn execute_ed_x0(&mut self, bus: &mut Bus, y: u8, z: u8) -> u32 {
        match z {
            0 => {
                // IN0 r,(n) - read from port address 0xFF00nn (eZ80 mapped I/O)
                // The port byte n maps to address 0xFF0000 + n, which corresponds to
                // the control ports region at 0xE000nn (aliased at 0xFF00nn).
                let port = self.fetch_byte(bus) as u32;
                let addr = 0xFF0000 | port;
                let val = bus.read_byte(addr);
                if y != 6 {
                    self.set_reg8(y, val, bus);
                }
                // Set flags for IN0
                self.set_sz_flags(val);
                self.set_flag_h(false);
                self.set_flag_n(false);
                self.set_flag_pv(Self::parity(val));
                12
            }
            1 => {
                // OUT0 (n),r - write to port address 0xFF00nn (eZ80 mapped I/O)
                // The port byte n maps to address 0xFF0000 + n, which corresponds to
                // the control ports region at 0xE000nn (aliased at 0xFF00nn).
                let port = self.fetch_byte(bus) as u32;
                let addr = 0xFF0000 | port;
                let val = self.get_reg8(y, bus);
                bus.write_byte(addr, val);
                12
            }
            4 => {
                // TST A,r - test register (eZ80-specific)
                let val = self.get_reg8(y, bus);
                let result = self.a & val;
                self.set_sz_flags(result);
                self.set_flag_h(true);
                self.set_flag_n(false);
                self.set_flag_pv(Self::parity(result));
                self.set_flag_c(false);
                8
            }
            6 if y == 7 => {
                // SLP - enter sleep mode (on eZ80, similar to HALT but lower power)
                // For now, treat as HALT
                self.halted = true;
                8
            }
            _ => 8, // Other x=0 opcodes are NONI
        }
    }

    /// Execute ED prefix x=1 opcodes
    pub fn execute_ed_x1(&mut self, bus: &mut Bus, y: u8, z: u8, p: u8, q: u8) -> u32 {
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

                    // Flags - sign bit position depends on mode
                    let sign_bit = if self.adl { 0x800000 } else { 0x8000 };
                    let max = if self.adl { 0xFFFFFF } else { 0xFFFF };
                    let half = (hl & 0xFFF) < (rp & 0xFFF) + c;
                    // Overflow: different sign inputs, and result has same sign as subtrahend
                    let overflow = ((hl ^ rp) & sign_bit != 0) && ((hl ^ result) & sign_bit != 0);

                    self.hl = self.wrap_pc(result);

                    self.f = 0;
                    self.set_flag_s((self.hl >> (if self.adl { 23 } else { 15 })) & 1 != 0);
                    self.set_flag_z((self.hl & max) == 0);
                    self.set_flag_h(half);
                    self.set_flag_pv(overflow);
                    self.set_flag_n(true);
                    self.set_flag_c(hl < rp + c);
                    // F3/F5 from high byte of result
                    let high_byte = if self.adl {
                        (self.hl >> 16) as u8
                    } else {
                        (self.hl >> 8) as u8
                    };
                    self.f =
                        (self.f & !(flags::F5 | flags::F3)) | (high_byte & (flags::F5 | flags::F3));
                    15
                } else {
                    // ADC HL,rp
                    let hl = self.hl;
                    let rp = self.get_rp(p);
                    let c = if self.flag_c() { 1u32 } else { 0 };
                    let result = hl.wrapping_add(rp).wrapping_add(c);

                    // Flags - sign bit position depends on mode
                    let sign_bit = if self.adl { 0x800000 } else { 0x8000 };
                    let half = ((hl & 0xFFF) + (rp & 0xFFF) + c) > 0xFFF;
                    // Overflow: same sign inputs, different sign result
                    let overflow = ((hl ^ rp) & sign_bit == 0) && ((hl ^ result) & sign_bit != 0);
                    let max = if self.adl { 0xFFFFFF } else { 0xFFFF };

                    self.hl = self.wrap_pc(result);

                    self.f = 0;
                    self.set_flag_s((self.hl >> (if self.adl { 23 } else { 15 })) & 1 != 0);
                    self.set_flag_z((self.hl & max) == 0);
                    self.set_flag_h(half);
                    self.set_flag_pv(overflow);
                    self.set_flag_n(false);
                    self.set_flag_c(result > max);
                    let high_byte = if self.adl {
                        (self.hl >> 16) as u8
                    } else {
                        (self.hl >> 8) as u8
                    };
                    self.f =
                        (self.f & !(flags::F5 | flags::F3)) | (high_byte & (flags::F5 | flags::F3));
                    15
                }
            }
            3 => {
                // LD (nn),rp / LD rp,(nn)
                let addr = self.fetch_addr(bus);
                let nn = self.mask_addr(addr);
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
                    if self.adl {
                        23
                    } else {
                        20
                    }
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
                        let addr = self.mask_addr(self.hl);
                        let mem = bus.read_byte(addr);
                        let new_mem = (self.a << 4) | (mem >> 4);
                        self.a = (self.a & 0xF0) | (mem & 0x0F);
                        bus.write_byte(addr, new_mem);

                        self.set_sz_flags(self.a);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        self.set_flag_pv(Self::parity(self.a));
                        18
                    }
                    5 => {
                        // RLD
                        let addr = self.mask_addr(self.hl);
                        let mem = bus.read_byte(addr);
                        let new_mem = (mem << 4) | (self.a & 0x0F);
                        self.a = (self.a & 0xF0) | (mem >> 4);
                        bus.write_byte(addr, new_mem);

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
    pub fn execute_bli(&mut self, bus: &mut Bus, y: u8, z: u8) -> u32 {
        match (y, z) {
            // LDI - Load and increment
            (4, 0) => {
                let val = bus.read_byte(self.mask_addr(self.hl));
                bus.write_byte(self.mask_addr(self.de), val);
                self.hl = self.wrap_pc(self.hl.wrapping_add(1));
                self.de = self.wrap_pc(self.de.wrapping_add(1));
                // BC is a counter, not an address - don't use mask_addr
                self.bc = self.bc.wrapping_sub(1) & if self.adl { 0xFFFFFF } else { 0xFFFF };

                self.set_flag_h(false);
                self.set_flag_n(false);
                self.set_flag_pv(self.bc != 0);
                let n = val.wrapping_add(self.a);
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);
                16
            }
            // LDD - Load and decrement
            (5, 0) => {
                let val = bus.read_byte(self.mask_addr(self.hl));
                bus.write_byte(self.mask_addr(self.de), val);
                self.hl = self.wrap_pc(self.hl.wrapping_sub(1));
                self.de = self.wrap_pc(self.de.wrapping_sub(1));
                // BC is a counter, not an address - don't use mask_addr
                self.bc = self.bc.wrapping_sub(1) & if self.adl { 0xFFFFFF } else { 0xFFFF };

                self.set_flag_h(false);
                self.set_flag_n(false);
                self.set_flag_pv(self.bc != 0);
                let n = val.wrapping_add(self.a);
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);
                16
            }
            // LDIR - Load, increment, repeat
            (6, 0) => {
                let val = bus.read_byte(self.mask_addr(self.hl));
                bus.write_byte(self.mask_addr(self.de), val);
                self.hl = self.wrap_pc(self.hl.wrapping_add(1));
                self.de = self.wrap_pc(self.de.wrapping_add(1));
                self.bc = self.bc.wrapping_sub(1) & if self.adl { 0xFFFFFF } else { 0xFFFF };

                self.set_flag_h(false);
                self.set_flag_n(false);
                self.set_flag_pv(self.bc != 0);
                let n = val.wrapping_add(self.a);
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);

                if self.bc != 0 {
                    self.pc = self.wrap_pc(self.pc.wrapping_sub(2));
                    21
                } else {
                    16
                }
            }
            // LDDR - Load, decrement, repeat
            (7, 0) => {
                let val = bus.read_byte(self.mask_addr(self.hl));
                bus.write_byte(self.mask_addr(self.de), val);
                self.hl = self.wrap_pc(self.hl.wrapping_sub(1));
                self.de = self.wrap_pc(self.de.wrapping_sub(1));
                self.bc = self.bc.wrapping_sub(1) & if self.adl { 0xFFFFFF } else { 0xFFFF };

                self.set_flag_h(false);
                self.set_flag_n(false);
                self.set_flag_pv(self.bc != 0);
                let n = val.wrapping_add(self.a);
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);

                if self.bc != 0 {
                    self.pc = self.wrap_pc(self.pc.wrapping_sub(2));
                    21
                } else {
                    16
                }
            }
            // CPI - Compare and increment
            (4, 1) => {
                let val = bus.read_byte(self.mask_addr(self.hl));
                let result = self.a.wrapping_sub(val);
                self.hl = self.wrap_pc(self.hl.wrapping_add(1));
                // BC is a counter, not an address
                self.bc = self.bc.wrapping_sub(1) & if self.adl { 0xFFFFFF } else { 0xFFFF };

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
                let val = bus.read_byte(self.mask_addr(self.hl));
                let result = self.a.wrapping_sub(val);
                self.hl = self.wrap_pc(self.hl.wrapping_sub(1));
                // BC is a counter, not an address
                self.bc = self.bc.wrapping_sub(1) & if self.adl { 0xFFFFFF } else { 0xFFFF };

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
                let val = bus.read_byte(self.mask_addr(self.hl));
                let result = self.a.wrapping_sub(val);
                self.hl = self.wrap_pc(self.hl.wrapping_add(1));
                // BC is a counter, not an address
                self.bc = self.bc.wrapping_sub(1) & if self.adl { 0xFFFFFF } else { 0xFFFF };

                self.set_sz_flags(result);
                self.set_flag_h((self.a & 0x0F) < (val & 0x0F));
                self.set_flag_n(true);
                self.set_flag_pv(self.bc != 0);
                let n = result.wrapping_sub(if self.flag_h() { 1 } else { 0 });
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);

                if self.bc != 0 && result != 0 {
                    self.pc = self.wrap_pc(self.pc.wrapping_sub(2));
                    21
                } else {
                    16
                }
            }
            // CPDR - Compare, decrement, repeat
            (7, 1) => {
                let val = bus.read_byte(self.mask_addr(self.hl));
                let result = self.a.wrapping_sub(val);
                self.hl = self.wrap_pc(self.hl.wrapping_sub(1));
                // BC is a counter, not an address
                self.bc = self.bc.wrapping_sub(1) & if self.adl { 0xFFFFFF } else { 0xFFFF };

                self.set_sz_flags(result);
                self.set_flag_h((self.a & 0x0F) < (val & 0x0F));
                self.set_flag_n(true);
                self.set_flag_pv(self.bc != 0);
                let n = result.wrapping_sub(if self.flag_h() { 1 } else { 0 });
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);

                if self.bc != 0 && result != 0 {
                    self.pc = self.wrap_pc(self.pc.wrapping_sub(2));
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
    pub fn execute_index(&mut self, bus: &mut Bus, use_ix: bool) -> u32 {
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
                    if y == 6 || z == 6 {
                        19
                    } else {
                        8
                    }
                }
            }
            2 => {
                // ALU A,r with indexed addressing
                let val = self.get_index_reg8(z, bus, use_ix);
                self.execute_alu(y, val);
                if z == 6 {
                    19
                } else {
                    8
                }
            }
            3 => self.execute_index_x3(bus, y, z, p, q, use_ix),
            _ => 8,
        }
    }

    /// Get 8-bit register with IX/IY substitution
    /// 4=IXH/IYH, 5=IXL/IYL, 6=(IX+d)/(IY+d)
    pub(super) fn get_index_reg8(&mut self, idx: u8, bus: &mut Bus, use_ix: bool) -> u8 {
        match idx {
            0 => self.b(),
            1 => self.c(),
            2 => self.d(),
            3 => self.e(),
            4 => {
                // H -> IXH/IYH
                if use_ix {
                    self.ixh()
                } else {
                    self.iyh()
                }
            }
            5 => {
                // L -> IXL/IYL
                if use_ix {
                    self.ixl()
                } else {
                    self.iyl()
                }
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
    pub(super) fn set_index_reg8(&mut self, idx: u8, val: u8, bus: &mut Bus, use_ix: bool) {
        match idx {
            0 => self.set_b(val),
            1 => self.set_c(val),
            2 => self.set_d(val),
            3 => self.set_e(val),
            4 => {
                // H -> IXH/IYH
                if use_ix {
                    self.set_ixh(val)
                } else {
                    self.set_iyh(val)
                }
            }
            5 => {
                // L -> IXL/IYL
                if use_ix {
                    self.set_ixl(val)
                } else {
                    self.set_iyl(val)
                }
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
    pub fn execute_index_x0(
        &mut self,
        bus: &mut Bus,
        y: u8,
        z: u8,
        p: u8,
        q: u8,
        use_ix: bool,
    ) -> u32 {
        match z {
            0 => {
                // These don't use HL, just execute normally
                match y {
                    0 => 4, // NOP
                    1 => {
                        self.ex_af();
                        4
                    }
                    2 => {
                        // DJNZ d
                        let d = self.fetch_byte(bus) as i8;
                        self.set_b(self.b().wrapping_sub(1));
                        if self.b() != 0 {
                            self.pc = self.wrap_pc((self.pc as i32 + d as i32) as u32);
                            13
                        } else {
                            8
                        }
                    }
                    3 => {
                        // JR d
                        let d = self.fetch_byte(bus) as i8;
                        self.pc = self.wrap_pc((self.pc as i32 + d as i32) as u32);
                        12
                    }
                    4..=7 => {
                        // JR cc,d
                        let d = self.fetch_byte(bus) as i8;
                        if self.check_cc(y - 4) {
                            self.pc = self.wrap_pc((self.pc as i32 + d as i32) as u32);
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
                        // Note: Cycle count doesn't differ by mode - timing difference
                        // from fetching 3-byte vs 2-byte immediate is handled by Bus wait states.
                        let nn = self.fetch_addr(bus);
                        if use_ix {
                            self.ix = nn;
                        } else {
                            self.iy = nn;
                        }
                        14
                    } else {
                        // LD rp,nn (not affected by prefix for BC/DE/SP)
                        // Note: Cycle count doesn't differ by mode - timing difference
                        // from fetching 3-byte vs 2-byte immediate is handled by Bus wait states.
                        let nn = self.fetch_addr(bus);
                        self.set_rp(p, nn);
                        10
                    }
                } else {
                    if p == 2 {
                        // ADD IX/IY,IX/IY
                        let index_reg = if use_ix { self.ix } else { self.iy };
                        let rp = self.get_index_rp(p, use_ix);
                        let result = index_reg.wrapping_add(rp);

                        let half = ((index_reg & 0xFFF) + (rp & 0xFFF)) > 0xFFF;
                        self.set_flag_h(half);
                        self.set_flag_n(false);
                        self.set_flag_c(result > if self.adl { 0xFFFFFF } else { 0xFFFF });

                        let wrapped = self.wrap_pc(result);
                        if use_ix {
                            self.ix = wrapped;
                        } else {
                            self.iy = wrapped;
                        }
                        // F3/F5 from high byte of result
                        let high_byte = if self.adl {
                            (wrapped >> 16) as u8
                        } else {
                            (wrapped >> 8) as u8
                        };
                        self.f =
                            (self.f & !(flags::F5 | flags::F3)) | (high_byte & (flags::F5 | flags::F3));
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

                        let wrapped = self.wrap_pc(result);
                        if use_ix {
                            self.ix = wrapped;
                        } else {
                            self.iy = wrapped;
                        }
                        // F3/F5 from high byte of result
                        let high_byte = if self.adl {
                            (wrapped >> 16) as u8
                        } else {
                            (wrapped >> 8) as u8
                        };
                        self.f =
                            (self.f & !(flags::F5 | flags::F3)) | (high_byte & (flags::F5 | flags::F3));
                        15
                    }
                }
            }
            2 => {
                match (p, q) {
                    (2, 0) => {
                        // LD (nn),IX/IY
                        let addr = self.fetch_addr(bus);
                        let nn = self.mask_addr(addr);
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
                        let addr = self.fetch_addr(bus);
                        let nn = self.mask_addr(addr);
                        let val = if self.adl {
                            bus.read_addr24(nn)
                        } else {
                            bus.read_word(nn) as u32
                        };
                        if use_ix {
                            self.ix = val;
                        } else {
                            self.iy = val;
                        }
                        if self.adl {
                            20
                        } else {
                            16
                        }
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
                            self.ix = self.wrap_pc(self.ix.wrapping_add(1));
                        } else {
                            self.iy = self.wrap_pc(self.iy.wrapping_add(1));
                        }
                        10
                    } else {
                        // DEC IX/IY
                        if use_ix {
                            self.ix = self.wrap_pc(self.ix.wrapping_sub(1));
                        } else {
                            self.iy = self.wrap_pc(self.iy.wrapping_sub(1));
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
                if y == 6 {
                    // (IX+d)/(IY+d) - fetch displacement once and cache address (matches CEmu)
                    let d = self.fetch_byte(bus) as i8;
                    let index_reg = if use_ix { self.ix } else { self.iy };
                    let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
                    let val = bus.read_byte(addr);
                    let result = self.alu_inc(val);
                    bus.write_byte(addr, result);
                    23
                } else {
                    let val = self.get_index_reg8(y, bus, use_ix);
                    let result = self.alu_inc(val);
                    self.set_index_reg8_no_disp(y, result, use_ix);
                    8
                }
            }
            5 => {
                // DEC r with indexed addressing
                if y == 6 {
                    // (IX+d)/(IY+d) - fetch displacement once and cache address (matches CEmu)
                    let d = self.fetch_byte(bus) as i8;
                    let index_reg = if use_ix { self.ix } else { self.iy };
                    let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
                    let val = bus.read_byte(addr);
                    let result = self.alu_dec(val);
                    bus.write_byte(addr, result);
                    23
                } else {
                    let val = self.get_index_reg8(y, bus, use_ix);
                    let result = self.alu_dec(val);
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
    pub(super) fn set_index_reg8_no_disp(&mut self, idx: u8, val: u8, use_ix: bool) {
        match idx {
            0 => self.set_b(val),
            1 => self.set_c(val),
            2 => self.set_d(val),
            3 => self.set_e(val),
            4 => {
                if use_ix {
                    self.set_ixh(val)
                } else {
                    self.set_iyh(val)
                }
            }
            5 => {
                if use_ix {
                    self.set_ixl(val)
                } else {
                    self.set_iyl(val)
                }
            }
            7 => self.a = val,
            _ => {}
        }
    }

    /// Get register pair for indexed ADD
    pub(super) fn get_index_rp(&self, p: u8, use_ix: bool) -> u32 {
        match p {
            0 => self.bc,
            1 => self.de,
            2 => {
                if use_ix {
                    self.ix
                } else {
                    self.iy
                }
            }
            3 => self.sp,
            _ => 0,
        }
    }

    /// Execute indexed x=3 opcodes
    pub fn execute_index_x3(
        &mut self,
        bus: &mut Bus,
        y: u8,
        z: u8,
        p: u8,
        q: u8,
        use_ix: bool,
    ) -> u32 {
        match z {
            0 => {
                // RET cc - not affected
                if self.check_cc(y) {
                    self.pc = self.pop_addr(bus);
                    if self.adl {
                        12
                    } else {
                        11
                    }
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
                        if use_ix {
                            self.ix = val;
                        } else {
                            self.iy = val;
                        }
                        14
                    } else if p == 3 {
                        // AF is always 16-bit
                        let val = self.pop_word(bus);
                        self.a = (val >> 8) as u8;
                        self.f = val as u8;
                        10
                    } else if self.adl {
                        // BC/DE are 24-bit in ADL mode
                        let val = self.pop_addr(bus);
                        match p {
                            0 => self.bc = val,
                            1 => self.de = val,
                            _ => {}
                        }
                        10
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
                            self.pc = self.wrap_pc(index_reg);
                            8
                        }
                        3 => {
                            // LD SP,IX/IY
                            let index_reg = if use_ix { self.ix } else { self.iy };
                            self.sp = self.wrap_pc(index_reg);
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
                        let sp_addr = self.mask_addr(self.sp);
                        let sp_val = if self.adl {
                            bus.read_addr24(sp_addr)
                        } else {
                            bus.read_word(sp_addr) as u32
                        };
                        let index_reg = if use_ix { self.ix } else { self.iy };
                        if self.adl {
                            bus.write_addr24(sp_addr, index_reg);
                        } else {
                            bus.write_word(sp_addr, index_reg as u16);
                        }
                        if use_ix {
                            self.ix = sp_val;
                        } else {
                            self.iy = sp_val;
                        }
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
                    if self.adl {
                        20
                    } else {
                        17
                    }
                } else {
                    if self.adl {
                        13
                    } else {
                        10
                    }
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
                    } else if p == 3 {
                        // AF is always 16-bit
                        let val = ((self.a as u16) << 8) | (self.f as u16);
                        self.push_word(bus, val);
                        11
                    } else if self.adl {
                        // BC/DE are 24-bit in ADL mode
                        let val = match p {
                            0 => self.bc,
                            1 => self.de,
                            _ => 0,
                        };
                        self.push_addr(bus, val);
                        11
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
                            if self.adl {
                                20
                            } else {
                                17
                            }
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
    pub fn execute_index_cb(&mut self, bus: &mut Bus, use_ix: bool) -> u32 {
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
                self.f = (self.f & !(flags::F5 | flags::F3))
                    | (((addr >> 8) as u8) & (flags::F5 | flags::F3));
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
