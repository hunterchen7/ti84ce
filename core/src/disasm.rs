//! eZ80 Disassembler
//!
//! Provides instruction disassembly for trace comparison and debugging.
//! Handles all eZ80 prefix combinations and addressing modes.

/// Result of disassembling an instruction
#[derive(Debug, Clone)]
pub struct DisasmResult {
    /// Raw opcode bytes as hex string (e.g., "DD 7E 05")
    pub bytes: String,
    /// Mnemonic with operands (e.g., "LD A,(IX+5)")
    pub mnemonic: String,
    /// Length of the instruction in bytes
    pub length: usize,
}

/// Disassemble an eZ80 instruction
///
/// # Arguments
/// * `opcode` - Slice of opcode bytes (at least 4 bytes recommended)
/// * `adl` - Current ADL mode (affects address sizes)
///
/// # Returns
/// DisasmResult with instruction details
pub fn disassemble(opcode: &[u8], adl: bool) -> DisasmResult {
    if opcode.is_empty() {
        return DisasmResult {
            bytes: String::new(),
            mnemonic: "???".to_string(),
            length: 0,
        };
    }

    let (mnemonic, length) = disasm_main(opcode, adl);
    let bytes = opcode[..length.min(opcode.len())]
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ");

    DisasmResult {
        bytes,
        mnemonic,
        length,
    }
}

/// Main disassembly dispatcher
fn disasm_main(opcode: &[u8], adl: bool) -> (String, usize) {
    let op = opcode[0];

    match op {
        // Suffix opcodes (.SIS, .LIS, .SIL, .LIL)
        0x40 => {
            // .SIS suffix
            if opcode.len() > 1 {
                let (inner, inner_len) = disasm_with_suffix(opcode, 1, false, false);
                (format!("{}.SIS", inner), 1 + inner_len)
            } else {
                ("NOP.SIS".to_string(), 1)
            }
        }
        0x49 => {
            // .LIS suffix
            if opcode.len() > 1 {
                let (inner, inner_len) = disasm_with_suffix(opcode, 1, true, false);
                (format!("{}.LIS", inner), 1 + inner_len)
            } else {
                ("NOP.LIS".to_string(), 1)
            }
        }
        0x52 => {
            // .SIL suffix
            if opcode.len() > 1 {
                let (inner, inner_len) = disasm_with_suffix(opcode, 1, false, true);
                (format!("{}.SIL", inner), 1 + inner_len)
            } else {
                ("NOP.SIL".to_string(), 1)
            }
        }
        0x5B => {
            // .LIL suffix
            if opcode.len() > 1 {
                let (inner, inner_len) = disasm_with_suffix(opcode, 1, true, true);
                (format!("{}.LIL", inner), 1 + inner_len)
            } else {
                ("NOP.LIL".to_string(), 1)
            }
        }
        // Index register prefixes
        0xDD => disasm_dd(opcode, adl),
        0xFD => disasm_fd(opcode, adl),
        // Extended instructions
        0xED => disasm_ed(opcode, adl),
        // CB prefix (bit operations)
        0xCB => disasm_cb(opcode, 1),
        // Main instructions
        _ => disasm_unprefixed(opcode, adl),
    }
}

/// Disassemble with an explicit suffix (handles next byte after suffix)
fn disasm_with_suffix(opcode: &[u8], offset: usize, il: bool, l: bool) -> (String, usize) {
    if offset >= opcode.len() {
        return ("???".to_string(), 0);
    }

    let op = opcode[offset];
    match op {
        0xDD => {
            let (inner, len) = disasm_dd_inner(&opcode[offset..], il, l);
            (inner, len)
        }
        0xFD => {
            let (inner, len) = disasm_fd_inner(&opcode[offset..], il, l);
            (inner, len)
        }
        0xED => {
            let (inner, len) = disasm_ed_inner(&opcode[offset..], il, l);
            (inner, len)
        }
        0xCB => {
            let (inner, len) = disasm_cb(opcode, offset + 1);
            (inner, len)
        }
        _ => {
            // Use modified ADL for regular instructions
            disasm_unprefixed(&opcode[offset..], l)
        }
    }
}

/// DD prefix (IX instructions)
fn disasm_dd(opcode: &[u8], adl: bool) -> (String, usize) {
    disasm_dd_inner(opcode, adl, adl)
}

fn disasm_dd_inner(opcode: &[u8], il: bool, l: bool) -> (String, usize) {
    if opcode.len() < 2 {
        return ("DB DDh".to_string(), 1);
    }

    let op = opcode[1];
    match op {
        0xCB => disasm_ddcb(opcode, il, l),
        0x21 => {
            // LD IX,nn
            let (val, size) = read_imm_word(&opcode[2..], l);
            (format!("LD IX,{}", val), 2 + size)
        }
        0x22 => {
            // LD (nn),IX
            let (addr, size) = read_imm_addr(&opcode[2..], il);
            (format!("LD ({}),IX", addr), 2 + size)
        }
        0x23 => ("INC IX".to_string(), 2),
        0x24 => ("INC IXH".to_string(), 2),
        0x25 => ("DEC IXH".to_string(), 2),
        0x26 => {
            // LD IXH,n
            if opcode.len() >= 3 {
                (format!("LD IXH,0x{:02X}", opcode[2]), 3)
            } else {
                ("LD IXH,?".to_string(), 2)
            }
        }
        0x29 => ("ADD IX,IX".to_string(), 2),
        0x2A => {
            // LD IX,(nn)
            let (addr, size) = read_imm_addr(&opcode[2..], il);
            (format!("LD IX,({})", addr), 2 + size)
        }
        0x2B => ("DEC IX".to_string(), 2),
        0x2C => ("INC IXL".to_string(), 2),
        0x2D => ("DEC IXL".to_string(), 2),
        0x2E => {
            // LD IXL,n
            if opcode.len() >= 3 {
                (format!("LD IXL,0x{:02X}", opcode[2]), 3)
            } else {
                ("LD IXL,?".to_string(), 2)
            }
        }
        0x31 => {
            // eZ80: LD IX,(IX+d)
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("LD IX,(IX{:+})", d), 3)
            } else {
                ("LD IX,(IX+?)".to_string(), 2)
            }
        }
        0x34 => {
            // INC (IX+d)
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("INC (IX{:+})", d), 3)
            } else {
                ("INC (IX+?)".to_string(), 2)
            }
        }
        0x35 => {
            // DEC (IX+d)
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("DEC (IX{:+})", d), 3)
            } else {
                ("DEC (IX+?)".to_string(), 2)
            }
        }
        0x36 => {
            // LD (IX+d),n
            if opcode.len() >= 4 {
                let d = opcode[2] as i8;
                (format!("LD (IX{:+}),0x{:02X}", d, opcode[3]), 4)
            } else {
                ("LD (IX+?),?".to_string(), 2)
            }
        }
        0x37 => {
            // eZ80: LD IX,(IX+d) long form
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("LD IX,(IX{:+})", d), 3)
            } else {
                ("LD IX,(IX+?)".to_string(), 2)
            }
        }
        0x3E => {
            // eZ80: LD (IX+d),IX
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("LD (IX{:+}),IX", d), 3)
            } else {
                ("LD (IX+?),IX".to_string(), 2)
            }
        }
        0xE1 => ("POP IX".to_string(), 2),
        0xE3 => ("EX (SP),IX".to_string(), 2),
        0xE5 => ("PUSH IX".to_string(), 2),
        0xE9 => ("JP (IX)".to_string(), 2),
        0xF9 => ("LD SP,IX".to_string(), 2),
        // 8-bit loads with IX+d
        op if op & 0xC0 == 0x40 => {
            // LD r,(IX+d) or LD (IX+d),r or LD IXH/IXL,r
            disasm_dd_ld(opcode, op)
        }
        // Arithmetic with IX+d
        op if op >= 0x80 && op <= 0xBF => {
            disasm_dd_alu(opcode, op)
        }
        _ => (format!("DB DDh,{:02X}h", op), 2),
    }
}

