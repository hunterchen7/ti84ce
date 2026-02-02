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
                            bus.add_cycles(1); // CEmu: cpu.cycles++ for branch taken
                            let target = self.wrap_pc((self.pc as i32 + d as i32) as u32);
                            self.prefetch(bus, target); // CEmu: cpu_prefetch(target)
                            self.pc = target;
                            13
                        } else {
                            8
                        }
                    }
                    3 => {
                        // JR d (unconditional)
                        let d = self.fetch_byte(bus) as i8;
                        let target = self.wrap_pc((self.pc as i32 + d as i32) as u32);
                        self.prefetch(bus, target); // CEmu: cpu_prefetch(target)
                        self.pc = target;
                        12
                    }
                    4..=7 => {
                        // JR cc,d
                        let d = self.fetch_byte(bus) as i8;
                        if self.check_cc(y - 4) {
                            bus.add_cycles(1); // CEmu: cpu.cycles++ for branch taken
                            let target = self.wrap_pc((self.pc as i32 + d as i32) as u32);
                            self.prefetch(bus, target); // CEmu: cpu_prefetch(target)
                            self.pc = target;
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
                    let mask = if self.l { 0xFFFFFF } else { 0xFFFF };
                    let hl = self.hl & mask;
                    let rp = self.get_rp(p) & mask;
                    let result = hl.wrapping_add(rp);

                    // Set flags
                    let half = ((hl & 0xFFF) + (rp & 0xFFF)) > 0xFFF;
                    self.set_flag_h(half);
                    self.set_flag_n(false);
                    self.set_flag_c(result > mask);

                    self.hl = result & mask;
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
                        let hl = self.get_rp(2);
                        if self.l {
                            bus.write_addr24(nn, hl);
                            20
                        } else {
                            bus.write_word(nn, hl as u16);
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
                        let val = if self.l {
                            bus.read_addr24(nn)
                        } else {
                            bus.read_word(nn) as u32
                        };
                        self.set_rp(2, val);
                        if self.l { 20 } else { 16 }
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
                if y == 6 {
                    bus.add_cycles(1); // CEmu: cpu.cycles += context.y == 6
                }
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
                if y == 6 {
                    bus.add_cycles(1); // CEmu: cpu.cycles += context.y == 6
                }
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
                        // RLCA - CEmu preserves F3/F5
                        let c = (self.a >> 7) & 1;
                        self.a = (self.a << 1) | c;
                        self.set_flag_c(c != 0);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        // F3/F5 preserved from before (CEmu behavior)
                        4
                    }
                    1 => {
                        // RRCA - CEmu preserves F3/F5
                        let c = self.a & 1;
                        self.a = (self.a >> 1) | (c << 7);
                        self.set_flag_c(c != 0);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        // F3/F5 preserved from before (CEmu behavior)
                        4
                    }
                    2 => {
                        // RLA - CEmu preserves F3/F5
                        let old_c = if self.flag_c() { 1 } else { 0 };
                        let new_c = (self.a >> 7) & 1;
                        self.a = (self.a << 1) | old_c;
                        self.set_flag_c(new_c != 0);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        // F3/F5 preserved from before (CEmu behavior)
                        4
                    }
                    3 => {
                        // RRA - CEmu preserves F3/F5
                        let old_c = if self.flag_c() { 0x80 } else { 0 };
                        let new_c = self.a & 1;
                        self.a = (self.a >> 1) | old_c;
                        self.set_flag_c(new_c != 0);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        // F3/F5 preserved from before (CEmu behavior)
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
                        // CPL - CEmu preserves F3/F5
                        self.a = !self.a;
                        self.set_flag_h(true);
                        self.set_flag_n(true);
                        // F3/F5 preserved from before (CEmu behavior)
                        4
                    }
                    6 => {
                        // SCF - CEmu preserves F3/F5
                        self.set_flag_c(true);
                        self.set_flag_h(false);
                        self.set_flag_n(false);
                        // F3/F5 preserved from before (CEmu behavior)
                        4
                    }
                    7 => {
                        // CCF - CEmu preserves F3/F5
                        let old_c = self.flag_c();
                        self.set_flag_h(old_c);
                        self.set_flag_c(!old_c);
                        self.set_flag_n(false);
                        // F3/F5 preserved from before (CEmu behavior)
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
                // Uses L mode for stack operations, then ADL becomes L
                bus.add_cycles(1); // CEmu: cpu.cycles++ before condition check
                if self.check_cc(y) {
                    self.pc = self.pop_addr(bus);
                    self.adl = self.l;
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
                    // POP rp2 - stack width uses L mode (CEmu behavior)
                    let val = self.pop_addr(bus);
                    if p == 3 {
                        // AF - upper byte discarded in 24-bit mode
                        self.a = (val >> 8) as u8;
                        self.f = val as u8;
                    } else {
                        self.set_rp(p, val);
                    }
                    10
                } else {
                    match p {
                        0 => {
                            // RET
                            // Uses L mode for stack operations, then ADL becomes L
                            bus.add_cycles(1); // CEmu: cpu.cycles++ in cpu_return()
                            self.pc = self.pop_addr(bus);
                            self.adl = self.l;
                            10
                        }
                        1 => {
                            // EXX
                            self.exx();
                            4
                        }
                        2 => {
                            // JP (HL)
                            self.adl = self.l;
                            self.pc = self.wrap_pc(self.hl);
                            4
                        }
                        3 => {
                            // LD SP,HL
                            self.sp = self.wrap_data(self.hl);
                            6
                        }
                        _ => 4,
                    }
                }
            }
            2 => {
                // JP cc,nn
                // Fetch uses IL mode, then ADL becomes IL if jump is taken
                let nn = self.fetch_addr(bus);
                if self.check_cc(y) {
                    bus.add_cycles(1); // CEmu: cpu.cycles++ for jump taken
                    self.pc = nn;
                    self.adl = self.il;
                }
                10
            }
            3 => {
                match y {
                    0 => {
                        // JP nn
                        // Fetch uses IL mode (set by suffix), then ADL becomes IL
                        bus.add_cycles(1); // CEmu: cpu.cycles++ for JP nn
                        self.pc = self.fetch_addr(bus);
                        self.adl = self.il;
                        10
                    }
                    1 => {
                        // CB prefix (bit operations)
                        self.execute_cb(bus)
                    }
                    2 => {
                        // OUT (n),A - write to I/O port (A << 8) | n
                        let n = self.fetch_byte(bus);
                        let port = ((self.a as u16) << 8) | (n as u16);
                        bus.port_write(port, self.a);
                        11
                    }
                    3 => {
                        // IN A,(n) - read from I/O port (A << 8) | n
                        let n = self.fetch_byte(bus);
                        let port = ((self.a as u16) << 8) | (n as u16);
                        self.a = bus.port_read(port);
                        11
                    }
                    4 => {
                        // EX (SP),HL
                        let sp_addr = self.mask_addr(self.sp);
                        let sp_val = if self.l {
                            bus.read_addr24(sp_addr)
                        } else {
                            bus.read_word(sp_addr) as u32
                        };
                        if self.l {
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
                        // EI - enable interrupts after the NEXT instruction completes
                        // Set delay counter to 2:
                        // - Step N (this step): EI executes, ei_delay = 2
                        // - Step N+1: ei_delay decrements to 1, IFF1 still false
                        //             instruction following EI executes fully
                        // - Step N+2: ei_delay decrements to 0, IFF1 = true
                        //             next interrupt check can fire
                        self.ei_delay = 2;
                        4
                    }
                    _ => 4,
                }
            }
            4 => {
                // CALL cc,nn
                // Fetch uses IL mode, push uses L mode, then ADL becomes IL if call is taken
                let nn = self.fetch_addr(bus);
                if self.check_cc(y) {
                    // CEmu: cpu.cycles += !cpu.SUFFIX && !cpu.ADL (only in Z80 mode)
                    if !self.suffix && !self.adl {
                        bus.add_cycles(1);
                    }
                    self.push_addr(bus, self.pc);
                    self.pc = nn;
                    self.adl = self.il;
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
                    // PUSH rp2 - stack width uses L mode (CEmu behavior)
                    let val = if p == 3 {
                        // AF - upper byte is 0 in 24-bit mode
                        ((self.a as u32) << 8) | (self.f as u32)
                    } else {
                        self.get_rp(p)
                    };
                    self.push_addr(bus, val);
                    11
                } else {
                    match p {
                        0 => {
                            // CALL nn
                            // Fetch uses IL mode, push uses L mode, then ADL becomes IL
                            let nn = self.fetch_addr(bus);
                            self.push_addr(bus, self.pc);
                            self.pc = nn;
                            self.adl = self.il;
                            if self.adl {
                                20
                            } else {
                                17
                            }
                        }
                        1 => {
                            // DD prefix (IX instructions)
                            // CEmu counts prefixes as separate instruction steps
                            // Set prefix flag and return - execute_index() will be called on next step()
                            // If next byte is ED, CEmu ignores DD and executes ED in the same step
                            let next = bus.peek_byte_fetch(self.mask_addr_instr(self.pc));
                            if next == 0xED {
                                self.execute_ed(bus)
                            } else {
                                // Preserve suffix modes (L/IL) for the indexed instruction
                                // See findings.md: suffix opcodes should apply to the entire
                                // next instruction including any DD/FD prefixes
                                self.suffix = true;
                                self.prefix = 2;
                                4
                            }
                        }
                        2 => {
                            // ED prefix (extended instructions)
                            self.execute_ed(bus)
                        }
                        3 => {
                            // FD prefix (IY instructions)
                            // CEmu counts prefixes as separate instruction steps
                            // If next byte is ED, CEmu ignores FD and executes ED in the same step
                            let next = bus.peek_byte_fetch(self.mask_addr_instr(self.pc));
                            if next == 0xED {
                                self.execute_ed(bus)
                            } else {
                                // Preserve suffix modes (L/IL) for the indexed instruction
                                // See findings.md: suffix opcodes should apply to the entire
                                // next instruction including any DD/FD prefixes
                                self.suffix = true;
                                self.prefix = 3;
                                4
                            }
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
                bus.add_cycles(1); // CEmu: cpu.cycles++ in cpu_rst()
                self.push_addr(bus, self.pc);
                self.adl = self.l;
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
                if z == 6 {
                    bus.add_cycles(1); // CEmu: cpu.cycles += z == 6 in cpu_execute_rot
                }
                let result = self.execute_rot(y, val);
                self.set_reg8(z, result, bus);
                cycles
            }
            1 => {
                // BIT y, r[z] - test bit (no extra cycle for z==6)
                let mask = 1 << y;
                let result = val & mask;

                // Set flags: Z if bit is zero, S from bit 7 if testing bit 7
                // Preserve carry and undocumented F3/F5 bits (CEmu behavior)
                self.f &= flags::C | flags::F5 | flags::F3;
                self.set_flag_z(result == 0);
                self.set_flag_h(true);
                self.set_flag_n(false);
                self.set_flag_pv(result == 0); // PV is same as Z for BIT
                if y == 7 && result != 0 {
                    self.f |= flags::S;
                }
                if z == 6 {
                    12
                } else {
                    8
                }
            }
            2 => {
                // RES y, r[z] - reset bit
                if z == 6 {
                    bus.add_cycles(1); // CEmu: cpu.cycles += context.z == 6
                }
                let result = val & !(1 << y);
                self.set_reg8(z, result, bus);
                cycles
            }
            3 => {
                // SET y, r[z] - set bit
                if z == 6 {
                    bus.add_cycles(1); // CEmu: cpu.cycles += context.z == 6
                }
                let result = val | (1 << y);
                self.set_reg8(z, result, bus);
                cycles
            }
            _ => 8,
        }
    }

    /// Execute rotate/shift operation (CB prefix, x=0)
    /// CEmu: preserves F3/F5 from previous F (cpuflag_undef behavior)
    pub fn execute_rot(&mut self, y: u8, val: u8) -> u8 {
        // Preserve F3/F5 from previous F (CEmu: cpuflag_undef(r->F))
        let old_f3f5 = self.f & (flags::F5 | flags::F3);

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

        // Set S, Z, P flags while preserving F3/F5
        self.set_flag_h(false);
        self.set_flag_n(false);
        self.set_sz_flags(result);
        self.f = (self.f & !(flags::F5 | flags::F3)) | old_f3f5; // Restore F3/F5
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
                // Block instructions
                // Standard Z80: y >= 4, z <= 3 (LDI/CPI/INI/OUTI and variants)
                // eZ80 extended: y < 4, z = 2 or 3 (INIM/OTIM and variants)
                if z <= 3 {
                    if y >= 4 {
                        // Standard Z80 block instructions
                        self.execute_bli(bus, y, z)
                    } else {
                        // eZ80 extended block I/O instructions
                        self.execute_bli_ez80(bus, y, z, p, q)
                    }
                } else {
                    8 // NOP for invalid
                }
            }
            _ => 8, // x=3 is NONI (no operation, no interrupt)
        }
    }

    /// Execute ED prefix x=0 opcodes (eZ80-specific I/O instructions)
    pub fn execute_ed_x0(&mut self, bus: &mut Bus, y: u8, z: u8) -> u32 {
        let p = y >> 1;
        let q = y & 1;
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
                // Set flags for IN0 - preserve F3/F5 from existing F (CEmu behavior)
                let old_f3f5 = self.f & (flags::F5 | flags::F3);
                self.f &= !(flags::S | flags::Z | flags::H | flags::N | flags::F5 | flags::F3);
                if val & 0x80 != 0 {
                    self.f |= flags::S;
                }
                if val == 0 {
                    self.f |= flags::Z;
                }
                self.f |= old_f3f5; // Preserve F3/F5
                self.set_flag_pv(Self::parity(val));
                12
            }
            1 => {
                if y == 6 {
                    // LD IY,(HL) - load IY from (HL)
                    // LD IY,(HL) - CEmu uses cpu_mask_mode(value, cpu.L)
                    let addr = self.mask_addr(self.hl);
                    let val = if self.l {
                        bus.read_addr24(addr)
                    } else {
                        bus.read_word(addr) as u32
                    };
                    self.iy = self.wrap_data(val);
                    8
                } else {
                    // OUT0 (n),r - write to port address 0xFF00nn (eZ80 mapped I/O)
                    // The port byte n maps to address 0xFF0000 + n, which corresponds to
                    // the control ports region at 0xE000nn (aliased at 0xFF00nn).
                    let port = self.fetch_byte(bus) as u32;
                    let addr = 0xFF0000 | port;
                    let val = self.get_reg8(y, bus);
                    bus.write_byte(addr, val);
                    12
                }
            }
            2 | 3 => {
                // LEA rp3[p], IX/IY (eZ80-specific)
                if q != 0 {
                    // OPCODETRAP in CEmu - treat as NOP
                    return 8;
                }
                let d = self.fetch_byte(bus) as i8;
                let index_reg = if z == 2 { self.ix } else { self.iy };
                let val = (index_reg as i32 + d as i32) as u32;
                let masked = if self.l { val & 0xFFFFFF } else { val & 0xFFFF };
                match p {
                    0 => self.bc = masked,
                    1 => self.de = masked,
                    2 => self.hl = masked,
                    3 => {
                        if z == 2 {
                            self.ix = masked;
                        } else {
                            self.iy = masked;
                        }
                    }
                    _ => {}
                }
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
                // LD (HL),IY - store IY at (HL)
                let addr = self.mask_addr(self.hl);
                if self.l {
                    bus.write_addr24(addr, self.iy);
                } else {
                    bus.write_word(addr, self.iy as u16);
                }
                8
            }
            7 => {
                // LD rp3[p],(HL) or LD (HL),rp3[p]
                // p = y >> 1, q = y & 1
                let p = y >> 1;
                let q = y & 1;
                let addr = self.mask_addr(self.hl);
                if q == 0 {
                    // LD rp3[p],(HL) - load register pair from (HL)
                    // CEmu: cpu_write_rp3 applies cpu_mask_mode(value, cpu.L)
                    let val = if self.l {
                        bus.read_addr24(addr)
                    } else {
                        bus.read_word(addr) as u32
                    };
                    match p {
                        0 => self.bc = self.wrap_data(val),
                        1 => self.de = self.wrap_data(val),
                        2 => self.hl = self.wrap_data(val),
                        3 => self.iy = self.wrap_data(val), // IY in rp3 context
                        _ => {}
                    }
                } else {
                    // LD (HL),rp3[p] - store register pair at (HL)
                    let val = match p {
                        0 => self.bc,
                        1 => self.de,
                        2 => self.hl,
                        3 => self.iy, // IY in rp3 context
                        _ => 0,
                    };
                    if self.l {
                        bus.write_addr24(addr, val);
                    } else {
                        bus.write_word(addr, val as u16);
                    }
                }
                8
            }
            _ => 8, // Other x=0 opcodes are NONI/undefined
        }
    }

    /// Execute ED prefix x=1 opcodes
    pub fn execute_ed_x1(&mut self, bus: &mut Bus, y: u8, z: u8, p: u8, q: u8) -> u32 {
        match z {
            0 => {
                // IN r,(C) - read from I/O port BC
                // eZ80 uses full BC register as 16-bit port address
                // The port address is routed based on bits 15:12 (see Bus::port_read)
                let port = self.bc as u16;
                let val = bus.port_read(port);
                if y != 6 {
                    self.set_reg8(y, val, bus);
                }
                // Set flags (even for y=6 which is IN F,(C))
                // Preserve F3/F5 from existing F (CEmu behavior)
                let old_f3f5 = self.f & (flags::F5 | flags::F3);
                self.f &= !(flags::S | flags::Z | flags::H | flags::N | flags::F5 | flags::F3);
                if val & 0x80 != 0 {
                    self.f |= flags::S;
                }
                if val == 0 {
                    self.f |= flags::Z;
                }
                self.f |= old_f3f5; // Preserve F3/F5
                self.set_flag_pv(Self::parity(val));
                12
            }
            1 => {
                // OUT (C),r - write to I/O port BC
                // eZ80 uses full BC register as 16-bit port address
                if y == 6 {
                    // OUT (C),0 is OPCODETRAP on eZ80 - treat as NOP for now
                    return 4;
                }
                let port = self.bc as u16;
                let val = self.get_reg8(y, bus);
                bus.port_write(port, val);
                12
            }
            2 => {
                if q == 0 {
                    // SBC HL,rp
                    let mask = if self.l { 0xFFFFFF } else { 0xFFFF };
                    let hl = self.hl & mask;
                    let rp = self.get_rp(p) & mask;
                    let c = if self.flag_c() { 1u32 } else { 0 };
                    let result = hl.wrapping_sub(rp).wrapping_sub(c);
                    let old_f3f5 = self.f & (flags::F5 | flags::F3);

                    // Flags - sign bit position depends on mode
                    let sign_bit = if self.l { 0x800000 } else { 0x8000 };
                    let max = mask;
                    let half = (hl & 0xFFF) < (rp & 0xFFF) + c;
                    // Overflow: different sign inputs, and result has same sign as subtrahend
                    let overflow = ((hl ^ rp) & sign_bit != 0) && ((hl ^ result) & sign_bit != 0);

                    self.hl = result & mask;

                    self.f = 0;
                    self.set_flag_s((self.hl >> (if self.l { 23 } else { 15 })) & 1 != 0);
                    self.set_flag_z((self.hl & max) == 0);
                    self.set_flag_h(half);
                    self.set_flag_pv(overflow);
                    self.set_flag_n(true);
                    self.set_flag_c(hl < rp + c);
                    // Preserve F3/F5 from previous F (CEmu behavior)
                    self.f = (self.f & !(flags::F5 | flags::F3)) | old_f3f5;
                    15
                } else {
                    // ADC HL,rp
                    let mask = if self.l { 0xFFFFFF } else { 0xFFFF };
                    let hl = self.hl & mask;
                    let rp = self.get_rp(p) & mask;
                    let c = if self.flag_c() { 1u32 } else { 0 };
                    let result = hl.wrapping_add(rp).wrapping_add(c);
                    let old_f3f5 = self.f & (flags::F5 | flags::F3);

                    // Flags - sign bit position depends on mode
                    let sign_bit = if self.l { 0x800000 } else { 0x8000 };
                    let half = ((hl & 0xFFF) + (rp & 0xFFF) + c) > 0xFFF;
                    // Overflow: same sign inputs, different sign result
                    let overflow = ((hl ^ rp) & sign_bit == 0) && ((hl ^ result) & sign_bit != 0);
                    let max = mask;

                    self.hl = result & mask;

                    self.f = 0;
                    self.set_flag_s((self.hl >> (if self.l { 23 } else { 15 })) & 1 != 0);
                    self.set_flag_z((self.hl & max) == 0);
                    self.set_flag_h(half);
                    self.set_flag_pv(overflow);
                    self.set_flag_n(false);
                    self.set_flag_c(result > max);
                    // Preserve F3/F5 from previous F (CEmu behavior)
                    self.f = (self.f & !(flags::F5 | flags::F3)) | old_f3f5;
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
                    if self.l {
                        bus.write_addr24(nn, rp);
                        23
                    } else {
                        bus.write_word(nn, rp as u16);
                        20
                    }
                } else {
                    // LD rp,(nn)
                    let val = if self.l {
                        bus.read_addr24(nn)
                    } else {
                        bus.read_word(nn) as u32
                    };
                    self.set_rp(p, val);
                    if self.l {
                        23
                    } else {
                        20
                    }
                }
            }
            4 => {
                if q == 0 {
                    // Various instructions based on p
                    match p {
                        0 => {
                            // NEG (ED 44)
                            // CEmu: preserves F3/F5 from previous F (cpuflag_undef)
                            let old_a = self.a;
                            self.a = 0u8.wrapping_sub(old_a);

                            let old_f3f5 = self.f & (flags::F5 | flags::F3);
                            self.f = 0;
                            self.set_sz_flags(self.a);
                            self.f |= old_f3f5; // Preserve F3/F5
                            self.set_flag_h((0 & 0x0F) < (old_a & 0x0F));
                            self.set_flag_pv(old_a == 0x80);
                            self.set_flag_n(true);
                            self.set_flag_c(old_a != 0);
                            8
                        }
                        1 => {
                            // LEA IX,IY+d (ED 54)
                            // CEmu: cpu_index_address() applies cpu_mask_mode(value, cpu.L)
                            let d = self.fetch_byte(bus) as i8;
                            let addr = (self.iy as i32 + d as i32) as u32;
                            self.ix = self.wrap_data(addr);
                            8
                        }
                        2 => {
                            // TST A,n (ED 64)
                            let n = self.fetch_byte(bus);
                            let result = self.a & n;
                            self.f = 0;
                            self.set_sz_flags(result);
                            self.set_flag_h(true);
                            self.set_flag_pv(Self::parity(result));
                            8
                        }
                        3 => {
                            // TSTIO n (ED 74)
                            let n = self.fetch_byte(bus);
                            let port_val = bus.port_read(self.c() as u16);
                            let result = port_val & n;
                            self.f = 0;
                            self.set_sz_flags(result);
                            self.set_flag_h(true);
                            self.set_flag_pv(Self::parity(result));
                            12
                        }
                        _ => 8,
                    }
                } else {
                    // MLT rp[p] (ED 4C, 5C, 6C, 7C) - multiply high * low byte
                    // p=0: MLT BC (ED 4C)
                    // p=1: MLT DE (ED 5C)
                    // p=2: MLT HL (ED 6C)
                    // p=3: MLT SP (ED 7C) - not used on TI-84 CE
                    let rp = self.get_rp(p);
                    let high = ((rp >> 8) & 0xFF) as u16;
                    let low = (rp & 0xFF) as u16;
                    let result = (high * low) as u32;
                    // CEmu writes the 16-bit result and masks via cpu_mask_mode (upper byte cleared in L mode)
                    self.set_rp(p, result);
                    8 // CEmu adds 4 cycles but base is 4, total 8
                }
            }
            5 => {
                // z=5 has RETN/RETI and eZ80-specific instructions
                // y=0: RETN (ED 45)
                // y=1: RETI (ED 4D)
                // y=4: PEA IX+d (ED 65) - push effective address
                // y=5: LD MB,A (ED 6D) - load A into MBASE (ADL mode only)
                // y=7: STMIX (ED 7D) - set mixed memory mode
                match y {
                    0 => {
                        // RETN - Uses L mode for stack operations, then ADL becomes L
                        bus.add_cycles(1); // CEmu: cpu.cycles++ in cpu_return()
                        self.iff1 = self.iff2;
                        self.pc = self.pop_addr(bus);
                        self.adl = self.l;
                        14
                    }
                    1 => {
                        // RETI - Uses L mode for stack operations, then ADL becomes L
                        bus.add_cycles(1); // CEmu: cpu.cycles++ in cpu_return()
                        self.pc = self.pop_addr(bus);
                        self.adl = self.l;
                        14
                    }
                    4 => {
                        // PEA IX+d - push IX + signed offset
                        let d = self.fetch_byte(bus) as i8;
                        let ea = (self.ix as i32 + d as i32) as u32;
                        let masked = if self.l { ea & 0xFFFFFF } else { ea & 0xFFFF };
                        self.push_addr(bus, masked);
                        16
                    }
                    5 => {
                        // LD MB,A - load A into MBASE (only in ADL mode)
                        if self.adl {
                            self.mbase = self.a;
                        }
                        8
                    }
                    7 => {
                        // STMIX - set mixed memory mode (MADL = 1)
                        self.madl = true;
                        8
                    }
                    _ => {
                        // Other y values are NOP in eZ80
                        8
                    }
                }
            }
            6 => {
                // z=6 has various eZ80-specific instructions based on y
                // eZ80 maps y directly to IM mode (different from standard Z80!)
                // y=0: IM 0 (ED 46)
                // y=1: OPCODETRAP (ED 4E) - trap on eZ80
                // y=2: IM 2 (ED 56) - NOTE: standard Z80 uses IM 1 here, but eZ80 uses IM 2
                // y=3: IM 3 (ED 5E) - eZ80-specific IM 3 mode (not used on TI-84 CE)
                // y=4: PEA IY+d (ED 66) - push effective address
                // y=5: LD A,MB (ED 6E) - load MBASE into A
                // y=6: SLP (ED 76) - sleep/halt
                // y=7: RSMIX (ED 7E) - reset mixed memory mode
                match y {
                    0 => self.im = InterruptMode::Mode0,
                    1 => {
                        // OPCODETRAP - treated as NOP on TI-84 CE
                    }
                    // eZ80 sets IM = y directly, so y=2 becomes IM 2 (Mode2)
                    2 => self.im = InterruptMode::Mode2,
                    // y=3 would be IM 3 on eZ80, but we only have Mode0/1/2
                    // Treat as Mode2 for compatibility (TI-84 CE doesn't use this)
                    3 => self.im = InterruptMode::Mode2,
                    4 => {
                        // PEA IY+d - push IY + signed offset
                        let d = self.fetch_byte(bus) as i8;
                        let ea = (self.iy as i32 + d as i32) as u32;
                        let masked = if self.l { ea & 0xFFFFFF } else { ea & 0xFFFF };
                        self.push_addr(bus, masked);
                        return 16;
                    }
                    5 => {
                        // LD A,MB - load MBASE into A
                        self.a = self.mbase;
                    }
                    6 => {
                        // SLP - sleep (same as HALT on TI-84 CE)
                        bus.add_cycles(1); // CEmu: cpu.cycles++ for SLP
                        self.halted = true;
                        return 4;
                    }
                    7 => {
                        // RSMIX - reset mixed memory mode (MADL = 0)
                        // On TI-84 CE, this is mostly a NOP as mixed mode isn't heavily used
                        self.madl = false;
                    }
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
                        bus.add_cycles(1); // CEmu: cpu.cycles++ for RRD
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
                        bus.add_cycles(1); // CEmu: cpu.cycles++ for RLD
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
            // CEmu preserves F3/F5 flags (doesn't compute from val+A)
            // CEmu: REG_WRITE_EX(HL, r->HL, cpu_mask_mode(r->HL + delta, cpu.L))
            (4, 0) => {
                let val = bus.read_byte(self.mask_addr(self.hl));
                bus.write_byte(self.mask_addr(self.de), val);
                self.hl = self.wrap_data(self.hl.wrapping_add(1));
                self.de = self.wrap_data(self.de.wrapping_add(1));
                // BC is a counter - use L mode for masking (CEmu: cpu_dec_bc_partial_mode)
                self.bc = self.bc.wrapping_sub(1) & if self.l { 0xFFFFFF } else { 0xFFFF };

                self.set_flag_h(false);
                self.set_flag_n(false);
                self.set_flag_pv(self.bc != 0);
                // F3/F5 preserved (CEmu behavior)
                bus.add_cycles(1); // CEmu: cpu.cycles += internalCycles (1 for LDI)
                16
            }
            // LDD - Load and decrement
            // CEmu preserves F3/F5 flags
            // CEmu: REG_WRITE_EX(HL, r->HL, cpu_mask_mode(r->HL + delta, cpu.L))
            (5, 0) => {
                let val = bus.read_byte(self.mask_addr(self.hl));
                bus.write_byte(self.mask_addr(self.de), val);
                self.hl = self.wrap_data(self.hl.wrapping_sub(1));
                self.de = self.wrap_data(self.de.wrapping_sub(1));
                // BC is a counter - use L mode for masking (CEmu: cpu_dec_bc_partial_mode)
                self.bc = self.bc.wrapping_sub(1) & if self.l { 0xFFFFFF } else { 0xFFFF };

                self.set_flag_h(false);
                self.set_flag_n(false);
                self.set_flag_pv(self.bc != 0);
                bus.add_cycles(1); // CEmu: cpu.cycles += internalCycles (1 for LDD)
                // F3/F5 preserved (CEmu behavior)
                16
            }
            // LDIR - Load, increment, repeat
            // Executes all iterations in a single instruction to match CEmu behavior
            // CEmu preserves F3/F5 flags
            // CEmu: REG_WRITE_EX(HL, r->HL, cpu_mask_mode(r->HL + delta, cpu.L))
            (6, 0) => {
                let mut cycles = 0u32;
                loop {
                    let val = bus.read_byte(self.mask_addr(self.hl));
                    bus.write_byte(self.mask_addr(self.de), val);
                    self.hl = self.wrap_data(self.hl.wrapping_add(1));
                    self.de = self.wrap_data(self.de.wrapping_add(1));
                    self.bc = self.bc.wrapping_sub(1) & if self.l { 0xFFFFFF } else { 0xFFFF };

                    self.set_flag_h(false);
                    self.set_flag_n(false);
                    self.set_flag_pv(self.bc != 0);
                    // F3/F5 preserved (CEmu behavior)

                    // CEmu: cpu.cycles += internalCycles (1 for LDIR)
                    bus.add_cycles(1);

                    if self.bc != 0 {
                        cycles += 21;
                        // Continue looping internally
                    } else {
                        cycles += 16;
                        break;
                    }
                }
                cycles
            }
            // LDDR - Load, decrement, repeat
            // Executes all iterations in a single instruction to match CEmu behavior
            // CEmu preserves F3/F5 flags
            // CEmu: REG_WRITE_EX(HL, r->HL, cpu_mask_mode(r->HL + delta, cpu.L))
            (7, 0) => {
                let mut cycles = 0u32;
                loop {
                    let val = bus.read_byte(self.mask_addr(self.hl));
                    bus.write_byte(self.mask_addr(self.de), val);
                    self.hl = self.wrap_data(self.hl.wrapping_sub(1));
                    self.de = self.wrap_data(self.de.wrapping_sub(1));
                    self.bc = self.bc.wrapping_sub(1) & if self.l { 0xFFFFFF } else { 0xFFFF };

                    self.set_flag_h(false);
                    self.set_flag_n(false);
                    self.set_flag_pv(self.bc != 0);
                    // F3/F5 preserved (CEmu behavior)

                    // CEmu: cpu.cycles += internalCycles (1 for LDDR)
                    bus.add_cycles(1);

                    if self.bc != 0 {
                        cycles += 21;
                        // Continue looping internally
                    } else {
                        cycles += 16;
                        break;
                    }
                }
                cycles
            }
            // CPI - Compare and increment
            // CEmu: REG_WRITE_EX(HL, r->HL, cpu_mask_mode(r->HL + delta, cpu.L))
            (4, 1) => {
                let val = bus.read_byte(self.mask_addr(self.hl));
                let result = self.a.wrapping_sub(val);
                self.hl = self.wrap_data(self.hl.wrapping_add(1));
                // BC is a counter - use L mode for masking
                self.bc = self.bc.wrapping_sub(1) & if self.l { 0xFFFFFF } else { 0xFFFF };

                self.set_sz_flags(result);
                self.set_flag_h((self.a & 0x0F) < (val & 0x0F));
                self.set_flag_n(true);
                self.set_flag_pv(self.bc != 0);
                // F3/F5 handling for CPI is complex
                let n = result.wrapping_sub(if self.flag_h() { 1 } else { 0 });
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);
                bus.add_cycles(1); // CEmu: cpu.cycles += internalCycles (1 for CPI)
                16
            }
            // CPD - Compare and decrement
            // CEmu: REG_WRITE_EX(HL, r->HL, cpu_mask_mode(r->HL + delta, cpu.L))
            (5, 1) => {
                let val = bus.read_byte(self.mask_addr(self.hl));
                let result = self.a.wrapping_sub(val);
                self.hl = self.wrap_data(self.hl.wrapping_sub(1));
                // BC is a counter - use L mode for masking
                self.bc = self.bc.wrapping_sub(1) & if self.l { 0xFFFFFF } else { 0xFFFF };

                self.set_sz_flags(result);
                self.set_flag_h((self.a & 0x0F) < (val & 0x0F));
                self.set_flag_n(true);
                self.set_flag_pv(self.bc != 0);
                let n = result.wrapping_sub(if self.flag_h() { 1 } else { 0 });
                self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);
                bus.add_cycles(1); // CEmu: cpu.cycles += internalCycles (1 for CPD)
                16
            }
            // CPIR - Compare, increment, repeat
            // Executes all iterations in a single instruction to match CEmu behavior
            // Stops when BC=0 or when A matches the memory byte (result=0)
            // CEmu: REG_WRITE_EX(HL, r->HL, cpu_mask_mode(r->HL + delta, cpu.L))
            (6, 1) => {
                let mut cycles = 0u32;
                loop {
                    let val = bus.read_byte(self.mask_addr(self.hl));
                    let result = self.a.wrapping_sub(val);
                    self.hl = self.wrap_data(self.hl.wrapping_add(1));
                    // BC is a counter - use L mode for masking
                    self.bc = self.bc.wrapping_sub(1) & if self.l { 0xFFFFFF } else { 0xFFFF };

                    self.set_sz_flags(result);
                    self.set_flag_h((self.a & 0x0F) < (val & 0x0F));
                    self.set_flag_n(true);
                    self.set_flag_pv(self.bc != 0);
                    let n = result.wrapping_sub(if self.flag_h() { 1 } else { 0 });
                    self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);

                    if self.bc != 0 && result != 0 {
                        // CEmu: cpu.cycles += internalCycles (2 for CPIR when repeating)
                        bus.add_cycles(2);
                        cycles += 21;
                        // Continue looping internally
                    } else {
                        // CEmu: internalCycles-- when not repeating, so 1 cycle
                        bus.add_cycles(1);
                        cycles += 16;
                        break;
                    }
                }
                cycles
            }
            // CPDR - Compare, decrement, repeat
            // Executes all iterations in a single instruction to match CEmu behavior
            // Stops when BC=0 or when A matches the memory byte (result=0)
            // CEmu: REG_WRITE_EX(HL, r->HL, cpu_mask_mode(r->HL + delta, cpu.L))
            (7, 1) => {
                let mut cycles = 0u32;
                loop {
                    let val = bus.read_byte(self.mask_addr(self.hl));
                    let result = self.a.wrapping_sub(val);
                    self.hl = self.wrap_data(self.hl.wrapping_sub(1));
                    // BC is a counter - use L mode for masking
                    self.bc = self.bc.wrapping_sub(1) & if self.l { 0xFFFFFF } else { 0xFFFF };

                    self.set_sz_flags(result);
                    self.set_flag_h((self.a & 0x0F) < (val & 0x0F));
                    self.set_flag_n(true);
                    self.set_flag_pv(self.bc != 0);
                    let n = result.wrapping_sub(if self.flag_h() { 1 } else { 0 });
                    self.f = (self.f & !(flags::F5 | flags::F3)) | ((n & 0x02) << 4) | (n & 0x08);

                    if self.bc != 0 && result != 0 {
                        // CEmu: cpu.cycles += internalCycles (2 for CPDR when repeating)
                        bus.add_cycles(2);
                        cycles += 21;
                        // Continue looping internally
                    } else {
                        // CEmu: internalCycles-- when not repeating, so 1 cycle
                        bus.add_cycles(1);
                        cycles += 16;
                        break;
                    }
                }
                cycles
            }
            // INI, IND, INIR, INDR - I/O blocked on TI-84 CE
            (4, 2) | (5, 2) | (6, 2) | (7, 2) => 16,
            // OUTI, OUTD, OTIR, OTDR - I/O blocked on TI-84 CE
            (4, 3) | (5, 3) | (6, 3) | (7, 3) => 16,
            _ => 8,
        }
    }

    /// Execute eZ80 extended block I/O instructions (ED prefix, x=2, y<4)
    /// These include OTIM, OTDM, OTIMR, OTDMR, INIM, INDM, INIMR, INDMR
    /// I/O is blocked on TI-84 CE, but registers still update
    ///
    /// Note: Repeat variants (INIMR, INDMR, OTIMR, OTDMR) execute all iterations
    /// in a single instruction, matching CEmu behavior. This is different from the
    /// standard Z80 block instructions which use PC rewind.
    pub fn execute_bli_ez80(&mut self, bus: &mut Bus, _y: u8, z: u8, p: u8, q: u8) -> u32 {
        // delta = q ? -1 : 1
        let delta: i32 = if q != 0 { -1 } else { 1 };
        let is_repeat = p == 1;

        match (z, p, q) {
            // z=2: Input instructions (INIM, INDM, INIMR, INDMR)
            // These read from port C, write to (HL), modify BC, and HL
            // CEmu: REG_WRITE_EX(HL, r->HL, cpu_mask_mode(r->HL + delta, cpu.L))
            (2, 0, _) | (2, 1, _) => {
                let mut cycles = 0u32;
                loop {
                    // INIM/INDM (p=0) or INIMR/INDMR (p=1)
                    // Read from port - blocked on TI-84 CE, returns 0xFF
                    let val = 0xFF;
                    bus.write_byte(self.mask_addr(self.hl), val);
                    self.hl = self.wrap_data((self.hl as i32 + delta) as u32);
                    // Update C by delta
                    let c = (self.c() as i32 + delta) as u8;
                    self.set_c(c);
                    // Decrement B
                    let old_b = self.b();
                    let new_b = old_b.wrapping_sub(1);
                    self.set_b(new_b);

                    // Set flags
                    self.set_sz_flags(new_b);
                    self.set_flag_h((old_b & 0x0F) == 0);
                    self.set_flag_n(val & 0x80 != 0);

                    if is_repeat && new_b != 0 {
                        cycles += 21;
                        // Continue looping internally
                    } else {
                        cycles += 16;
                        break;
                    }
                }
                cycles
            }
            // z=3: Output instructions (OTIM, OTDM, OTIMR, OTDMR)
            // These read from (HL), write to port C, modify BC, and HL
            // CEmu: REG_WRITE_EX(HL, r->HL, cpu_mask_mode(r->HL + delta, cpu.L))
            (3, 0, _) | (3, 1, _) => {
                let mut cycles = 0u32;
                loop {
                    // OTIM/OTDM (p=0) or OTIMR/OTDMR (p=1)
                    // Read from memory
                    let val = bus.read_byte(self.mask_addr(self.hl));
                    // Output to port - blocked on TI-84 CE, ignored
                    self.hl = self.wrap_data((self.hl as i32 + delta) as u32);
                    // Update C by delta
                    let c = (self.c() as i32 + delta) as u8;
                    self.set_c(c);
                    // Decrement B
                    let old_b = self.b();
                    let new_b = old_b.wrapping_sub(1);
                    self.set_b(new_b);

                    // Set flags
                    self.set_sz_flags(new_b);
                    self.set_flag_h((old_b & 0x0F) == 0);
                    self.set_flag_n(val & 0x80 != 0);

                    if is_repeat && new_b != 0 {
                        cycles += 21;
                        // Continue looping internally
                    } else {
                        cycles += 16;
                        break;
                    }
                }
                cycles
            }
            _ => 8, // Unknown eZ80 BLI opcode
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
                    bus.add_cycles(1); // CEmu: cpu.cycles++ for HALT
                    self.halted = true;
                    4
                } else if y == 6 {
                    // LD (IX+d), r - write register to indexed memory
                    // IMPORTANT: Source register r is NOT substituted (use original H/L)
                    let src = self.get_reg8(z, bus);
                    let d = self.fetch_byte(bus) as i8;
                    let index_reg = if use_ix { self.ix } else { self.iy };
                    let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
                    bus.write_byte(addr, src);
                    19
                } else if z == 6 {
                    // LD r, (IX+d) - read indexed memory into register
                    // IMPORTANT: Destination register r is NOT substituted (use original H/L)
                    let d = self.fetch_byte(bus) as i8;
                    let index_reg = if use_ix { self.ix } else { self.iy };
                    let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
                    let val = bus.read_byte(addr);
                    self.set_reg8(y, val, bus);
                    19
                } else {
                    // LD r,r' with H/L -> IXH/IXL substitution
                    // (no memory operand, so H/L are substituted)
                    let src = self.get_index_reg8(z, bus, use_ix);
                    self.set_index_reg8(y, src, bus, use_ix);
                    8
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
                            bus.add_cycles(1); // CEmu: cpu.cycles++ for branch taken
                            let target = self.wrap_pc((self.pc as i32 + d as i32) as u32);
                            self.prefetch(bus, target); // CEmu: cpu_prefetch(target)
                            self.pc = target;
                            13
                        } else {
                            8
                        }
                    }
                    3 => {
                        // JR d (unconditional)
                        let d = self.fetch_byte(bus) as i8;
                        let target = self.wrap_pc((self.pc as i32 + d as i32) as u32);
                        self.prefetch(bus, target); // CEmu: cpu_prefetch(target)
                        self.pc = target;
                        12
                    }
                    4..=7 => {
                        // JR cc,d
                        let d = self.fetch_byte(bus) as i8;
                        if self.check_cc(y - 4) {
                            bus.add_cycles(1); // CEmu: cpu.cycles++ for branch taken
                            let target = self.wrap_pc((self.pc as i32 + d as i32) as u32);
                            self.prefetch(bus, target); // CEmu: cpu_prefetch(target)
                            self.pc = target;
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
                    } else if y == 6 {
                        // eZ80: LD IY/IX,(IX/IY+d) when prefixed with DD/FD and opcode is 0x31
                        let d = self.fetch_byte(bus) as i8;
                        let index_reg = if use_ix { self.ix } else { self.iy };
                        let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
                        let val = if self.l {
                            bus.read_addr24(addr)
                        } else {
                            bus.read_word(addr) as u32
                        };
                        let masked = if self.l { val & 0xFFFFFF } else { val & 0xFFFF };
                        if use_ix {
                            self.iy = masked;
                        } else {
                            self.ix = masked;
                        }
                        19
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
                        self.set_flag_c(result > if self.l { 0xFFFFFF } else { 0xFFFF });
                        // S, Z, PV, F3, F5 preserved from previous F (CEmu behavior)

                        // CEmu: cpu_write_index applies cpu_mask_mode(value, cpu.L)
                        let wrapped = self.wrap_data(result);
                        if use_ix {
                            self.ix = wrapped;
                        } else {
                            self.iy = wrapped;
                        }
                        15
                    } else {
                        // ADD IX/IY,rp (for BC/DE/SP)
                        let index_reg = if use_ix { self.ix } else { self.iy };
                        let rp = self.get_rp(p);
                        let result = index_reg.wrapping_add(rp);

                        let half = ((index_reg & 0xFFF) + (rp & 0xFFF)) > 0xFFF;
                        self.set_flag_h(half);
                        self.set_flag_n(false);
                        self.set_flag_c(result > if self.l { 0xFFFFFF } else { 0xFFFF });
                        // S, Z, PV, F3, F5 preserved from previous F (CEmu behavior)

                        // CEmu: cpu_write_index applies cpu_mask_mode(value, cpu.L)
                        let wrapped = self.wrap_data(result);
                        if use_ix {
                            self.ix = wrapped;
                        } else {
                            self.iy = wrapped;
                        }
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
                        if self.l {
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
                        let val = if self.l {
                            bus.read_addr24(nn)
                        } else {
                            bus.read_word(nn) as u32
                        };
                        if use_ix {
                            self.ix = val;
                        } else {
                            self.iy = val;
                        }
                        if self.l {
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
                        // INC IX/IY - CEmu uses cpu_mask_mode with L mode
                        if use_ix {
                            self.ix = self.wrap_data(self.ix.wrapping_add(1));
                        } else {
                            self.iy = self.wrap_data(self.iy.wrapping_add(1));
                        }
                        10
                    } else {
                        // DEC IX/IY - CEmu uses cpu_mask_mode with L mode
                        if use_ix {
                            self.ix = self.wrap_data(self.ix.wrapping_sub(1));
                        } else {
                            self.iy = self.wrap_data(self.iy.wrapping_sub(1));
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
                    bus.add_cycles(1); // CEmu: cpu.cycles += context.y == 6
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
                    bus.add_cycles(1); // CEmu: cpu.cycles += context.y == 6
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
                // eZ80: y < 4 is undefined (trap), y=7 is LD (IX/IY+d),IY/IX
                if y < 4 {
                    // eZ80: DD/FD 06/0E/16/1E nn are undefined - treat as NOP
                    // CEmu calls cpu_trap() which typically just continues
                    self.fetch_byte(bus); // consume the immediate
                    8
                } else if y == 7 {
                    // LD (IX+d),IY or LD (IY+d),IX - stores the OTHER index register
                    let d = self.fetch_byte(bus) as i8;
                    let index_reg = if use_ix { self.ix } else { self.iy };
                    let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
                    let other_reg = if use_ix { self.iy } else { self.ix };
                    if self.l {
                        bus.write_addr24(addr, other_reg);
                    } else {
                        bus.write_word(addr, other_reg as u16);
                    }
                    19
                } else if y == 6 {
                    // LD (IX+d),n - displacement before immediate
                    let d = self.fetch_byte(bus) as i8;
                    let n = self.fetch_byte(bus);
                    let index_reg = if use_ix { self.ix } else { self.iy };
                    let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
                    bus.write_byte(addr, n);
                    19
                } else {
                    // y=4,5: LD IXH/IXL,n or LD IYH/IYL,n
                    let n = self.fetch_byte(bus);
                    self.set_index_reg8_no_disp(y, n, use_ix);
                    11
                }
            }
            7 => {
                // eZ80: DD/FD prefix transforms z=7 into LD rp3,(IX/IY+d) / LD (IX/IY+d),rp3
                let d = self.fetch_byte(bus) as i8;
                let index_reg = if use_ix { self.ix } else { self.iy };
                let addr = self.mask_addr((index_reg as i32 + d as i32) as u32);
                let mask = if self.l { 0xFFFFFF } else { 0xFFFF };

                if q == 0 {
                    // LD rp3[p],(IX/IY+d)
                    let val = if self.l {
                        bus.read_addr24(addr)
                    } else {
                        bus.read_word(addr) as u32
                    } & mask;
                    match p {
                        0 => self.bc = val,
                        1 => self.de = val,
                        2 => self.hl = val,
                        3 => {
                            if use_ix {
                                self.ix = val;
                            } else {
                                self.iy = val;
                            }
                        }
                        _ => {}
                    }
                } else {
                    // LD (IX/IY+d),rp3[p]
                    let rp3_val = match p {
                        0 => self.bc,
                        1 => self.de,
                        2 => self.hl,
                        3 => index_reg,
                        _ => 0,
                    } & mask;
                    if self.l {
                        bus.write_addr24(addr, rp3_val);
                    } else {
                        bus.write_word(addr, rp3_val as u16);
                    }
                }
                19
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
                    self.adl = self.l;
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
                        let val = self.pop_addr(bus);
                        if use_ix {
                            self.ix = val;
                        } else {
                            self.iy = val;
                        }
                        14
                    } else if p == 3 {
                        // AF - CEmu pops 3 bytes in ADL mode (upper byte discarded)
                        let val = self.pop_addr(bus);
                        self.a = (val >> 8) as u8;
                        self.f = val as u8;
                        10
                    } else {
                        let val = self.pop_addr(bus);
                        self.set_rp(p, val);
                        10
                    }
                } else {
                    match p {
                        0 => {
                            // RET
                            self.pc = self.pop_addr(bus);
                            self.adl = self.l;
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
                            self.adl = self.l;
                            self.pc = self.wrap_pc(index_reg);
                            8
                        }
                        3 => {
                            // LD SP,IX/IY
                            let index_reg = if use_ix { self.ix } else { self.iy };
                            self.sp = self.wrap_data(index_reg);
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
                    self.adl = self.il;
                }
                10
            }
            3 => {
                match y {
                    0 => {
                        // JP nn - not affected
                        self.pc = self.fetch_addr(bus);
                        self.adl = self.il;
                        10
                    }
                    1 => {
                        // DD CB / FD CB - handled at top of execute_index
                        8
                    }
                    4 => {
                        // EX (SP),IX/IY
                        let sp_addr = self.mask_addr(self.sp);
                        let sp_val = if self.l {
                            bus.read_addr24(sp_addr)
                        } else {
                            bus.read_word(sp_addr) as u32
                        };
                        let index_reg = if use_ix { self.ix } else { self.iy };
                        if self.l {
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
                    self.adl = self.il;
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
                        self.push_addr(bus, index_reg);
                        15
                    } else if p == 3 {
                        // AF - CEmu pushes 3 bytes in ADL mode (upper byte is 0)
                        let val = ((self.a as u32) << 8) | (self.f as u32);
                        self.push_addr(bus, val);
                        11
                    } else {
                        let val = self.get_rp(p);
                        self.push_addr(bus, val);
                        11
                    }
                } else {
                    match p {
                        0 => {
                            // CALL nn - not affected
                            let nn = self.fetch_addr(bus);
                            self.push_addr(bus, self.pc);
                            self.pc = nn;
                            self.adl = self.il;
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
                bus.add_cycles(1); // CEmu: cpu.cycles++ in cpu_rst()
                self.push_addr(bus, self.pc);
                self.adl = self.l;
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
                // CEmu: cpu.cycles += z == 6 in cpu_execute_rot (z is always 6 for indexed)
                bus.add_cycles(1);
                let result = self.execute_rot(y, val);
                bus.write_byte(addr, result);
                // If z != 6, also copy to register (undocumented)
                if z != 6 {
                    self.set_reg8(z, result, bus);
                }
                23
            }
            1 => {
                // BIT y,(IX+d)/(IY+d) - no extra cycle
                let mask = 1 << y;
                let result = val & mask;

                // Preserve carry and undocumented F3/F5 bits (CEmu behavior)
                self.f &= flags::C | flags::F5 | flags::F3;
                self.set_flag_z(result == 0);
                self.set_flag_h(true);
                self.set_flag_n(false);
                self.set_flag_pv(result == 0);
                if y == 7 && result != 0 {
                    self.f |= flags::S;
                }
                20
            }
            2 => {
                // RES y,(IX+d)/(IY+d), optionally copy to register
                // CEmu: cpu.cycles += context.z == 6 (z is always 6 for indexed)
                bus.add_cycles(1);
                let result = val & !(1 << y);
                bus.write_byte(addr, result);
                if z != 6 {
                    self.set_reg8(z, result, bus);
                }
                23
            }
            3 => {
                // SET y,(IX+d)/(IY+d), optionally copy to register
                // CEmu: cpu.cycles += context.z == 6 (z is always 6 for indexed)
                bus.add_cycles(1);
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
