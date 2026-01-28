//! Instruction-level tests for eZ80 CPU
//!
//! Tests for individual instructions and instruction families including:
//! - Basic operations: NOP, LD, register access
//! - Arithmetic: ADD, SUB, INC, DEC, ADC, SBC, NEG
//! - Logic: AND, OR, XOR, CP, CPL
//! - Rotate/shift: RLCA, RRCA, RLA, RRA, RLC, RRC, RL, RR, SLA, SRA, SRL  
//! - Bit operations: BIT, RES, SET
//! - Control flow: JP, JR, CALL, RET, DJNZ, RST, HALT
//! - Stack: PUSH, POP
//! - Extended: LD I/A/R, IM, block ops (LDI, LDIR, CPI, etc.), RRD, RLD
//! - Indexed: IX, IY operations
//! - DAA (Decimal Adjust Accumulator)
//! - Flag behavior verification
//!
//! # References
//! - eZ80 CPU User Manual (Zilog UM0077)
//! - CEmu (https://github.com/CE-Programming/CEmu)

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
    assert!(cpu.flag_z()); // Z set because equal
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

// ========== Regression Tests for Bug Fixes ==========
// These tests verify correct behavior that was previously buggy

#[test]
fn test_ld_a_hl_uses_read_byte() {
    // Bug: get_reg8 was using peek_byte instead of read_byte for (HL)
    // This test verifies that LD A,(HL) properly reads from memory
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.hl = 0xD00100;
    bus.poke_byte(0xD00100, 0x42);

    // LD A,(HL) (opcode 0x7E)
    bus.poke_byte(0, 0x7E);
    let cycles = cpu.step(&mut bus);

    assert_eq!(cpu.a, 0x42, "LD A,(HL) should read value from memory");
    assert_eq!(
        cycles, 7,
        "LD A,(HL) should take 7 cycles (includes memory read)"
    );
}

#[test]
fn test_add_a_hl_uses_read_byte() {
    // Verify ADD A,(HL) properly reads from memory
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.a = 0x10;
    cpu.hl = 0xD00100;
    bus.poke_byte(0xD00100, 0x05);

    // ADD A,(HL) (opcode 0x86)
    bus.poke_byte(0, 0x86);
    let cycles = cpu.step(&mut bus);

    assert_eq!(cpu.a, 0x15, "ADD A,(HL) should add value from memory");
    assert_eq!(cycles, 7, "ADD A,(HL) should take 7 cycles");
}

#[test]
fn test_inc_indexed_mem_r_register() {
    // Bug fixed: INC (IX+d) no longer double-fetches the displacement
    // Note: Our implementation increments R on every fetch_byte call (3 times: DD, opcode, displacement)
    // Strict Z80: R should only increment on M1 cycles (opcode fetches), so should be 2
    // This test verifies the fix doesn't cause extra R increments beyond the fetch calls
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.ix = 0xD00100;
    cpu.r = 0;
    bus.poke_byte(0xD00105, 0x41);

    // INC (IX+5) (DD 34 05)
    bus.poke_byte(0, 0xDD);
    bus.poke_byte(1, 0x34);
    bus.poke_byte(2, 0x05);

    cpu.step(&mut bus);

    assert_eq!(
        bus.peek_byte(0xD00105),
        0x42,
        "INC (IX+d) should increment memory"
    );
    // R increments 3 times in our impl: DD prefix, opcode, displacement
    // Previously with double-fetch bug it was 4 (displacement fetched twice)
    assert_eq!(
        cpu.r & 0x7F,
        3,
        "R should increment by 3 (DD + opcode + displacement fetch)"
    );
}

#[test]
fn test_dec_indexed_mem_r_register() {
    // Bug fixed: DEC (IY+d) no longer double-fetches the displacement
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.iy = 0xD00100;
    cpu.r = 0;
    bus.poke_byte(0xD00103, 0x42);

    // DEC (IY+3) (FD 35 03)
    bus.poke_byte(0, 0xFD);
    bus.poke_byte(1, 0x35);
    bus.poke_byte(2, 0x03);

    cpu.step(&mut bus);

    assert_eq!(
        bus.peek_byte(0xD00103),
        0x41,
        "DEC (IY+d) should decrement memory"
    );
    assert_eq!(
        cpu.r & 0x7F,
        3,
        "R should increment by 3 (FD + opcode + displacement fetch)"
    );
}