/// DD prefix 8-bit load instructions
fn disasm_dd_ld(opcode: &[u8], op: u8) -> (String, usize) {
    let dst = (op >> 3) & 7;
    let src = op & 7;

    // Check for (IX+d) operand
    if src == 6 || dst == 6 {
        // Involves (IX+d)
        if opcode.len() >= 3 {
            let d = opcode[2] as i8;
            let dst_str = reg8_name_ix(dst);
            let src_str = if src == 6 {
                format!("(IX{:+})", d)
            } else {
                reg8_name_ix(src).to_string()
            };
            let dst_str = if dst == 6 {
                format!("(IX{:+})", d)
            } else {
                dst_str.to_string()
            };
            (format!("LD {},{}", dst_str, src_str), 3)
        } else {
            ("LD ?,?".to_string(), 2)
        }
    } else {
        // IXH/IXL operations
        let dst_str = reg8_name_ix(dst);
        let src_str = reg8_name_ix(src);
        (format!("LD {},{}", dst_str, src_str), 2)
    }
}

/// DD prefix ALU instructions
fn disasm_dd_alu(opcode: &[u8], op: u8) -> (String, usize) {
    let alu_op = (op >> 3) & 7;
    let operand = op & 7;

    if operand == 6 {
        // ALU A,(IX+d)
        if opcode.len() >= 3 {
            let d = opcode[2] as i8;
            let alu_name = alu_name(alu_op);
            (format!("{} (IX{:+})", alu_name, d), 3)
        } else {
            (format!("{} (IX+?)", alu_name(alu_op)), 2)
        }
    } else {
        // ALU A,IXH/IXL
        let alu_name = alu_name(alu_op);
        let reg = reg8_name_ix(operand);
        (format!("{} {}", alu_name, reg), 2)
    }
}

/// FD prefix (IY instructions)
fn disasm_fd(opcode: &[u8], adl: bool) -> (String, usize) {
    disasm_fd_inner(opcode, adl, adl)
}

fn disasm_fd_inner(opcode: &[u8], il: bool, l: bool) -> (String, usize) {
    if opcode.len() < 2 {
        return ("DB FDh".to_string(), 1);
    }

    let op = opcode[1];
    match op {
        0xCB => disasm_fdcb(opcode, il, l),
        0x21 => {
            let (val, size) = read_imm_word(&opcode[2..], l);
            (format!("LD IY,{}", val), 2 + size)
        }
        0x22 => {
            let (addr, size) = read_imm_addr(&opcode[2..], il);
            (format!("LD ({}),IY", addr), 2 + size)
        }
        0x23 => ("INC IY".to_string(), 2),
        0x24 => ("INC IYH".to_string(), 2),
        0x25 => ("DEC IYH".to_string(), 2),
        0x26 => {
            if opcode.len() >= 3 {
                (format!("LD IYH,0x{:02X}", opcode[2]), 3)
            } else {
                ("LD IYH,?".to_string(), 2)
            }
        }
        0x29 => ("ADD IY,IY".to_string(), 2),
        0x2A => {
            let (addr, size) = read_imm_addr(&opcode[2..], il);
            (format!("LD IY,({})", addr), 2 + size)
        }
        0x2B => ("DEC IY".to_string(), 2),
        0x2C => ("INC IYL".to_string(), 2),
        0x2D => ("DEC IYL".to_string(), 2),
        0x2E => {
            if opcode.len() >= 3 {
                (format!("LD IYL,0x{:02X}", opcode[2]), 3)
            } else {
                ("LD IYL,?".to_string(), 2)
            }
        }
        0x31 => {
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("LD IY,(IY{:+})", d), 3)
            } else {
                ("LD IY,(IY+?)".to_string(), 2)
            }
        }
        0x34 => {
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("INC (IY{:+})", d), 3)
            } else {
                ("INC (IY+?)".to_string(), 2)
            }
        }
        0x35 => {
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("DEC (IY{:+})", d), 3)
            } else {
                ("DEC (IY+?)".to_string(), 2)
            }
        }
        0x36 => {
            if opcode.len() >= 4 {
                let d = opcode[2] as i8;
                (format!("LD (IY{:+}),0x{:02X}", d, opcode[3]), 4)
            } else {
                ("LD (IY+?),?".to_string(), 2)
            }
        }
        0x3E => {
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("LD (IY{:+}),IY", d), 3)
            } else {
                ("LD (IY+?),IY".to_string(), 2)
            }
        }
        0xE1 => ("POP IY".to_string(), 2),
        0xE3 => ("EX (SP),IY".to_string(), 2),
        0xE5 => ("PUSH IY".to_string(), 2),
        0xE9 => ("JP (IY)".to_string(), 2),
        0xF9 => ("LD SP,IY".to_string(), 2),
        // 8-bit loads with IY+d
        op if op & 0xC0 == 0x40 => disasm_fd_ld(opcode, op),
        // Arithmetic with IY+d
        op if op >= 0x80 && op <= 0xBF => disasm_fd_alu(opcode, op),
        _ => (format!("DB FDh,{:02X}h", op), 2),
    }
}