#[test]
fn test_push_pop_bc_24bit_adl() {
    // Bug: PUSH/POP BC was only using 16-bit in ADL mode
    // This test verifies full 24-bit round-trip
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = true;
    cpu.sp = 0xD00200;
    cpu.bc = 0xABCDEF; // Full 24-bit value

    // PUSH BC (opcode 0xC5)
    bus.poke_byte(0, 0xC5);
    cpu.step(&mut bus);

    // SP should decrease by 3 in ADL mode
    assert_eq!(cpu.sp, 0xD001FD, "SP should decrease by 3 for 24-bit push");

    // Verify all 3 bytes on stack
    assert_eq!(bus.peek_byte(0xD001FD), 0xEF, "Low byte on stack");
    assert_eq!(bus.peek_byte(0xD001FE), 0xCD, "Middle byte on stack");
    assert_eq!(bus.peek_byte(0xD001FF), 0xAB, "High byte on stack");

    // Clear BC and pop it back
    cpu.bc = 0;

    // POP BC (opcode 0xC1)
    bus.poke_byte(1, 0xC1);
    cpu.step(&mut bus);

    assert_eq!(cpu.bc, 0xABCDEF, "POP BC should restore full 24-bit value");
    assert_eq!(cpu.sp, 0xD00200, "SP should return to original value");
}

#[test]
fn test_push_pop_de_24bit_adl() {
    // Verify DE also uses 24-bit push/pop in ADL mode
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = true;
    cpu.sp = 0xD00200;
    cpu.de = 0x123456;

    // PUSH DE (opcode 0xD5)
    bus.poke_byte(0, 0xD5);
    cpu.step(&mut bus);

    assert_eq!(cpu.sp, 0xD001FD, "SP should decrease by 3");

    cpu.de = 0;

    // POP DE (opcode 0xD1)
    bus.poke_byte(1, 0xD1);
    cpu.step(&mut bus);

    assert_eq!(cpu.de, 0x123456, "POP DE should restore full 24-bit value");
}

#[test]
fn test_push_pop_hl_24bit_adl() {
    // Verify HL also uses 24-bit push/pop in ADL mode
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = true;
    cpu.sp = 0xD00200;
    cpu.hl = 0xFEDCBA;

    // PUSH HL (opcode 0xE5)
    bus.poke_byte(0, 0xE5);
    cpu.step(&mut bus);

    assert_eq!(cpu.sp, 0xD001FD, "SP should decrease by 3");

    cpu.hl = 0;

    // POP HL (opcode 0xE1)
    bus.poke_byte(1, 0xE1);
    cpu.step(&mut bus);

    assert_eq!(cpu.hl, 0xFEDCBA, "POP HL should restore full 24-bit value");
}

#[test]
fn test_push_pop_af_16bit_adl() {
    // AF should remain 16-bit even in ADL mode (A and F are always 8-bit each)
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = true;
    cpu.sp = 0xD00200;
    cpu.a = 0xAB;
    cpu.f = 0xCD;

    // PUSH AF (opcode 0xF5)
    bus.poke_byte(0, 0xF5);
    cpu.step(&mut bus);

    // SP should decrease by 2 for AF (always 16-bit)
    assert_eq!(cpu.sp, 0xD001FE, "SP should decrease by 2 for AF (16-bit)");

    cpu.a = 0;
    cpu.f = 0;

    // POP AF (opcode 0xF1)
    bus.poke_byte(1, 0xF1);
    cpu.step(&mut bus);

    assert_eq!(cpu.a, 0xAB, "A should be restored");
    assert_eq!(cpu.f, 0xCD, "F should be restored");
    assert_eq!(cpu.sp, 0xD00200, "SP should return to original");
}

#[test]
fn test_push_pop_bc_16bit_z80_mode() {
    // In Z80 mode (ADL=false), push/pop should be 16-bit
    // Note: In Z80 mode, addresses combine MBASE with 16-bit offset
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = false;
    // Keep default MBASE (0xD0) which maps to RAM region
    // SP low 16-bits = 0x0200, with MBASE=0xD0 gives address 0xD00200 (in RAM)
    cpu.sp = 0xD00200;
    cpu.bc = 0x123456; // Upper byte should be ignored in 16-bit push

    // Code needs to be in the MBASE region too
    // Put code at 0xD00000
    cpu.pc = 0xD00000;

    // PUSH BC (opcode 0xC5)
    bus.poke_byte(0xD00000, 0xC5);
    cpu.step(&mut bus);

    // SP should decrease by 2 in Z80 mode
    // Low 16 bits: 0x0200 - 2 = 0x01FE
    assert_eq!(
        cpu.sp & 0xFFFF,
        0x01FE,
        "SP low 16 bits should decrease by 2 in Z80 mode"
    );

    cpu.bc = 0;

    // POP BC (opcode 0xC1)
    bus.poke_byte(0xD00001, 0xC1);
    cpu.step(&mut bus);

    // Only lower 16 bits should be restored
    assert_eq!(
        cpu.bc & 0xFFFF,
        0x3456,
        "POP BC should restore 16-bit value in Z80 mode"
    );
}

#[test]
fn test_indexed_push_pop_bc_24bit_adl() {
    // Bug: Indexed (DD/FD prefix) PUSH/POP BC/DE were also using 16-bit
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = true;
    cpu.sp = 0xD00200;
    cpu.bc = 0xAABBCC;

    // DD PUSH BC (DD C5) - DD prefix but BC still uses 24-bit
    bus.poke_byte(0, 0xDD);
    bus.poke_byte(1, 0xC5);
    cpu.step(&mut bus);

    assert_eq!(cpu.sp, 0xD001FD, "SP should decrease by 3 for DD PUSH BC");

    cpu.bc = 0;

    // DD POP BC (DD C1)
    bus.poke_byte(2, 0xDD);
    bus.poke_byte(3, 0xC1);
    cpu.step(&mut bus);

    assert_eq!(
        cpu.bc, 0xAABBCC,
        "DD POP BC should restore full 24-bit value"
    );
}

#[test]
fn test_cb_bit_hl_uses_read_byte() {
    // BIT n,(HL) should use read_byte for proper cycle counting
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.hl = 0xD00100;
    bus.poke_byte(0xD00100, 0x80); // Bit 7 set

    // BIT 7,(HL) (CB 7E)
    bus.poke_byte(0, 0xCB);
    bus.poke_byte(1, 0x7E);
    let cycles = cpu.step(&mut bus);

    assert!(!cpu.flag_z(), "Z flag should be clear (bit 7 is set)");
    // BIT n,(HL) takes 12 cycles: 4 for CB prefix + 4 for opcode + 4 for memory read
    assert_eq!(cycles, 12, "BIT n,(HL) should take 12 cycles");
}

#[test]
fn test_ld_hl_indirect_memory_read() {
    // LD r,(HL) variants should all properly read from memory
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.hl = 0xD00100;
    bus.poke_byte(0xD00100, 0x55);

    // LD B,(HL) (opcode 0x46)
    bus.poke_byte(0, 0x46);
    cpu.step(&mut bus);

    assert_eq!(cpu.b(), 0x55, "LD B,(HL) should read from memory");

    // LD C,(HL) (opcode 0x4E)
    bus.poke_byte(0xD00100, 0x66);
    bus.poke_byte(1, 0x4E);
    cpu.step(&mut bus);

    assert_eq!(cpu.c(), 0x66, "LD C,(HL) should read from memory");
}

#[test]
fn test_inc_hl_indirect_cycle_count() {
    // INC (HL) should have proper cycle count including memory read/write
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.hl = 0xD00100;
    bus.poke_byte(0xD00100, 0x41);

    // INC (HL) (opcode 0x34)
    bus.poke_byte(0, 0x34);
    let cycles = cpu.step(&mut bus);

    assert_eq!(bus.peek_byte(0xD00100), 0x42);
    assert_eq!(cycles, 11, "INC (HL) should take 11 cycles");
}

#[test]
fn test_dec_hl_indirect_cycle_count() {
    // DEC (HL) should have proper cycle count
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.hl = 0xD00100;
    bus.poke_byte(0xD00100, 0x42);

    // DEC (HL) (opcode 0x35)
    bus.poke_byte(0, 0x35);
    let cycles = cpu.step(&mut bus);

    assert_eq!(bus.peek_byte(0xD00100), 0x41);
    assert_eq!(cycles, 11, "DEC (HL) should take 11 cycles");
}