/// FD prefix 8-bit load instructions
fn disasm_fd_ld(opcode: &[u8], op: u8) -> (String, usize) {
    let dst = (op >> 3) & 7;
    let src = op & 7;

    if src == 6 || dst == 6 {
        if opcode.len() >= 3 {
            let d = opcode[2] as i8;
            let dst_str = reg8_name_iy(dst);
            let src_str = if src == 6 {
                format!("(IY{:+})", d)
            } else {
                reg8_name_iy(src).to_string()
            };
            let dst_str = if dst == 6 {
                format!("(IY{:+})", d)
            } else {
                dst_str.to_string()
            };
            (format!("LD {},{}", dst_str, src_str), 3)
        } else {
            ("LD ?,?".to_string(), 2)
        }
    } else {
        let dst_str = reg8_name_iy(dst);
        let src_str = reg8_name_iy(src);
        (format!("LD {},{}", dst_str, src_str), 2)
    }
}

/// FD prefix ALU instructions
fn disasm_fd_alu(opcode: &[u8], op: u8) -> (String, usize) {
    let alu_op = (op >> 3) & 7;
    let operand = op & 7;

    if operand == 6 {
        if opcode.len() >= 3 {
            let d = opcode[2] as i8;
            let alu_name = alu_name(alu_op);
            (format!("{} (IY{:+})", alu_name, d), 3)
        } else {
            (format!("{} (IY+?)", alu_name(alu_op)), 2)
        }
    } else {
        let alu_name = alu_name(alu_op);
        let reg = reg8_name_iy(operand);
        (format!("{} {}", alu_name, reg), 2)
    }
}

/// ED prefix instructions
fn disasm_ed(opcode: &[u8], adl: bool) -> (String, usize) {
    disasm_ed_inner(opcode, adl, adl)
}

fn disasm_ed_inner(opcode: &[u8], il: bool, _l: bool) -> (String, usize) {
    if opcode.len() < 2 {
        return ("DB EDh".to_string(), 1);
    }

    let op = opcode[1];
    match op {
        // IN r,(C) / OUT (C),r
        0x40 => ("IN B,(C)".to_string(), 2),
        0x41 => ("OUT (C),B".to_string(), 2),
        0x48 => ("IN C,(C)".to_string(), 2),
        0x49 => ("OUT (C),C".to_string(), 2),
        0x50 => ("IN D,(C)".to_string(), 2),
        0x51 => ("OUT (C),D".to_string(), 2),
        0x58 => ("IN E,(C)".to_string(), 2),
        0x59 => ("OUT (C),E".to_string(), 2),
        0x60 => ("IN H,(C)".to_string(), 2),
        0x61 => ("OUT (C),H".to_string(), 2),
        0x68 => ("IN L,(C)".to_string(), 2),
        0x69 => ("OUT (C),L".to_string(), 2),
        0x70 => ("IN F,(C)".to_string(), 2),
        0x71 => ("OUT (C),0".to_string(), 2),
        0x78 => ("IN A,(C)".to_string(), 2),
        0x79 => ("OUT (C),A".to_string(), 2),

        // SBC/ADC HL,rr
        0x42 => ("SBC HL,BC".to_string(), 2),
        0x4A => ("ADC HL,BC".to_string(), 2),
        0x52 => ("SBC HL,DE".to_string(), 2),
        0x5A => ("ADC HL,DE".to_string(), 2),
        0x62 => ("SBC HL,HL".to_string(), 2),
        0x6A => ("ADC HL,HL".to_string(), 2),
        0x72 => ("SBC HL,SP".to_string(), 2),
        0x7A => ("ADC HL,SP".to_string(), 2),

        // LD (nn),rr / LD rr,(nn)
        0x43 => {
            let (addr, size) = read_imm_addr(&opcode[2..], il);
            (format!("LD ({}),BC", addr), 2 + size)
        }
        0x4B => {
            let (addr, size) = read_imm_addr(&opcode[2..], il);
            (format!("LD BC,({})", addr), 2 + size)
        }
        0x53 => {
            let (addr, size) = read_imm_addr(&opcode[2..], il);
            (format!("LD ({}),DE", addr), 2 + size)
        }
        0x5B => {
            let (addr, size) = read_imm_addr(&opcode[2..], il);
            (format!("LD DE,({})", addr), 2 + size)
        }
        0x63 => {
            let (addr, size) = read_imm_addr(&opcode[2..], il);
            (format!("LD ({}),HL", addr), 2 + size)
        }
        0x6B => {
            let (addr, size) = read_imm_addr(&opcode[2..], il);
            (format!("LD HL,({})", addr), 2 + size)
        }
        0x73 => {
            let (addr, size) = read_imm_addr(&opcode[2..], il);
            (format!("LD ({}),SP", addr), 2 + size)
        }
        0x7B => {
            let (addr, size) = read_imm_addr(&opcode[2..], il);
            (format!("LD SP,({})", addr), 2 + size)
        }

        // NEG - note: some opcodes have different meanings in eZ80
        // 0x4C, 0x5C, 0x6C, 0x7C are MLT in eZ80
        // 0x74 is TSTIO in eZ80
        0x44 | 0x54 | 0x64 => ("NEG".to_string(), 2),

        // MLT instructions (eZ80-specific, these are NEG aliases on Z80)
        0x4C => ("MLT BC".to_string(), 2),
        0x5C => ("MLT DE".to_string(), 2),
        0x6C => ("MLT HL".to_string(), 2),
        0x7C => ("MLT SP".to_string(), 2),

        // RETN/RETI - note: 0x6D is LD MB,A in eZ80, 0x7D is STMIX
        0x45 => ("RETN".to_string(), 2),
        0x4D => ("RETI".to_string(), 2),
        0x55 | 0x5D | 0x75 => ("RETN".to_string(), 2), // alternate RETN encodings

        // IM modes - note: some alternate encodings have different meanings in eZ80
        // 0x66 is PEA IY+d, 0x6E is LD A,MB, 0x76 is SLP
        0x46 | 0x4E => ("IM 0".to_string(), 2),
        0x56 => ("IM 1".to_string(), 2),
        0x5E | 0x7E => ("IM 2".to_string(), 2),

        // Special registers
        0x47 => ("LD I,A".to_string(), 2),
        0x4F => ("LD R,A".to_string(), 2),
        0x57 => ("LD A,I".to_string(), 2),
        0x5F => ("LD A,R".to_string(), 2),

        // RRD/RLD
        0x67 => ("RRD".to_string(), 2),
        0x6F => ("RLD".to_string(), 2),

        // eZ80: LD MB,A / LD A,MB
        0x6D => ("LD MB,A".to_string(), 2),
        0x6E => ("LD A,MB".to_string(), 2),

        // Block instructions
        0xA0 => ("LDI".to_string(), 2),
        0xA1 => ("CPI".to_string(), 2),
        0xA2 => ("INI".to_string(), 2),
        0xA3 => ("OUTI".to_string(), 2),
        0xA8 => ("LDD".to_string(), 2),
        0xA9 => ("CPD".to_string(), 2),
        0xAA => ("IND".to_string(), 2),
        0xAB => ("OUTD".to_string(), 2),
        0xB0 => ("LDIR".to_string(), 2),
        0xB1 => ("CPIR".to_string(), 2),
        0xB2 => ("INIR".to_string(), 2),
        0xB3 => ("OTIR".to_string(), 2),
        0xB8 => ("LDDR".to_string(), 2),
        0xB9 => ("CPDR".to_string(), 2),
        0xBA => ("INDR".to_string(), 2),
        0xBB => ("OTDR".to_string(), 2),

        // eZ80: INIRX/OTIRX/INDRX/OTDRX
        0xC2 => ("INIRX".to_string(), 2),
        0xC3 => ("OTIRX".to_string(), 2),
        0xCA => ("INDRX".to_string(), 2),
        0xCB => ("OTDRX".to_string(), 2),

        // IN0/OUT0
        0x38 => {
            // IN0 A,(n)
            if opcode.len() >= 3 {
                (format!("IN0 A,(0x{:02X})", opcode[2]), 3)
            } else {
                ("IN0 A,(?)".to_string(), 2)
            }
        }
        0x39 => {
            // OUT0 (n),A
            if opcode.len() >= 3 {
                (format!("OUT0 (0x{:02X}),A", opcode[2]), 3)
            } else {
                ("OUT0 (?),A".to_string(), 2)
            }
        }
        0x00 => {
            // IN0 B,(n)
            if opcode.len() >= 3 {
                (format!("IN0 B,(0x{:02X})", opcode[2]), 3)
            } else {
                ("IN0 B,(?)".to_string(), 2)
            }
        }
        0x01 => {
            // OUT0 (n),B
            if opcode.len() >= 3 {
                (format!("OUT0 (0x{:02X}),B", opcode[2]), 3)
            } else {
                ("OUT0 (?),B".to_string(), 2)
            }
        }
        0x08 => {
            // IN0 C,(n)
            if opcode.len() >= 3 {
                (format!("IN0 C,(0x{:02X})", opcode[2]), 3)
            } else {
                ("IN0 C,(?)".to_string(), 2)
            }
        }
        0x09 => {
            // OUT0 (n),C
            if opcode.len() >= 3 {
                (format!("OUT0 (0x{:02X}),C", opcode[2]), 3)
            } else {
                ("OUT0 (?),C".to_string(), 2)
            }
        }
        0x10 => {
            // IN0 D,(n)
            if opcode.len() >= 3 {
                (format!("IN0 D,(0x{:02X})", opcode[2]), 3)
            } else {
                ("IN0 D,(?)".to_string(), 2)
            }
        }
        0x11 => {
            // OUT0 (n),D
            if opcode.len() >= 3 {
                (format!("OUT0 (0x{:02X}),D", opcode[2]), 3)
            } else {
                ("OUT0 (?),D".to_string(), 2)
            }
        }
        0x18 => {
            // IN0 E,(n)
            if opcode.len() >= 3 {
                (format!("IN0 E,(0x{:02X})", opcode[2]), 3)
            } else {
                ("IN0 E,(?)".to_string(), 2)
            }
        }
        0x19 => {
            // OUT0 (n),E
            if opcode.len() >= 3 {
                (format!("OUT0 (0x{:02X}),E", opcode[2]), 3)
            } else {
                ("OUT0 (?),E".to_string(), 2)
            }
        }
        0x20 => {
            // IN0 H,(n)
            if opcode.len() >= 3 {
                (format!("IN0 H,(0x{:02X})", opcode[2]), 3)
            } else {
                ("IN0 H,(?)".to_string(), 2)
            }
        }
        0x21 => {
            // OUT0 (n),H
            if opcode.len() >= 3 {
                (format!("OUT0 (0x{:02X}),H", opcode[2]), 3)
            } else {
                ("OUT0 (?),H".to_string(), 2)
            }
        }
        0x28 => {
            // IN0 L,(n)
            if opcode.len() >= 3 {
                (format!("IN0 L,(0x{:02X})", opcode[2]), 3)
            } else {
                ("IN0 L,(?)".to_string(), 2)
            }
        }
        0x29 => {
            // OUT0 (n),L
            if opcode.len() >= 3 {
                (format!("OUT0 (0x{:02X}),L", opcode[2]), 3)
            } else {
                ("OUT0 (?),L".to_string(), 2)
            }
        }
        0x30 => {
            // IN0 F,(n) (eZ80)
            if opcode.len() >= 3 {
                (format!("IN0 F,(0x{:02X})", opcode[2]), 3)
            } else {
                ("IN0 F,(?)".to_string(), 2)
            }
        }

        // eZ80: LEA instructions
        0x02 => {
            // LEA BC,IX+d
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("LEA BC,IX{:+}", d), 3)
            } else {
                ("LEA BC,IX+?".to_string(), 2)
            }
        }
        0x03 => {
            // LEA BC,IY+d
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("LEA BC,IY{:+}", d), 3)
            } else {
                ("LEA BC,IY+?".to_string(), 2)
            }
        }
        0x12 => {
            // LEA DE,IX+d
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("LEA DE,IX{:+}", d), 3)
            } else {
                ("LEA DE,IX+?".to_string(), 2)
            }
        }
        0x13 => {
            // LEA DE,IY+d
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("LEA DE,IY{:+}", d), 3)
            } else {
                ("LEA DE,IY+?".to_string(), 2)
            }
        }
        0x22 => {
            // LEA HL,IX+d
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("LEA HL,IX{:+}", d), 3)
            } else {
                ("LEA HL,IX+?".to_string(), 2)
            }
        }
        0x23 => {
            // LEA HL,IY+d
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("LEA HL,IY{:+}", d), 3)
            } else {
                ("LEA HL,IY+?".to_string(), 2)
            }
        }
        0x32 => {
            // LEA IX,IX+d
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("LEA IX,IX{:+}", d), 3)
            } else {
                ("LEA IX,IX+?".to_string(), 2)
            }
        }
        0x33 => {
            // LEA IY,IY+d
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("LEA IY,IY{:+}", d), 3)
            } else {
                ("LEA IY,IY+?".to_string(), 2)
            }
        }

        // eZ80: PEA
        0x65 => {
            // PEA IX+d
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("PEA IX{:+}", d), 3)
            } else {
                ("PEA IX+?".to_string(), 2)
            }
        }
        0x66 => {
            // PEA IY+d
            if opcode.len() >= 3 {
                let d = opcode[2] as i8;
                (format!("PEA IY{:+}", d), 3)
            } else {
                ("PEA IY+?".to_string(), 2)
            }
        }

        // eZ80: TSTIO
        0x74 => {
            if opcode.len() >= 3 {
                (format!("TSTIO 0x{:02X}", opcode[2]), 3)
            } else {
                ("TSTIO ?".to_string(), 2)
            }
        }

        // eZ80: SLP
        0x76 => ("SLP".to_string(), 2),

        // eZ80: STMIX/RSMIX
        0x7D => ("STMIX".to_string(), 2),
        0x7F => ("RSMIX".to_string(), 2),

        _ => (format!("DB EDh,{:02X}h", op), 2),
    }
}