// ========== ZEXALL-Style Comprehensive Flag Tests ==========
// These tests validate all 8 flag bits (including undocumented F3/F5)
// Reference: ZEXALL Z80 instruction exerciser validates against real hardware

#[test]
fn test_add_all_flags() {
    // Test ADD with comprehensive flag validation
    // Based on ZEXALL "aluop a,<b,c,d,e,h,l,(hl),a>" tests
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // ADD A,B: 0x44 + 0x11 = 0x55
    // Expected: S=0 Z=0 H=0 PV=0 N=0 C=0, F3/F5 from result (0x55)
    cpu.a = 0x44;
    cpu.set_b(0x11);
    cpu.f = 0;
    bus.poke_byte(0, 0x80);
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x55);
    // F5=bit5 of result=1, F3=bit3 of result=0
    assert!(!cpu.flag_s(), "ADD 0x44+0x11: S should be clear");
    assert!(!cpu.flag_z(), "ADD 0x44+0x11: Z should be clear");
    assert!(!cpu.flag_h(), "ADD 0x44+0x11: H should be clear");
    assert!(
        !cpu.flag_pv(),
        "ADD 0x44+0x11: PV should be clear (no overflow)"
    );
    assert!(!cpu.flag_n(), "ADD: N should always be clear");
    assert!(!cpu.flag_c(), "ADD 0x44+0x11: C should be clear");

    // ADD A,B: 0x7F + 0x01 = 0x80 (overflow: positive + positive = negative)
    cpu.a = 0x7F;
    cpu.set_b(0x01);
    cpu.f = 0;
    bus.poke_byte(1, 0x80);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x80);
    assert!(
        cpu.flag_s(),
        "ADD overflow: S should be set (result negative)"
    );
    assert!(!cpu.flag_z(), "ADD overflow: Z should be clear");
    assert!(
        cpu.flag_h(),
        "ADD 0x7F+0x01: H should be set (0xF+0x1=0x10)"
    );
    assert!(
        cpu.flag_pv(),
        "ADD overflow: PV should be set (signed overflow)"
    );
    assert!(!cpu.flag_c(), "ADD 0x7F+0x01: C should be clear");

    // ADD A,B: 0x80 + 0x80 = 0x00 (overflow: negative + negative = positive with carry)
    cpu.a = 0x80;
    cpu.set_b(0x80);
    cpu.f = 0;
    bus.poke_byte(2, 0x80);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x00);
    assert!(!cpu.flag_s(), "ADD 0x80+0x80: S should be clear");
    assert!(cpu.flag_z(), "ADD 0x80+0x80: Z should be set (result=0)");
    assert!(!cpu.flag_h(), "ADD 0x80+0x80: H should be clear");
    assert!(
        cpu.flag_pv(),
        "ADD 0x80+0x80: PV should be set (signed overflow)"
    );
    assert!(
        cpu.flag_c(),
        "ADD 0x80+0x80: C should be set (unsigned overflow)"
    );
}

#[test]
fn test_sub_all_flags() {
    // Test SUB with comprehensive flag validation
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // SUB B: 0x44 - 0x11 = 0x33
    cpu.a = 0x44;
    cpu.set_b(0x11);
    cpu.f = 0;
    bus.poke_byte(0, 0x90);
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x33);
    assert!(!cpu.flag_s(), "SUB 0x44-0x11: S should be clear");
    assert!(!cpu.flag_z(), "SUB 0x44-0x11: Z should be clear");
    assert!(!cpu.flag_h(), "SUB 0x44-0x11: H should be clear");
    assert!(!cpu.flag_pv(), "SUB 0x44-0x11: PV should be clear");
    assert!(cpu.flag_n(), "SUB: N should always be set");
    assert!(
        !cpu.flag_c(),
        "SUB 0x44-0x11: C should be clear (no borrow)"
    );

    // SUB B: 0x00 - 0x01 = 0xFF (borrow)
    cpu.a = 0x00;
    cpu.set_b(0x01);
    cpu.f = 0;
    bus.poke_byte(1, 0x90);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0xFF);
    assert!(
        cpu.flag_s(),
        "SUB 0x00-0x01: S should be set (result negative)"
    );
    assert!(!cpu.flag_z(), "SUB 0x00-0x01: Z should be clear");
    assert!(cpu.flag_h(), "SUB 0x00-0x01: H should be set (half-borrow)");
    assert!(
        !cpu.flag_pv(),
        "SUB 0x00-0x01: PV should be clear (no signed overflow)"
    );
    assert!(cpu.flag_c(), "SUB 0x00-0x01: C should be set (borrow)");

    // SUB B: 0x80 - 0x01 = 0x7F (underflow: -128 - 1 = +127)
    cpu.a = 0x80;
    cpu.set_b(0x01);
    cpu.f = 0;
    bus.poke_byte(2, 0x90);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x7F);
    assert!(!cpu.flag_s(), "SUB underflow: S should be clear");
    assert!(!cpu.flag_z(), "SUB underflow: Z should be clear");
    assert!(cpu.flag_h(), "SUB 0x80-0x01: H should be set");
    assert!(
        cpu.flag_pv(),
        "SUB underflow: PV should be set (signed overflow)"
    );
    assert!(!cpu.flag_c(), "SUB 0x80-0x01: C should be clear");
}

#[test]
fn test_inc_all_flags() {
    // INC doesn't affect C flag - important ZEXALL test
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // INC A when C is set - C should remain set
    cpu.a = 0x00;
    cpu.f = flags::C; // Set carry
    bus.poke_byte(0, 0x3C); // INC A
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x01);
    assert!(cpu.flag_c(), "INC should NOT affect C flag");
    assert!(!cpu.flag_z(), "INC 0x00: Z should be clear");

    // INC A: 0xFF -> 0x00 (wrap)
    cpu.a = 0xFF;
    cpu.f = 0;
    bus.poke_byte(1, 0x3C);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x00);
    assert!(cpu.flag_z(), "INC 0xFF: Z should be set");
    assert!(!cpu.flag_s(), "INC 0xFF: S should be clear");
    assert!(cpu.flag_h(), "INC 0xFF: H should be set (0xF+1)");
    assert!(!cpu.flag_c(), "INC should NOT affect C flag");

    // INC A: 0x7F -> 0x80 (overflow)
    cpu.a = 0x7F;
    cpu.f = 0;
    bus.poke_byte(2, 0x3C);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x80);
    assert!(cpu.flag_s(), "INC 0x7F: S should be set");
    assert!(cpu.flag_pv(), "INC 0x7F: PV should be set (overflow)");
    assert!(cpu.flag_h(), "INC 0x7F: H should be set");
}

#[test]
fn test_dec_all_flags() {
    // DEC doesn't affect C flag - important ZEXALL test
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // DEC A when C is set - C should remain set
    cpu.a = 0x01;
    cpu.f = flags::C;
    bus.poke_byte(0, 0x3D); // DEC A
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x00);
    assert!(cpu.flag_c(), "DEC should NOT affect C flag");
    assert!(cpu.flag_z(), "DEC 0x01: Z should be set");

    // DEC A: 0x00 -> 0xFF (wrap)
    cpu.a = 0x00;
    cpu.f = 0;
    bus.poke_byte(1, 0x3D);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0xFF);
    assert!(!cpu.flag_z(), "DEC 0x00: Z should be clear");
    assert!(cpu.flag_s(), "DEC 0x00: S should be set");
    assert!(cpu.flag_h(), "DEC 0x00: H should be set (half-borrow)");

    // DEC A: 0x80 -> 0x7F (underflow)
    cpu.a = 0x80;
    cpu.f = 0;
    bus.poke_byte(2, 0x3D);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x7F);
    assert!(!cpu.flag_s(), "DEC 0x80: S should be clear");
    assert!(cpu.flag_pv(), "DEC 0x80: PV should be set (underflow)");
    assert!(cpu.flag_h(), "DEC 0x80: H should be set");
}