/// CB prefix (bit operations)
fn disasm_cb(opcode: &[u8], offset: usize) -> (String, usize) {
    if offset >= opcode.len() {
        return ("DB CBh".to_string(), offset);
    }

    let op = opcode[offset];
    let reg = op & 7;
    let bit = (op >> 3) & 7;
    let group = op >> 6;
    let reg_name = reg8_name(reg);

    let mnemonic = match group {
        0 => {
            // Rotates and shifts
            let shift_name = match bit {
                0 => "RLC",
                1 => "RRC",
                2 => "RL",
                3 => "RR",
                4 => "SLA",
                5 => "SRA",
                6 => "SLL", // Undocumented
                7 => "SRL",
                _ => unreachable!(),
            };
            format!("{} {}", shift_name, reg_name)
        }
        1 => format!("BIT {},{}", bit, reg_name),
        2 => format!("RES {},{}", bit, reg_name),
        3 => format!("SET {},{}", bit, reg_name),
        _ => unreachable!(),
    };

    (mnemonic, offset + 1)
}

/// DD CB prefix (IX+d bit operations)
fn disasm_ddcb(opcode: &[u8], _il: bool, _l: bool) -> (String, usize) {
    if opcode.len() < 4 {
        return ("DB DDh,CBh".to_string(), 2);
    }

    let d = opcode[2] as i8;
    let op = opcode[3];
    let bit = (op >> 3) & 7;
    let group = op >> 6;
    let reg = op & 7;

    let operand = format!("(IX{:+})", d);

    let mnemonic = match group {
        0 => {
            let shift_name = match bit {
                0 => "RLC",
                1 => "RRC",
                2 => "RL",
                3 => "RR",
                4 => "SLA",
                5 => "SRA",
                6 => "SLL",
                7 => "SRL",
                _ => unreachable!(),
            };
            if reg == 6 {
                format!("{} {}", shift_name, operand)
            } else {
                // Undocumented: also stores result in register
                format!("{} {},{}", shift_name, operand, reg8_name(reg))
            }
        }
        1 => format!("BIT {},{}", bit, operand),
        2 => {
            if reg == 6 {
                format!("RES {},{}", bit, operand)
            } else {
                format!("RES {},{},{}", bit, operand, reg8_name(reg))
            }
        }
        3 => {
            if reg == 6 {
                format!("SET {},{}", bit, operand)
            } else {
                format!("SET {},{},{}", bit, operand, reg8_name(reg))
            }
        }
        _ => unreachable!(),
    };

    (mnemonic, 4)
}

/// FD CB prefix (IY+d bit operations)
fn disasm_fdcb(opcode: &[u8], _il: bool, _l: bool) -> (String, usize) {
    if opcode.len() < 4 {
        return ("DB FDh,CBh".to_string(), 2);
    }

    let d = opcode[2] as i8;
    let op = opcode[3];
    let bit = (op >> 3) & 7;
    let group = op >> 6;
    let reg = op & 7;

    let operand = format!("(IY{:+})", d);

    let mnemonic = match group {
        0 => {
            let shift_name = match bit {
                0 => "RLC",
                1 => "RRC",
                2 => "RL",
                3 => "RR",
                4 => "SLA",
                5 => "SRA",
                6 => "SLL",
                7 => "SRL",
                _ => unreachable!(),
            };
            if reg == 6 {
                format!("{} {}", shift_name, operand)
            } else {
                format!("{} {},{}", shift_name, operand, reg8_name(reg))
            }
        }
        1 => format!("BIT {},{}", bit, operand),
        2 => {
            if reg == 6 {
                format!("RES {},{}", bit, operand)
            } else {
                format!("RES {},{},{}", bit, operand, reg8_name(reg))
            }
        }
        3 => {
            if reg == 6 {
                format!("SET {},{}", bit, operand)
            } else {
                format!("SET {},{},{}", bit, operand, reg8_name(reg))
            }
        }
        _ => unreachable!(),
    };

    (mnemonic, 4)
}

/// Unprefixed instructions
fn disasm_unprefixed(opcode: &[u8], adl: bool) -> (String, usize) {
    let op = opcode[0];

    match op {
        0x00 => ("NOP".to_string(), 1),
        0x01 => {
            let (val, size) = read_imm_word(&opcode[1..], adl);
            (format!("LD BC,{}", val), 1 + size)
        }
        0x02 => ("LD (BC),A".to_string(), 1),
        0x03 => ("INC BC".to_string(), 1),
        0x04 => ("INC B".to_string(), 1),
        0x05 => ("DEC B".to_string(), 1),
        0x06 => {
            if opcode.len() >= 2 {
                (format!("LD B,0x{:02X}", opcode[1]), 2)
            } else {
                ("LD B,?".to_string(), 1)
            }
        }
        0x07 => ("RLCA".to_string(), 1),
        0x08 => ("EX AF,AF'".to_string(), 1),
        0x09 => ("ADD HL,BC".to_string(), 1),
        0x0A => ("LD A,(BC)".to_string(), 1),
        0x0B => ("DEC BC".to_string(), 1),
        0x0C => ("INC C".to_string(), 1),
        0x0D => ("DEC C".to_string(), 1),
        0x0E => {
            if opcode.len() >= 2 {
                (format!("LD C,0x{:02X}", opcode[1]), 2)
            } else {
                ("LD C,?".to_string(), 1)
            }
        }
        0x0F => ("RRCA".to_string(), 1),

        0x10 => {
            // DJNZ
            if opcode.len() >= 2 {
                let d = opcode[1] as i8;
                (format!("DJNZ {:+}", d as i32 + 2), 2)
            } else {
                ("DJNZ ?".to_string(), 1)
            }
        }
        0x11 => {
            let (val, size) = read_imm_word(&opcode[1..], adl);
            (format!("LD DE,{}", val), 1 + size)
        }
        0x12 => ("LD (DE),A".to_string(), 1),
        0x13 => ("INC DE".to_string(), 1),
        0x14 => ("INC D".to_string(), 1),
        0x15 => ("DEC D".to_string(), 1),
        0x16 => {
            if opcode.len() >= 2 {
                (format!("LD D,0x{:02X}", opcode[1]), 2)
            } else {
                ("LD D,?".to_string(), 1)
            }
        }
        0x17 => ("RLA".to_string(), 1),
        0x18 => {
            // JR
            if opcode.len() >= 2 {
                let d = opcode[1] as i8;
                (format!("JR {:+}", d as i32 + 2), 2)
            } else {
                ("JR ?".to_string(), 1)
            }
        }
        0x19 => ("ADD HL,DE".to_string(), 1),
        0x1A => ("LD A,(DE)".to_string(), 1),
        0x1B => ("DEC DE".to_string(), 1),
        0x1C => ("INC E".to_string(), 1),
        0x1D => ("DEC E".to_string(), 1),
        0x1E => {
            if opcode.len() >= 2 {
                (format!("LD E,0x{:02X}", opcode[1]), 2)
            } else {
                ("LD E,?".to_string(), 1)
            }
        }
        0x1F => ("RRA".to_string(), 1),

        0x20 => {
            if opcode.len() >= 2 {
                let d = opcode[1] as i8;
                (format!("JR NZ,{:+}", d as i32 + 2), 2)
            } else {
                ("JR NZ,?".to_string(), 1)
            }
        }
        0x21 => {
            let (val, size) = read_imm_word(&opcode[1..], adl);
            (format!("LD HL,{}", val), 1 + size)
        }
        0x22 => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("LD ({}),HL", addr), 1 + size)
        }
        0x23 => ("INC HL".to_string(), 1),
        0x24 => ("INC H".to_string(), 1),
        0x25 => ("DEC H".to_string(), 1),
        0x26 => {
            if opcode.len() >= 2 {
                (format!("LD H,0x{:02X}", opcode[1]), 2)
            } else {
                ("LD H,?".to_string(), 1)
            }
        }
        0x27 => ("DAA".to_string(), 1),
        0x28 => {
            if opcode.len() >= 2 {
                let d = opcode[1] as i8;
                (format!("JR Z,{:+}", d as i32 + 2), 2)
            } else {
                ("JR Z,?".to_string(), 1)
            }
        }
        0x29 => ("ADD HL,HL".to_string(), 1),
        0x2A => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("LD HL,({})", addr), 1 + size)
        }
        0x2B => ("DEC HL".to_string(), 1),
        0x2C => ("INC L".to_string(), 1),
        0x2D => ("DEC L".to_string(), 1),
        0x2E => {
            if opcode.len() >= 2 {
                (format!("LD L,0x{:02X}", opcode[1]), 2)
            } else {
                ("LD L,?".to_string(), 1)
            }
        }
        0x2F => ("CPL".to_string(), 1),

        0x30 => {
            if opcode.len() >= 2 {
                let d = opcode[1] as i8;
                (format!("JR NC,{:+}", d as i32 + 2), 2)
            } else {
                ("JR NC,?".to_string(), 1)
            }
        }
        0x31 => {
            let (val, size) = read_imm_word(&opcode[1..], adl);
            (format!("LD SP,{}", val), 1 + size)
        }
        0x32 => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("LD ({}),A", addr), 1 + size)
        }
        0x33 => ("INC SP".to_string(), 1),
        0x34 => ("INC (HL)".to_string(), 1),
        0x35 => ("DEC (HL)".to_string(), 1),
        0x36 => {
            if opcode.len() >= 2 {
                (format!("LD (HL),0x{:02X}", opcode[1]), 2)
            } else {
                ("LD (HL),?".to_string(), 1)
            }
        }
        0x37 => ("SCF".to_string(), 1),
        0x38 => {
            if opcode.len() >= 2 {
                let d = opcode[1] as i8;
                (format!("JR C,{:+}", d as i32 + 2), 2)
            } else {
                ("JR C,?".to_string(), 1)
            }
        }
        0x39 => ("ADD HL,SP".to_string(), 1),
        0x3A => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("LD A,({})", addr), 1 + size)
        }
        0x3B => ("DEC SP".to_string(), 1),
        0x3C => ("INC A".to_string(), 1),
        0x3D => ("DEC A".to_string(), 1),
        0x3E => {
            if opcode.len() >= 2 {
                (format!("LD A,0x{:02X}", opcode[1]), 2)
            } else {
                ("LD A,?".to_string(), 1)
            }
        }
        0x3F => ("CCF".to_string(), 1),

        // 8-bit loads
        op if op >= 0x40 && op <= 0x7F => {
            if op == 0x76 {
                ("HALT".to_string(), 1)
            } else {
                let dst = (op >> 3) & 7;
                let src = op & 7;
                (format!("LD {},{}", reg8_name(dst), reg8_name(src)), 1)
            }
        }

        // 8-bit arithmetic
        op if op >= 0x80 && op <= 0xBF => {
            let alu_op = (op >> 3) & 7;
            let operand = op & 7;
            (format!("{} {}", alu_name(alu_op), reg8_name(operand)), 1)
        }

        // Returns and misc
        0xC0 => ("RET NZ".to_string(), 1),
        0xC1 => ("POP BC".to_string(), 1),
        0xC2 => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("JP NZ,{}", addr), 1 + size)
        }
        0xC3 => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("JP {}", addr), 1 + size)
        }
        0xC4 => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("CALL NZ,{}", addr), 1 + size)
        }
        0xC5 => ("PUSH BC".to_string(), 1),
        0xC6 => {
            if opcode.len() >= 2 {
                (format!("ADD A,0x{:02X}", opcode[1]), 2)
            } else {
                ("ADD A,?".to_string(), 1)
            }
        }
        0xC7 => ("RST 00h".to_string(), 1),
        0xC8 => ("RET Z".to_string(), 1),
        0xC9 => ("RET".to_string(), 1),
        0xCA => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("JP Z,{}", addr), 1 + size)
        }
        0xCC => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("CALL Z,{}", addr), 1 + size)
        }
        0xCD => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("CALL {}", addr), 1 + size)
        }
        0xCE => {
            if opcode.len() >= 2 {
                (format!("ADC A,0x{:02X}", opcode[1]), 2)
            } else {
                ("ADC A,?".to_string(), 1)
            }
        }
        0xCF => ("RST 08h".to_string(), 1),

        0xD0 => ("RET NC".to_string(), 1),
        0xD1 => ("POP DE".to_string(), 1),
        0xD2 => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("JP NC,{}", addr), 1 + size)
        }
        0xD3 => {
            if opcode.len() >= 2 {
                (format!("OUT (0x{:02X}),A", opcode[1]), 2)
            } else {
                ("OUT (?),A".to_string(), 1)
            }
        }
        0xD4 => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("CALL NC,{}", addr), 1 + size)
        }
        0xD5 => ("PUSH DE".to_string(), 1),
        0xD6 => {
            if opcode.len() >= 2 {
                (format!("SUB 0x{:02X}", opcode[1]), 2)
            } else {
                ("SUB ?".to_string(), 1)
            }
        }
        0xD7 => ("RST 10h".to_string(), 1),
        0xD8 => ("RET C".to_string(), 1),
        0xD9 => ("EXX".to_string(), 1),
        0xDA => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("JP C,{}", addr), 1 + size)
        }
        0xDB => {
            if opcode.len() >= 2 {
                (format!("IN A,(0x{:02X})", opcode[1]), 2)
            } else {
                ("IN A,(?)".to_string(), 1)
            }
        }
        0xDC => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("CALL C,{}", addr), 1 + size)
        }
        0xDE => {
            if opcode.len() >= 2 {
                (format!("SBC A,0x{:02X}", opcode[1]), 2)
            } else {
                ("SBC A,?".to_string(), 1)
            }
        }
        0xDF => ("RST 18h".to_string(), 1),

        0xE0 => ("RET PO".to_string(), 1),
        0xE1 => ("POP HL".to_string(), 1),
        0xE2 => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("JP PO,{}", addr), 1 + size)
        }
        0xE3 => ("EX (SP),HL".to_string(), 1),
        0xE4 => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("CALL PO,{}", addr), 1 + size)
        }
        0xE5 => ("PUSH HL".to_string(), 1),
        0xE6 => {
            if opcode.len() >= 2 {
                (format!("AND 0x{:02X}", opcode[1]), 2)
            } else {
                ("AND ?".to_string(), 1)
            }
        }
        0xE7 => ("RST 20h".to_string(), 1),
        0xE8 => ("RET PE".to_string(), 1),
        0xE9 => ("JP (HL)".to_string(), 1),
        0xEA => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("JP PE,{}", addr), 1 + size)
        }
        0xEB => ("EX DE,HL".to_string(), 1),
        0xEC => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("CALL PE,{}", addr), 1 + size)
        }
        0xEE => {
            if opcode.len() >= 2 {
                (format!("XOR 0x{:02X}", opcode[1]), 2)
            } else {
                ("XOR ?".to_string(), 1)
            }
        }
        0xEF => ("RST 28h".to_string(), 1),

        0xF0 => ("RET P".to_string(), 1),
        0xF1 => ("POP AF".to_string(), 1),
        0xF2 => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("JP P,{}", addr), 1 + size)
        }
        0xF3 => ("DI".to_string(), 1),
        0xF4 => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("CALL P,{}", addr), 1 + size)
        }
        0xF5 => ("PUSH AF".to_string(), 1),
        0xF6 => {
            if opcode.len() >= 2 {
                (format!("OR 0x{:02X}", opcode[1]), 2)
            } else {
                ("OR ?".to_string(), 1)
            }
        }
        0xF7 => ("RST 30h".to_string(), 1),
        0xF8 => ("RET M".to_string(), 1),
        0xF9 => ("LD SP,HL".to_string(), 1),
        0xFA => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("JP M,{}", addr), 1 + size)
        }
        0xFB => ("EI".to_string(), 1),
        0xFC => {
            let (addr, size) = read_imm_addr(&opcode[1..], adl);
            (format!("CALL M,{}", addr), 1 + size)
        }
        0xFE => {
            if opcode.len() >= 2 {
                (format!("CP 0x{:02X}", opcode[1]), 2)
            } else {
                ("CP ?".to_string(), 1)
            }
        }
        0xFF => ("RST 38h".to_string(), 1),

        _ => (format!("DB {:02X}h", op), 1),
    }
}