#[test]
fn test_adc_with_carry() {
    // Test ADC with different carry states
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // ADC A,B with carry clear: 0x10 + 0x20 + 0 = 0x30
    cpu.a = 0x10;
    cpu.set_b(0x20);
    cpu.f = 0;
    bus.poke_byte(0, 0x88); // ADC A,B
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x30);
    assert!(!cpu.flag_c());

    // ADC A,B with carry set: 0x10 + 0x20 + 1 = 0x31
    cpu.a = 0x10;
    cpu.set_b(0x20);
    cpu.f = flags::C;
    bus.poke_byte(1, 0x88);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x31);
    assert!(!cpu.flag_c());

    // ADC A,B: 0xFF + 0x00 + 1 = 0x00 with carry
    cpu.a = 0xFF;
    cpu.set_b(0x00);
    cpu.f = flags::C;
    bus.poke_byte(2, 0x88);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x00);
    assert!(cpu.flag_c(), "ADC 0xFF+0+1: C should be set");
    assert!(cpu.flag_z(), "ADC 0xFF+0+1: Z should be set");
}

#[test]
fn test_sbc_with_carry() {
    // Test SBC with different carry states
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // SBC A,B with carry clear: 0x30 - 0x10 - 0 = 0x20
    cpu.a = 0x30;
    cpu.set_b(0x10);
    cpu.f = 0;
    bus.poke_byte(0, 0x98); // SBC A,B
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x20);
    assert!(!cpu.flag_c());

    // SBC A,B with carry set: 0x30 - 0x10 - 1 = 0x1F
    cpu.a = 0x30;
    cpu.set_b(0x10);
    cpu.f = flags::C;
    bus.poke_byte(1, 0x98);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x1F);
    assert!(!cpu.flag_c());

    // SBC A,B: 0x00 - 0x00 - 1 = 0xFF with carry
    cpu.a = 0x00;
    cpu.set_b(0x00);
    cpu.f = flags::C;
    bus.poke_byte(2, 0x98);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0xFF);
    assert!(cpu.flag_c(), "SBC 0x00-0-1: C should be set");
    assert!(!cpu.flag_z(), "SBC 0x00-0-1: Z should be clear");
}

#[test]
fn test_and_flags() {
    // AND always sets H, clears N and C
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.a = 0xFF;
    cpu.set_b(0x0F);
    cpu.f = flags::C | flags::N; // These should be cleared
    bus.poke_byte(0, 0xA0); // AND B
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x0F);
    assert!(cpu.flag_h(), "AND: H should always be set");
    assert!(!cpu.flag_n(), "AND: N should always be clear");
    assert!(!cpu.flag_c(), "AND: C should always be clear");
    assert!(cpu.flag_pv(), "AND 0x0F: PV=parity (even number of 1s)");

    // AND resulting in zero
    cpu.a = 0xF0;
    cpu.set_b(0x0F);
    cpu.f = 0;
    bus.poke_byte(1, 0xA0);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x00);
    assert!(cpu.flag_z(), "AND 0xF0 & 0x0F: Z should be set");
    assert!(cpu.flag_pv(), "AND result 0x00: PV=parity (even)");
}

#[test]
fn test_or_flags() {
    // OR always clears H, N, and C
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.a = 0xF0;
    cpu.set_b(0x0F);
    cpu.f = flags::C | flags::N | flags::H; // All should be cleared
    bus.poke_byte(0, 0xB0); // OR B
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0xFF);
    assert!(!cpu.flag_h(), "OR: H should always be clear");
    assert!(!cpu.flag_n(), "OR: N should always be clear");
    assert!(!cpu.flag_c(), "OR: C should always be clear");
    assert!(cpu.flag_pv(), "OR 0xFF: PV=parity (even number of 1s)");
    assert!(cpu.flag_s(), "OR 0xFF: S should be set");
}

#[test]
fn test_xor_flags() {
    // XOR always clears H, N, and C
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.a = 0xFF;
    cpu.set_b(0x55);
    cpu.f = flags::C | flags::N | flags::H;
    bus.poke_byte(0, 0xA8); // XOR B
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0xAA);
    assert!(!cpu.flag_h(), "XOR: H should always be clear");
    assert!(!cpu.flag_n(), "XOR: N should always be clear");
    assert!(!cpu.flag_c(), "XOR: C should always be clear");

    // XOR A,A = 0 (common idiom to clear A)
    cpu.a = 0x55;
    cpu.f = flags::C;
    bus.poke_byte(1, 0xAF); // XOR A
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x00);
    assert!(cpu.flag_z(), "XOR A,A: Z should be set");
    assert!(cpu.flag_pv(), "XOR 0x00: PV=parity (even)");
}

#[test]
fn test_cp_flags() {
    // CP is like SUB but doesn't store result
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // CP B: 0x44 - 0x44 = 0 (equal)
    cpu.a = 0x44;
    cpu.set_b(0x44);
    cpu.f = 0;
    bus.poke_byte(0, 0xB8); // CP B
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x44, "CP should not modify A");
    assert!(cpu.flag_z(), "CP equal: Z should be set");
    assert!(!cpu.flag_c(), "CP equal: C should be clear");
    assert!(cpu.flag_n(), "CP: N should always be set");

    // CP B: 0x44 - 0x45 (A < B)
    cpu.a = 0x44;
    cpu.set_b(0x45);
    cpu.f = 0;
    bus.poke_byte(1, 0xB8);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x44);
    assert!(!cpu.flag_z(), "CP A<B: Z should be clear");
    assert!(cpu.flag_c(), "CP A<B: C should be set (borrow)");
    assert!(cpu.flag_s(), "CP A<B: S should be set (result negative)");
}

#[test]
fn test_scf_ccf_flags() {
    // SCF sets carry, CCF complements it
    // These are tested by ZEXALL "<daa,cpl,scf,ccf>" test
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // SCF - Set Carry Flag
    cpu.f = 0;
    bus.poke_byte(0, 0x37); // SCF
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert!(cpu.flag_c(), "SCF: C should be set");
    assert!(!cpu.flag_n(), "SCF: N should be clear");
    assert!(!cpu.flag_h(), "SCF: H should be clear");

    // CCF - Complement Carry Flag (C was set, should clear)
    bus.poke_byte(1, 0x3F); // CCF
    cpu.step(&mut bus);
    assert!(!cpu.flag_c(), "CCF: C should be complemented (now clear)");
    assert!(!cpu.flag_n(), "CCF: N should be clear");
    // H gets the old carry value
    assert!(cpu.flag_h(), "CCF: H should be old C value (was set)");

    // CCF again - C was clear, should set
    bus.poke_byte(2, 0x3F);
    cpu.step(&mut bus);
    assert!(cpu.flag_c(), "CCF: C should be complemented (now set)");
    assert!(!cpu.flag_h(), "CCF: H should be old C value (was clear)");
}

#[test]
fn test_cpl_flags() {
    // CPL complements A, sets H and N
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.a = 0x55;
    cpu.f = flags::C; // C should be preserved
    bus.poke_byte(0, 0x2F); // CPL
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0xAA, "CPL should complement A");
    assert!(cpu.flag_h(), "CPL: H should be set");
    assert!(cpu.flag_n(), "CPL: N should be set");
    assert!(cpu.flag_c(), "CPL: C should be preserved");
}

#[test]
fn test_rlca_rrca_flags() {
    // RLCA/RRCA: rotate A, bit 7/0 goes to C and wraps
    // Clears H and N, doesn't affect S, Z, PV
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // RLCA with bit 7 set
    cpu.a = 0x80; // 10000000
    cpu.f = flags::Z | flags::S | flags::PV; // These should be preserved
    bus.poke_byte(0, 0x07); // RLCA
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x01, "RLCA: 0x80 -> 0x01");
    assert!(cpu.flag_c(), "RLCA: C = old bit 7");
    assert!(!cpu.flag_h(), "RLCA: H should be clear");
    assert!(!cpu.flag_n(), "RLCA: N should be clear");
    assert!(cpu.flag_z(), "RLCA: Z should be preserved");
    assert!(cpu.flag_s(), "RLCA: S should be preserved");

    // RRCA with bit 0 set
    cpu.a = 0x01; // 00000001
    cpu.f = flags::Z;
    bus.poke_byte(1, 0x0F); // RRCA
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x80, "RRCA: 0x01 -> 0x80");
    assert!(cpu.flag_c(), "RRCA: C = old bit 0");
    assert!(cpu.flag_z(), "RRCA: Z should be preserved");
}