// === Helper functions ===

/// Read immediate word value (2 or 3 bytes depending on ADL mode)
fn read_imm_word(data: &[u8], adl: bool) -> (String, usize) {
    if adl {
        // 3-byte address
        if data.len() >= 3 {
            let val = (data[0] as u32) | ((data[1] as u32) << 8) | ((data[2] as u32) << 16);
            (format!("0x{:06X}", val), 3)
        } else if data.len() >= 2 {
            let val = (data[0] as u32) | ((data[1] as u32) << 8);
            (format!("0x{:04X}??", val), 2)
        } else if data.len() >= 1 {
            (format!("0x{:02X}????", data[0]), 1)
        } else {
            ("0x??????".to_string(), 0)
        }
    } else {
        // 2-byte address
        if data.len() >= 2 {
            let val = (data[0] as u16) | ((data[1] as u16) << 8);
            (format!("0x{:04X}", val), 2)
        } else if data.len() >= 1 {
            (format!("0x{:02X}??", data[0]), 1)
        } else {
            ("0x????".to_string(), 0)
        }
    }
}

/// Read immediate address (same as word for most purposes)
fn read_imm_addr(data: &[u8], adl: bool) -> (String, usize) {
    read_imm_word(data, adl)
}

/// Get 8-bit register name
fn reg8_name(r: u8) -> &'static str {
    match r {
        0 => "B",
        1 => "C",
        2 => "D",
        3 => "E",
        4 => "H",
        5 => "L",
        6 => "(HL)",
        7 => "A",
        _ => "?",
    }
}