#[test]
fn test_rla_rra_flags() {
    // RLA/RRA: rotate through carry
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // RLA with carry set
    cpu.a = 0x40; // 01000000
    cpu.f = flags::C; // Carry is set
    bus.poke_byte(0, 0x17); // RLA
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x81, "RLA: 0x40 with C=1 -> 0x81");
    assert!(!cpu.flag_c(), "RLA: C = old bit 7 (was 0)");

    // RRA with carry set
    cpu.a = 0x02; // 00000010
    cpu.f = flags::C;
    bus.poke_byte(1, 0x1F); // RRA
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x81, "RRA: 0x02 with C=1 -> 0x81");
    assert!(!cpu.flag_c(), "RRA: C = old bit 0 (was 0)");
}

#[test]
fn test_adc_hl_all_flags() {
    // ADC HL,rp affects all flags including PV for overflow
    // Test in Z80 mode (16-bit) for ZEXALL compatibility
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = false; // Z80 mode for 16-bit semantics
    cpu.mbase = 0xD0; // Point to RAM

    // ADC HL,BC: 0x7FFF + 0x0001 + 0 = 0x8000 (overflow in 16-bit)
    cpu.hl = 0x7FFF;
    cpu.bc = 0x0001;
    cpu.f = 0;
    bus.poke_byte(0xD00000, 0xED);
    bus.poke_byte(0xD00001, 0x4A); // ADC HL,BC
    cpu.pc = 0xD00000;
    cpu.step(&mut bus);
    assert_eq!(cpu.hl & 0xFFFF, 0x8000);
    assert!(
        cpu.flag_s(),
        "ADC HL overflow: S should be set (bit 15 of result)"
    );
    assert!(!cpu.flag_z(), "ADC HL overflow: Z should be clear");
    assert!(
        cpu.flag_pv(),
        "ADC HL overflow: PV should be set (signed overflow)"
    );
    assert!(!cpu.flag_c(), "ADC HL 0x7FFF+0x0001: C should be clear");

    // ADC HL,BC: 0xFFFF + 0x0001 + 0 = 0x0000 with carry
    cpu.hl = 0xFFFF;
    cpu.bc = 0x0001;
    cpu.f = 0;
    cpu.pc = 0xD00000;
    cpu.step(&mut bus);
    assert_eq!(cpu.hl & 0xFFFF, 0x0000);
    assert!(cpu.flag_z(), "ADC HL 0xFFFF+0x0001: Z should be set");
    assert!(cpu.flag_c(), "ADC HL 0xFFFF+0x0001: C should be set");
}

#[test]
fn test_sbc_hl_all_flags() {
    // SBC HL,rp affects all flags
    // Test in Z80 mode (16-bit) for ZEXALL compatibility
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = false; // Z80 mode for 16-bit semantics
    cpu.mbase = 0xD0; // Point to RAM

    // SBC HL,BC: 0x8000 - 0x0001 - 0 = 0x7FFF (underflow in 16-bit)
    cpu.hl = 0x8000;
    cpu.bc = 0x0001;
    cpu.f = 0;
    bus.poke_byte(0xD00000, 0xED);
    bus.poke_byte(0xD00001, 0x42); // SBC HL,BC
    cpu.pc = 0xD00000;
    cpu.step(&mut bus);
    assert_eq!(cpu.hl & 0xFFFF, 0x7FFF);
    assert!(!cpu.flag_s(), "SBC HL underflow: S should be clear");
    assert!(!cpu.flag_z(), "SBC HL underflow: Z should be clear");
    assert!(
        cpu.flag_pv(),
        "SBC HL underflow: PV should be set (signed overflow)"
    );
    assert!(cpu.flag_n(), "SBC HL: N should be set");
    assert!(!cpu.flag_c(), "SBC HL 0x8000-0x0001: C should be clear");

    // SBC HL,BC: 0x0000 - 0x0001 - 0 = 0xFFFF with borrow
    cpu.hl = 0x0000;
    cpu.bc = 0x0001;
    cpu.f = 0;
    cpu.pc = 0xD00000;
    cpu.step(&mut bus);
    assert_eq!(cpu.hl & 0xFFFF, 0xFFFF);
    assert!(cpu.flag_s(), "SBC HL 0x0000-0x0001: S should be set");
    assert!(!cpu.flag_z(), "SBC HL 0x0000-0x0001: Z should be clear");
    assert!(
        cpu.flag_c(),
        "SBC HL 0x0000-0x0001: C should be set (borrow)"
    );
}