/// Get 8-bit register name for IX context
fn reg8_name_ix(r: u8) -> &'static str {
    match r {
        0 => "B",
        1 => "C",
        2 => "D",
        3 => "E",
        4 => "IXH",
        5 => "IXL",
        6 => "(IX+d)",
        7 => "A",
        _ => "?",
    }
}

/// Get 8-bit register name for IY context
fn reg8_name_iy(r: u8) -> &'static str {
    match r {
        0 => "B",
        1 => "C",
        2 => "D",
        3 => "E",
        4 => "IYH",
        5 => "IYL",
        6 => "(IY+d)",
        7 => "A",
        _ => "?",
    }
}

/// Get ALU operation name
fn alu_name(op: u8) -> &'static str {
    match op {
        0 => "ADD A,",
        1 => "ADC A,",
        2 => "SUB",
        3 => "SBC A,",
        4 => "AND",
        5 => "XOR",
        6 => "OR",
        7 => "CP",
        _ => "???",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_instructions() {
        assert_eq!(disassemble(&[0x00], false).mnemonic, "NOP");
        assert_eq!(disassemble(&[0x76], false).mnemonic, "HALT");
        assert_eq!(disassemble(&[0xF3], false).mnemonic, "DI");
        assert_eq!(disassemble(&[0xFB], false).mnemonic, "EI");
        assert_eq!(disassemble(&[0xC9], false).mnemonic, "RET");
    }

    #[test]
    fn test_ld_instructions() {
        // LD B,0x42
        assert_eq!(disassemble(&[0x06, 0x42], false).mnemonic, "LD B,0x42");
        // LD A,(HL)
        assert_eq!(disassemble(&[0x7E], false).mnemonic, "LD A,(HL)");
        // LD (HL),A
        assert_eq!(disassemble(&[0x77], false).mnemonic, "LD (HL),A");
    }

    #[test]
    fn test_16bit_loads() {
        // LD BC,0x1234 (Z80 mode - 2 bytes)
        let result = disassemble(&[0x01, 0x34, 0x12], false);
        assert_eq!(result.mnemonic, "LD BC,0x1234");
        assert_eq!(result.length, 3);

        // LD BC,0x123456 (ADL mode - 3 bytes)
        let result = disassemble(&[0x01, 0x56, 0x34, 0x12], true);
        assert_eq!(result.mnemonic, "LD BC,0x123456");
        assert_eq!(result.length, 4);
    }

    #[test]
    fn test_ed_prefix() {
        // IN0 A,(0x05)
        let result = disassemble(&[0xED, 0x38, 0x05], false);
        assert_eq!(result.mnemonic, "IN0 A,(0x05)");

        // OUT0 (0x01),A
        let result = disassemble(&[0xED, 0x39, 0x01], false);
        assert_eq!(result.mnemonic, "OUT0 (0x01),A");

        // IM 2
        let result = disassemble(&[0xED, 0x7E], false);
        assert_eq!(result.mnemonic, "IM 2");
    }

    #[test]
    fn test_ix_instructions() {
        // LD A,(IX+5)
        let result = disassemble(&[0xDD, 0x7E, 0x05], false);
        assert_eq!(result.mnemonic, "LD A,(IX+5)");

        // LD (IX-3),0x42
        let result = disassemble(&[0xDD, 0x36, 0xFD, 0x42], false);
        assert_eq!(result.mnemonic, "LD (IX-3),0x42");
    }

    #[test]
    fn test_suffix_opcodes() {
        // .SIS prefix followed by NOP
        let result = disassemble(&[0x40, 0x00], false);
        assert!(result.mnemonic.contains(".SIS"));
    }

    #[test]
    fn test_cb_prefix() {
        // RLC B
        let result = disassemble(&[0xCB, 0x00], false);
        assert_eq!(result.mnemonic, "RLC B");

        // BIT 3,A
        let result = disassemble(&[0xCB, 0x5F], false);
        assert_eq!(result.mnemonic, "BIT 3,A");

        // SET 7,(HL)
        let result = disassemble(&[0xCB, 0xFE], false);
        assert_eq!(result.mnemonic, "SET 7,(HL)");
    }
}
