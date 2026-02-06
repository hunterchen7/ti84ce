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
    assert!(!cpu.adl);
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
    // ADL mode (24-bit): swap full values
    let mut cpu = Cpu::new();
    cpu.l = true;
    cpu.de = 0x123456;
    cpu.hl = 0xABCDEF;

    cpu.ex_de_hl();

    assert_eq!(cpu.de, 0xABCDEF);
    assert_eq!(cpu.hl, 0x123456);

    // Z80 mode (16-bit): swap only lower 16 bits, mask upper byte
    let mut cpu2 = Cpu::new();
    cpu2.l = false;
    cpu2.de = 0x123456;
    cpu2.hl = 0xABCDEF;

    cpu2.ex_de_hl();

    assert_eq!(cpu2.de, 0xCDEF);
    assert_eq!(cpu2.hl, 0x3456);
}

#[test]
fn test_mask_addr_instr_adl() {
    let mut cpu = Cpu::new();
    cpu.adl = true;
    assert_eq!(cpu.mask_addr_instr(0x123456), 0x123456);
    assert_eq!(cpu.mask_addr_instr(0xFF123456), 0x123456); // Upper bits masked
}

#[test]
fn test_mask_addr_data_l() {
    let mut cpu = Cpu::new();
    cpu.l = true;
    assert_eq!(cpu.mask_addr(0x123456), 0x123456);
    assert_eq!(cpu.mask_addr(0xFF123456), 0x123456); // Upper bits masked
}

#[test]
fn test_mask_addr_data_z80() {
    let mut cpu = Cpu::new();
    cpu.l = false;
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

    // NOP at address 0 (flash memory)
    bus.poke_byte(0, 0x00); // NOP

    let cycles = cpu.step(&mut bus);
    // NOP: 1 flash fetch = FLASH_READ_CYCLES (10)
    assert_eq!(cycles, 10);
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
    cpu.adl = true;

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
    cpu.adl = true;

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
    cpu.init_prefetch(&mut bus); // Load first byte into prefetch

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

    step_full(&mut cpu, &mut bus);
    // PC was 0x102 after fetch, then -3 = 0xFF
    assert_eq!(cpu.pc, 0xFF);
}

#[test]
fn test_call_ret() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

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
fn test_ret_nz_conditional() {
    // Test RET NZ (0xC0) - return if Z flag is NOT set
    // This is used in format routines for decimal point logic
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    // Setup: call a subroutine first to put return address on stack
    cpu.sp = 0xD00100;
    bus.poke_byte(0, 0xCD);      // CALL 0x001000
    bus.poke_byte(1, 0x00);
    bus.poke_byte(2, 0x10);
    bus.poke_byte(3, 0x00);
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x001000, "Should be at subroutine");

    // Test 1: RET NZ when Z is SET (should NOT return)
    cpu.f = flags::Z;            // Z flag SET
    bus.poke_byte(0x001000, 0xC0); // RET NZ
    bus.poke_byte(0x001001, 0x00); // NOP (next instruction)
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x001001, "RET NZ should NOT return when Z is set");

    // Test 2: RET NZ when Z is CLEAR (should return)
    // First, call again
    cpu.pc = 0;
    cpu.sp = 0xD00100;
    cpu.step(&mut bus);          // CALL 0x001000
    assert_eq!(cpu.pc, 0x001000);

    cpu.f = 0;                   // Z flag CLEAR
    cpu.step(&mut bus);          // RET NZ
    assert_eq!(cpu.pc, 4, "RET NZ should return when Z is clear");
}

#[test]
fn test_ret_z_conditional() {
    // Test RET Z (0xC8) - return if Z flag IS set
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    // Setup: call a subroutine
    cpu.sp = 0xD00100;
    bus.poke_byte(0, 0xCD);      // CALL 0x001000
    bus.poke_byte(1, 0x00);
    bus.poke_byte(2, 0x10);
    bus.poke_byte(3, 0x00);
    cpu.step(&mut bus);

    // Test 1: RET Z when Z is CLEAR (should NOT return)
    cpu.f = 0;                   // Z flag CLEAR
    bus.poke_byte(0x001000, 0xC8); // RET Z
    bus.poke_byte(0x001001, 0x00); // NOP
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x001001, "RET Z should NOT return when Z is clear");

    // Test 2: RET Z when Z is SET (should return)
    cpu.pc = 0;
    cpu.sp = 0xD00100;
    cpu.step(&mut bus);          // CALL 0x001000

    cpu.f = flags::Z;            // Z flag SET
    cpu.step(&mut bus);          // RET Z
    assert_eq!(cpu.pc, 4, "RET Z should return when Z is set");
}

#[test]
fn test_push_pop() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

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
    assert!(!cpu.iff1);
    assert!(!cpu.iff2);

    // Next instruction executes with interrupts still disabled
    bus.poke_byte(2, 0x00); // NOP
    cpu.step(&mut bus);
    assert!(!cpu.iff1);
    assert!(!cpu.iff2);

    // After one instruction, interrupts become enabled
    bus.poke_byte(3, 0x00); // NOP
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
    cpu.adl = true;

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
    cpu.adl = true;

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

    // eZ80 maps y value directly to IM mode (different from standard Z80!)
    // ED 56 (y=2) -> IM 2 on eZ80 (would be IM 1 on standard Z80)
    bus.poke_byte(0, 0xED);
    bus.poke_byte(1, 0x56);
    cpu.step(&mut bus);
    assert_eq!(cpu.im, InterruptMode::Mode2);

    // ED 5E (y=3) -> IM 3 on eZ80 (treated as Mode2 since we don't have IM 3)
    bus.poke_byte(2, 0xED);
    bus.poke_byte(3, 0x5E);
    cpu.step(&mut bus);
    assert_eq!(cpu.im, InterruptMode::Mode2);
}

#[test]
fn test_mlt_bc_clears_upper_adl() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = true;
    cpu.bc = 0x123456;

    // MLT BC (ED 4C)
    bus.poke_byte(0, 0xED);
    bus.poke_byte(1, 0x4C);

    cpu.step(&mut bus);

    // 0x34 * 0x56 = 0x1178, upper byte cleared by cpu_mask_mode in ADL/L mode
    assert_eq!(cpu.bc, 0x001178);
}

#[test]
fn test_pea_ix_pushes_24bit_adl() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = true;
    cpu.sp = 0xD00200;
    cpu.ix = 0x123456;

    // PEA IX+2 (ED 65 02)
    bus.poke_byte(0, 0xED);
    bus.poke_byte(1, 0x65);
    bus.poke_byte(2, 0x02);

    cpu.step(&mut bus);

    assert_eq!(cpu.sp, 0xD001FD, "PEA should push 24-bit address in ADL mode");
    assert_eq!(bus.peek_byte(0xD001FD), 0x58, "Low byte on stack");
    assert_eq!(bus.peek_byte(0xD001FE), 0x34, "Middle byte on stack");
    assert_eq!(bus.peek_byte(0xD001FF), 0x12, "High byte on stack");
}

#[test]
fn test_ldi() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

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
    cpu.adl = true;

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
    cpu.adl = true;

    cpu.hl = 0xD00100;
    cpu.de = 0xD00200;
    cpu.bc = 0x000003;
    bus.poke_byte(0xD00100, 0x11);
    bus.poke_byte(0xD00101, 0x22);
    bus.poke_byte(0xD00102, 0x33);

    // LDIR (ED B0)
    bus.poke_byte(0, 0xED);
    bus.poke_byte(1, 0xB0);

    // LDIR executes all iterations in a single step (matches CEmu behavior)
    let cycles = cpu.step(&mut bus);

    // All 3 bytes copied in one step
    assert_eq!(cpu.bc, 0x000000);
    assert_eq!(cpu.pc, 2); // Done, advances
    assert_eq!(cpu.hl, 0xD00103); // Source pointer advanced
    assert_eq!(cpu.de, 0xD00203); // Dest pointer advanced

    // Cycles: 2 flash fetches (ED B0) + 3 iterations * (RAM read + RAM write + internal)
    // = 10+10 + 3*(4+2+1) = 20 + 21 = 41
    // CEmu: cpu.cycles += internalCycles (1 per iteration) for block instructions
    assert_eq!(cycles, 41);

    // Check memory was copied
    assert_eq!(bus.peek_byte(0xD00200), 0x11);
    assert_eq!(bus.peek_byte(0xD00201), 0x22);
    assert_eq!(bus.peek_byte(0xD00202), 0x33);
}

#[test]
fn test_cpi() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

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
    cpu.adl = true;

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
    cpu.adl = true;

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
    cpu.adl = true;

    // LD IX,0x123456 (DD 21 56 34 12)
    bus.poke_byte(0, 0xDD);
    bus.poke_byte(1, 0x21);
    bus.poke_byte(2, 0x56);
    bus.poke_byte(3, 0x34);
    bus.poke_byte(4, 0x12);

    step_full(&mut cpu, &mut bus);
    assert_eq!(cpu.ix, 0x123456);
}

#[test]
fn test_ld_iy_imm() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    // LD IY,0xABCDEF (FD 21 EF CD AB)
    bus.poke_byte(0, 0xFD);
    bus.poke_byte(1, 0x21);
    bus.poke_byte(2, 0xEF);
    bus.poke_byte(3, 0xCD);
    bus.poke_byte(4, 0xAB);

    step_full(&mut cpu, &mut bus);
    assert_eq!(cpu.iy, 0xABCDEF);
}

#[test]
fn test_ld_c_ix_negative_displacement() {
    // Test LD C, (IX-5) with negative displacement
    // This is used in TI-OS format routine at 0x84B6B: DD 4E FB
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.ix = 0xD00100;  // IX points to some address
    bus.poke_byte(0xD000FB, 0x42);  // Value at IX-5 (0xD00100 - 5 = 0xD000FB)

    // LD C, (IX-5) (DD 4E FB) where FB is -5 in signed byte
    bus.poke_byte(0, 0xDD);
    bus.poke_byte(1, 0x4E);  // LD C, (IX+d)
    bus.poke_byte(2, 0xFB);  // d = -5 (0xFB = 251 unsigned, or -5 signed)

    step_full(&mut cpu, &mut bus);  // Use step_full for DD-prefixed instructions

    assert_eq!(cpu.c(), 0x42,
        "LD C, (IX-5) should load from IX-5 = 0xD000FB");
}

#[test]
fn test_ld_indexed_mem() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.ix = 0xD00100;
    cpu.a = 0x42;

    // LD (IX+5),A (DD 77 05)
    bus.poke_byte(0, 0xDD);
    bus.poke_byte(1, 0x77);
    bus.poke_byte(2, 0x05);

    step_full(&mut cpu, &mut bus);
    assert_eq!(bus.peek_byte(0xD00105), 0x42);
}

#[test]
fn test_ld_from_indexed_mem() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.ix = 0xD00100;
    bus.poke_byte(0xD00105, 0x55);

    // LD A,(IX+5) (DD 7E 05)
    bus.poke_byte(0, 0xDD);
    bus.poke_byte(1, 0x7E);
    bus.poke_byte(2, 0x05);

    step_full(&mut cpu, &mut bus);
    assert_eq!(cpu.a, 0x55);
}

#[test]
fn test_indexed_negative_offset() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.iy = 0xD00110;
    bus.poke_byte(0xD00100, 0x77);

    // LD A,(IY-16) (FD 7E F0) where F0 = -16
    bus.poke_byte(0, 0xFD);
    bus.poke_byte(1, 0x7E);
    bus.poke_byte(2, 0xF0); // -16 as signed byte

    step_full(&mut cpu, &mut bus);
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

    step_full(&mut cpu, &mut bus);
    assert_eq!(cpu.ix, 0x001234);
}

// NOTE: The following two tests are disabled because F3/F5 (undocumented flags)
// behavior for ADD IX/IY,rr is not fully understood. CEmu preserves F3/F5 from
// the previous F register value rather than deriving them from the result.
// The TI-84 CE OS boots and runs correctly without matching this exact behavior,
// so these tests are commented out until we have a definitive reference.
// See findings.md "SBC/ADC HL,rr Preserves F3/F5" for related discussion.

// #[test]
// fn test_add_ix_bc_flags_f3_f5_adl() {
//     let mut cpu = Cpu::new();
//     let mut bus = Bus::new();
//     cpu.adl = true;
//
//     // ADL mode: F3/F5 from high byte of 24-bit result
//     cpu.ix = 0x280000;
//     cpu.bc = 0x000100;
//     cpu.f = 0;
//
//     // ADD IX,BC (DD 09)
//     bus.poke_byte(0, 0xDD);
//     bus.poke_byte(1, 0x09);
//
//     step_full(&mut cpu, &mut bus);
//
//     assert_eq!(cpu.ix, 0x280100);
//     assert_eq!(
//         cpu.f & (flags::F5 | flags::F3),
//         flags::F5 | flags::F3,
//         "ADL: F3/F5 should match high byte (0x28)"
//     );
// }

// #[test]
// fn test_add_iy_iy_flags_f3_f5_z80() {
//     let mut cpu = Cpu::new();
//     let mut bus = Bus::new();
//
//     cpu.adl = false;
//     cpu.mbase = 0x00; // Clear MBASE so PC=0 reads from address 0
//     cpu.iy = 0x1000;
//     cpu.f = flags::F5 | flags::F3; // ensure flags get overwritten
//
//     // ADD IY,IY (FD 29)
//     bus.poke_byte(0, 0xFD);
//     bus.poke_byte(1, 0x29);
//
//     step_full(&mut cpu, &mut bus);
//
//     assert_eq!(cpu.iy, 0x2000);
//     assert_eq!(
//         cpu.f & (flags::F5 | flags::F3),
//         flags::F5,
//         "Z80: F3/F5 should match high byte (0x20)"
//     );
// }

#[test]
fn test_inc_ix() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.ix = 0x00FFFF;

    // INC IX (DD 23)
    bus.poke_byte(0, 0xDD);
    bus.poke_byte(1, 0x23);

    step_full(&mut cpu, &mut bus);
    assert_eq!(cpu.ix, 0x010000);
}

#[test]
fn test_push_pop_ix() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.sp = 0xD00200;
    cpu.ix = 0x123456;

    // PUSH IX (DD E5)
    bus.poke_byte(0, 0xDD);
    bus.poke_byte(1, 0xE5);
    step_full(&mut cpu, &mut bus);
    assert_eq!(cpu.pc, 2);

    // Change IX
    cpu.ix = 0;

    // POP IX (DD E1) - at position 2 (where PC is now)
    bus.poke_byte(2, 0xDD);
    bus.poke_byte(3, 0xE1);
    step_full(&mut cpu, &mut bus);

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
    cpu.init_prefetch(&mut bus); // Load first byte into prefetch

    step_full(&mut cpu, &mut bus);
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

    step_full(&mut cpu, &mut bus);
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

    step_full(&mut cpu, &mut bus);
    assert!(!cpu.flag_z()); // Bit 7 is set
}

#[test]
fn test_indexed_cb_set() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.ix = 0xD00100;
    bus.poke_byte(0xD00105, 0x00);

    // SET 7,(IX+5) (DD CB 05 FE)
    bus.poke_byte(0, 0xDD);
    bus.poke_byte(1, 0xCB);
    bus.poke_byte(2, 0x05);
    bus.poke_byte(3, 0xFE);

    step_full(&mut cpu, &mut bus);
    assert_eq!(bus.peek_byte(0xD00105), 0x80);
}

#[test]
fn test_set_1_iy_indexed_format_flag() {
    // Test SET 1, (IY+0x0C) - used by TI-OS format routine at 0x84BE9
    // This sets the "decimal point written" flag
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.iy = 0xD00080;  // IY points to TI-OS flags area
    bus.poke_byte(0xD0008C, 0x20);  // Initial value: 0x20 (bit 1 clear)

    // SET 1, (IY+0x0C) (FD CB 0C CE)
    bus.poke_byte(0, 0xFD);
    bus.poke_byte(1, 0xCB);
    bus.poke_byte(2, 0x0C);
    bus.poke_byte(3, 0xCE);

    step_full(&mut cpu, &mut bus);

    // After SET 1, bit 1 should be set: 0x20 | 0x02 = 0x22
    assert_eq!(bus.peek_byte(0xD0008C), 0x22,
        "SET 1, (IY+0x0C) should set bit 1, changing 0x20 to 0x22");
}

#[test]
fn test_indexed_cb_res() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.iy = 0xD00100;
    bus.poke_byte(0xD00105, 0xFF);

    // RES 0,(IY+5) (FD CB 05 86)
    bus.poke_byte(0, 0xFD);
    bus.poke_byte(1, 0xCB);
    bus.poke_byte(2, 0x05);
    bus.poke_byte(3, 0x86);

    step_full(&mut cpu, &mut bus);
    assert_eq!(bus.peek_byte(0xD00105), 0xFE);
}

#[test]
fn test_inc_indexed_mem() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.ix = 0xD00100;
    bus.poke_byte(0xD00105, 0x41);

    // INC (IX+5) (DD 34 05)
    bus.poke_byte(0, 0xDD);
    bus.poke_byte(1, 0x34);
    bus.poke_byte(2, 0x05);

    step_full(&mut cpu, &mut bus);
    assert_eq!(bus.peek_byte(0xD00105), 0x42);
}

#[test]
fn test_add_a_indexed_mem() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.a = 0x10;
    cpu.ix = 0xD00100;
    bus.poke_byte(0xD00105, 0x05);

    // ADD A,(IX+5) (DD 86 05)
    bus.poke_byte(0, 0xDD);
    bus.poke_byte(1, 0x86);
    bus.poke_byte(2, 0x05);

    step_full(&mut cpu, &mut bus);
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
    cpu.adl = true;

    cpu.hl = 0xD00100;
    bus.poke_byte(0xD00100, 0x42);

    // LD A,(HL) (opcode 0x7E)
    bus.poke_byte(0, 0x7E);
    let cycles = cpu.step(&mut bus);

    assert_eq!(cpu.a, 0x42, "LD A,(HL) should read value from memory");
    // Cycles: 1 flash fetch (10) + 1 RAM read (4) + 1 internal (HL) operand cycle = 15
    // CEmu: cpu.cycles += z == 6 || y == 6 for (HL) operand
    assert_eq!(
        cycles, 15,
        "LD A,(HL) should take 15 cycles (flash fetch + RAM read + (HL) operand)"
    );
}

#[test]
fn test_add_a_hl_uses_read_byte() {
    // Verify ADD A,(HL) properly reads from memory
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.a = 0x10;
    cpu.hl = 0xD00100;
    bus.poke_byte(0xD00100, 0x05);

    // ADD A,(HL) (opcode 0x86)
    bus.poke_byte(0, 0x86);
    let cycles = cpu.step(&mut bus);

    assert_eq!(cpu.a, 0x15, "ADD A,(HL) should add value from memory");
    // Cycles: 1 flash fetch (10) + 1 RAM read (4) + 1 internal (HL) operand cycle = 15
    // CEmu: cpu.cycles += z == 6 for (HL) operand in ALU operations
    assert_eq!(cycles, 15, "ADD A,(HL) should take 15 cycles (flash fetch + RAM read + (HL) operand)");
}

#[test]
fn test_inc_indexed_mem_r_register() {
    // Bug fixed: INC (IX+d) no longer double-fetches the displacement
    // Note: Our implementation increments R on every fetch_byte call (3 times: DD, opcode, displacement)
    // Strict Z80: R should only increment on M1 cycles (opcode fetches), so should be 2
    // This test verifies the fix doesn't cause extra R increments beyond the fetch calls
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.ix = 0xD00100;
    cpu.r = 0;
    bus.poke_byte(0xD00105, 0x41);

    // INC (IX+5) (DD 34 05)
    bus.poke_byte(0, 0xDD);
    bus.poke_byte(1, 0x34);
    bus.poke_byte(2, 0x05);

    step_full(&mut cpu, &mut bus);

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
    cpu.adl = true;

    cpu.iy = 0xD00100;
    cpu.r = 0;
    bus.poke_byte(0xD00103, 0x42);

    // DEC (IY+3) (FD 35 03)
    bus.poke_byte(0, 0xFD);
    bus.poke_byte(1, 0x35);
    bus.poke_byte(2, 0x03);

    step_full(&mut cpu, &mut bus);

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
fn test_push_pop_af_adl() {
    // In ADL mode, CEmu pushes/pops 3 bytes for AF (upper byte is 0)
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = true;
    cpu.sp = 0xD00200;
    cpu.a = 0xAB;
    cpu.f = 0xCD;

    // PUSH AF (opcode 0xF5)
    bus.poke_byte(0, 0xF5);
    cpu.step(&mut bus);

    // SP should decrease by 3 for AF in ADL mode (CEmu behavior)
    assert_eq!(cpu.sp, 0xD001FD, "SP should decrease by 3 for AF in ADL mode");

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
fn test_push_bc_uses_l_mode_with_suffix() {
    // Suffix can force L=0 even when ADL=1; PUSH must follow L for stack width.
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = true;
    cpu.mbase = 0xD0;
    cpu.sp = 0xD00200;
    cpu.bc = 0x112233;

    // Pre-fill the byte that would be overwritten by a 24-bit push
    bus.poke_byte(0xD001FD, 0xAA);

    // .SIS (0x40) -> L=0, IL=0 for next instruction
    // PUSH BC (0xC5)
    bus.poke_byte(0, 0x40);
    bus.poke_byte(1, 0xC5);

    cpu.step(&mut bus); // execute suffix + PUSH BC atomically (L=0 from suffix)

    // SP should decrease by 2 (16-bit stack) and only low 16 bits should be pushed.
    assert_eq!(cpu.sp & 0xFFFF, 0x01FE, "SP low 16 bits should decrease by 2");
    assert_eq!(bus.peek_byte(0xD001FE), 0x33, "Low byte on stack");
    assert_eq!(bus.peek_byte(0xD001FF), 0x22, "High byte on stack");
    assert_eq!(bus.peek_byte(0xD001FD), 0xAA, "Upper byte should remain untouched");
}

#[test]
fn test_ld_hl_nn_uses_l_mode_with_suffix() {
    // When L=0, LD HL,(nn) should read only 16 bits and clear upper byte.
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = true;
    cpu.mbase = 0xD0;
    cpu.hl = 0xAA0000; // ensure upper byte would be visible if not cleared

    // Place data at 0xD00200
    bus.poke_byte(0xD00200, 0x34);
    bus.poke_byte(0xD00201, 0x12);
    bus.poke_byte(0xD00202, 0x99);

    // .SIS (0x40) -> L=0, IL=0 for next instruction
    // LD HL,(0x0200) (opcode 0x2A, addr low/high)
    bus.poke_byte(0, 0x40);
    bus.poke_byte(1, 0x2A);
    bus.poke_byte(2, 0x00);
    bus.poke_byte(3, 0x02);

    cpu.step(&mut bus); // execute suffix + LD HL,(nn) atomically

    assert_eq!(cpu.hl, 0x00001234, "HL should load 16-bit value and clear upper byte");
}

#[test]
fn test_push_pop_bc_16bit_z80_mode() {
    // In Z80 mode (ADL=false), push/pop should be 16-bit
    // Note: In Z80 mode, addresses combine MBASE with 16-bit offset
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = false;
    // Use MBASE=0xD0 to map into RAM region
    cpu.mbase = 0xD0;
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
    step_full(&mut cpu, &mut bus);

    assert_eq!(cpu.sp, 0xD001FD, "SP should decrease by 3 for DD PUSH BC");

    cpu.bc = 0;

    // DD POP BC (DD C1)
    bus.poke_byte(2, 0xDD);
    bus.poke_byte(3, 0xC1);
    step_full(&mut cpu, &mut bus);

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
    cpu.adl = true;

    cpu.hl = 0xD00100;
    bus.poke_byte(0xD00100, 0x80); // Bit 7 set

    // BIT 7,(HL) (CB 7E)
    bus.poke_byte(0, 0xCB);
    bus.poke_byte(1, 0x7E);
    let cycles = cpu.step(&mut bus);

    assert!(!cpu.flag_z(), "Z flag should be clear (bit 7 is set)");
    // Cycles: 2 flash fetches (CB, 7E) + 1 RAM read = 10+10+4 = 24
    assert_eq!(cycles, 24, "BIT n,(HL) should take 24 cycles (2 flash fetches + RAM read)");
}

#[test]
fn test_ld_hl_indirect_memory_read() {
    // LD r,(HL) variants should all properly read from memory
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

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
    cpu.adl = true;

    cpu.hl = 0xD00100;
    bus.poke_byte(0xD00100, 0x41);

    // INC (HL) (opcode 0x34)
    bus.poke_byte(0, 0x34);
    let cycles = cpu.step(&mut bus);

    assert_eq!(bus.peek_byte(0xD00100), 0x42);
    // Cycles: 1 flash fetch (10) + 1 RAM read (4) + 1 internal (HL) cycle + 1 RAM write (2) = 17
    // CEmu: cpu.cycles += context.y == 6 for (HL) operand in INC/DEC
    assert_eq!(cycles, 17, "INC (HL) should take 17 cycles (flash fetch + RAM read + (HL) operand + RAM write)");
}

#[test]
fn test_dec_hl_indirect_cycle_count() {
    // DEC (HL) should have proper cycle count
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.hl = 0xD00100;
    bus.poke_byte(0xD00100, 0x42);

    // DEC (HL) (opcode 0x35)
    bus.poke_byte(0, 0x35);
    let cycles = cpu.step(&mut bus);

    assert_eq!(bus.peek_byte(0xD00100), 0x41);
    // Cycles: 1 flash fetch (10) + 1 RAM read (4) + 1 internal (HL) cycle + 1 RAM write (2) = 17
    // CEmu: cpu.cycles += context.y == 6 for (HL) operand in INC/DEC
    assert_eq!(cycles, 17, "DEC (HL) should take 17 cycles (flash fetch + RAM read + (HL) operand + RAM write)");
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
fn test_decimal_point_sequence() {
    // Test the exact sequence from TI-OS format routine at ROM 0x84bd8:
    // LD A, C (79); DEC C (0D); OR A (B7); RET NZ (C0); LD A, 0x2E (3E 2E); LD (DE), A (12)
    // When C=0, the decimal point should be written to (DE)
    // When C!=0, the routine should return without writing
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    // Setup: create a subroutine call so we can test RET NZ
    cpu.sp = 0xD00100;

    // First, CALL the test subroutine
    bus.poke_byte(0, 0xCD);       // CALL 0x001000
    bus.poke_byte(1, 0x00);
    bus.poke_byte(2, 0x10);
    bus.poke_byte(3, 0x00);

    // The test subroutine at 0x001000:
    // 79 = LD A, C
    // 0D = DEC C
    // B7 = OR A
    // C0 = RET NZ
    // 3E 2E = LD A, 0x2E
    // 12 = LD (DE), A
    // C9 = RET
    bus.poke_byte(0x001000, 0x79); // LD A, C
    bus.poke_byte(0x001001, 0x0D); // DEC C
    bus.poke_byte(0x001002, 0xB7); // OR A
    bus.poke_byte(0x001003, 0xC0); // RET NZ
    bus.poke_byte(0x001004, 0x3E); // LD A, 0x2E
    bus.poke_byte(0x001005, 0x2E);
    bus.poke_byte(0x001006, 0x12); // LD (DE), A
    bus.poke_byte(0x001007, 0xC9); // RET

    // Test Case 1: C = 0 should write decimal point
    cpu.set_c(0);
    cpu.de = 0xD01234;
    bus.poke_byte(0xD01234, 0x00); // Clear target
    cpu.step(&mut bus);  // CALL 0x001000
    assert_eq!(cpu.pc, 0x001000);

    // Execute the subroutine
    cpu.step(&mut bus);  // LD A, C (A = 0)
    assert_eq!(cpu.a, 0, "A should be 0 after LD A, C with C=0");

    cpu.step(&mut bus);  // DEC C (C = 0xFF because of wrap)
    assert_eq!(cpu.c(), 0xFF, "C should wrap to 0xFF after DEC C from 0");

    cpu.step(&mut bus);  // OR A (Z flag should be SET because A=0)
    assert!(cpu.flag_z(), "Z flag should be SET after OR A with A=0");

    cpu.step(&mut bus);  // RET NZ - should NOT return because Z is SET
    assert_eq!(cpu.pc, 0x001004, "RET NZ should NOT return when Z is set; PC should be 0x001004");

    cpu.step(&mut bus);  // LD A, 0x2E
    assert_eq!(cpu.a, 0x2E, "A should be 0x2E (decimal point)");

    cpu.step(&mut bus);  // LD (DE), A
    assert_eq!(bus.peek_byte(0xD01234), 0x2E, "Decimal point should be written to (DE)");

    // Test Case 2: C = 1 should NOT write decimal point (returns early)
    cpu.pc = 0;
    cpu.sp = 0xD00100;
    cpu.set_c(1);
    cpu.de = 0xD01235;
    bus.poke_byte(0xD01235, 0x00); // Clear target
    cpu.step(&mut bus);  // CALL 0x001000

    cpu.step(&mut bus);  // LD A, C (A = 1)
    assert_eq!(cpu.a, 1);

    cpu.step(&mut bus);  // DEC C (C = 0)
    assert_eq!(cpu.c(), 0);

    cpu.step(&mut bus);  // OR A (Z flag should be CLEAR because A=1)
    assert!(!cpu.flag_z(), "Z flag should be CLEAR after OR A with A=1");

    cpu.step(&mut bus);  // RET NZ - should return because Z is CLEAR
    assert_eq!(cpu.pc, 4, "RET NZ should return when Z is clear; PC should be 4");
    assert_eq!(bus.peek_byte(0xD01235), 0x00, "Decimal point should NOT be written when C!=0");
}

#[test]
fn test_or_a_self_zero_flag() {
    // OR A (0xB7) with A=0 should set Z flag
    // This is commonly used in format routines: OR A; JR Z, skip_zero
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // Test 1: OR A when A=0 should set Z
    cpu.a = 0x00;
    cpu.f = 0; // Clear all flags
    bus.poke_byte(0, 0xB7); // OR A
    cpu.pc = 0;
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x00, "OR A: A should remain 0");
    assert!(cpu.flag_z(), "OR A with A=0: Z flag MUST be set");
    assert!(!cpu.flag_c(), "OR A: C should be clear");
    assert!(!cpu.flag_n(), "OR A: N should be clear");
    assert!(!cpu.flag_h(), "OR A: H should be clear");

    // Test 2: OR A when A=0x30 ('0' in ASCII) should NOT set Z
    cpu.a = 0x30;
    cpu.f = flags::Z; // Pre-set Z to verify it gets cleared
    bus.poke_byte(1, 0xB7); // OR A
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x30, "OR A: A should remain 0x30");
    assert!(!cpu.flag_z(), "OR A with A=0x30: Z flag must NOT be set");

    // Test 3: OR A when A=1 should NOT set Z
    cpu.a = 0x01;
    cpu.f = flags::Z;
    bus.poke_byte(2, 0xB7); // OR A
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x01);
    assert!(!cpu.flag_z(), "OR A with A=1: Z flag must NOT be set");
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

// ========== eZ80 Suffix Opcode Tests ==========
// These tests verify that suffix opcodes (.SIS, .LIS, .SIL, .LIL) correctly
// change the L mode independently from ADL mode.
//
// Suffix opcodes:
//   0x40 (.SIS) - ADL=false, L=false (Z80 mode, 16-bit data)
//   0x49 (.LIS) - ADL=false, L=true  (Z80 mode, 24-bit data)
//   0x52 (.SIL) - ADL=true,  L=false (ADL mode, 16-bit data)
//   0x5B (.LIL) - ADL=true,  L=true  (ADL mode, 24-bit data)
//
// Critical: Block instructions (LDIR, etc.) must use L mode for HL/DE/BC masking,
// NOT ADL mode. These tests would catch the bug where wrap_pc() was used instead
// of wrap_data().

#[test]
fn test_suffix_sil_ldir_uses_16bit_registers() {
    // .SIL LDIR: ADL=true (24-bit PC) but L=false (16-bit data registers)
    // When L=false, mask_addr() uses MBASE for the upper byte
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true; // Start in ADL mode
    cpu.mbase = 0xD0; // Set MBASE to point to RAM (required for L=false data access)

    // Use addresses within RAM - when L=false, upper byte comes from MBASE
    // So 0xD00100 with L=false = MBASE:0x0100 = 0xD00100
    cpu.hl = 0xD00100; // Source in RAM (16-bit portion: 0x0100)
    cpu.de = 0xD00200; // Dest in RAM (16-bit portion: 0x0200)
    cpu.bc = 0x000003; // Copy 3 bytes

    // Source data
    bus.poke_byte(0xD00100, 0x11);
    bus.poke_byte(0xD00101, 0x22);
    bus.poke_byte(0xD00102, 0x33);

    // .SIL LDIR (0x52 ED B0)
    // Suffix opcode is processed as a separate step
    bus.poke_byte(0xD00000, 0x52); // .SIL prefix
    bus.poke_byte(0xD00001, 0xED);
    bus.poke_byte(0xD00002, 0xB0); // LDIR
    cpu.pc = 0xD00000;

    // Step 1: Process suffix opcode (sets L=false, IL=true)
    cpu.step(&mut bus);
    // After suffix, L and IL should be set but suffix flag is internal
    assert!(!cpu.l, ".SIL should set L=false");
    assert!(cpu.il, ".SIL should set IL=true");

    // Step 2: Process LDIR (runs all iterations in one step with L=false)
    cpu.step(&mut bus);

    // Verify data copied - with MBASE=0xD0, addresses 0x0100-0x0102 map to 0xD00100-0xD00102
    assert_eq!(bus.peek_byte(0xD00200), 0x11, "First byte copied");
    assert_eq!(bus.peek_byte(0xD00201), 0x22, "Second byte copied");
    assert_eq!(bus.peek_byte(0xD00202), 0x33, "Third byte copied");

    // BC should be 0 after LDIR completes
    assert_eq!(cpu.bc, 0, "BC should be 0 after LDIR");

    // HL and DE should have advanced by 3
    // With L=false, they should wrap at 16-bit, but since we're within range, no wrap occurs
    assert_eq!(cpu.hl & 0xFFFF, 0x0103, "HL low 16 bits should be 0x0103");
    assert_eq!(cpu.de & 0xFFFF, 0x0203, "DE low 16 bits should be 0x0203");
}

#[test]
fn test_suffix_lil_ldir_uses_24bit_registers() {
    // .LIL LDIR: Both ADL=true and L=true (24-bit everything)
    // This is the default in ADL mode, but explicit .LIL should also work
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;
    cpu.l = false; // Start with L=false to verify .LIL changes it

    // Set up 24-bit addresses
    cpu.hl = 0xD00100; // Source in RAM
    cpu.de = 0xD00200; // Dest in RAM
    cpu.bc = 0x000003;

    bus.poke_byte(0xD00100, 0x11);
    bus.poke_byte(0xD00101, 0x22);
    bus.poke_byte(0xD00102, 0x33);

    // .LIL LDIR (0x5B ED B0)
    bus.poke_byte(0xD00000, 0x5B); // .LIL prefix
    bus.poke_byte(0xD00001, 0xED);
    bus.poke_byte(0xD00002, 0xB0); // LDIR
    cpu.pc = 0xD00000;

    // Step 1: Process suffix opcode
    cpu.step(&mut bus);
    assert!(cpu.l, ".LIL should set L=true");
    assert!(cpu.il, ".LIL should set IL=true");

    // Step 2: Process LDIR (runs all iterations in one step)
    cpu.step(&mut bus);

    // Verify copy worked with 24-bit addresses
    assert_eq!(bus.peek_byte(0xD00200), 0x11);
    assert_eq!(bus.peek_byte(0xD00201), 0x22);
    assert_eq!(bus.peek_byte(0xD00202), 0x33);

    // Verify 24-bit register advancement
    assert_eq!(cpu.hl, 0xD00103, "HL should be 24-bit");
    assert_eq!(cpu.de, 0xD00203, "DE should be 24-bit");
    assert_eq!(cpu.bc, 0, "BC should be 0 after LDIR");
}

#[test]
fn test_suffix_sis_push_uses_16bit_sp() {
    // .SIS PUSH: ADL=false, L=false - should use 16-bit SP and push 2 bytes
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = false;
    cpu.mbase = 0xD0;
    cpu.l = true; // Start with L=true, suffix will change to L=false

    cpu.sp = 0x1000;
    cpu.bc = 0x123456; // Only lower 16 bits should be pushed

    // .SIS PUSH BC (0x40 C5)
    bus.poke_byte(0xD00000, 0x40); // .SIS prefix
    bus.poke_byte(0xD00001, 0xC5); // PUSH BC
    cpu.pc = 0x0000;

    // Step 1: Process suffix opcode
    cpu.step(&mut bus);
    assert!(!cpu.l, ".SIS should set L=false");
    assert!(!cpu.il, ".SIS should set IL=false");

    // Step 2: Process PUSH BC with L=false
    cpu.step(&mut bus);

    // SP should decrease by 2 (16-bit push)
    assert_eq!(cpu.sp & 0xFFFF, 0x0FFE, "SP should decrease by 2 in .SIS mode");

    // Only 2 bytes should be on stack
    assert_eq!(bus.peek_byte(0xD00FFE), 0x56, "Low byte of BC");
    assert_eq!(bus.peek_byte(0xD00FFF), 0x34, "High byte of BC");
}

#[test]
fn test_suffix_lil_push_uses_24bit_sp() {
    // .LIL PUSH: ADL=true, L=true - should use 24-bit SP and push 3 bytes
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;
    cpu.l = false; // Start with L=false, suffix will change to L=true

    cpu.sp = 0xD01000;
    cpu.bc = 0x123456; // All 3 bytes should be pushed

    // .LIL PUSH BC (0x5B C5)
    bus.poke_byte(0xD00000, 0x5B); // .LIL prefix
    bus.poke_byte(0xD00001, 0xC5); // PUSH BC
    cpu.pc = 0xD00000;

    // Step 1: Process suffix opcode
    cpu.step(&mut bus);
    assert!(cpu.l, ".LIL should set L=true");
    assert!(cpu.il, ".LIL should set IL=true");

    // Step 2: Process PUSH BC with L=true
    cpu.step(&mut bus);

    // SP should decrease by 3 (24-bit push)
    assert_eq!(cpu.sp, 0xD00FFD, "SP should decrease by 3 in .LIL mode");

    // All 3 bytes should be on stack
    assert_eq!(bus.peek_byte(0xD00FFD), 0x56, "Low byte of BC");
    assert_eq!(bus.peek_byte(0xD00FFE), 0x34, "Middle byte of BC");
    assert_eq!(bus.peek_byte(0xD00FFF), 0x12, "High byte of BC");
}

#[test]
fn test_mlt_bc_multiply() {
    // MLT BC: B * C -> BC (8-bit * 8-bit = 16-bit result)
    // This is an eZ80-only instruction
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.bc = 0x001234; // B=0x12, C=0x34
    // 0x12 * 0x34 = 18 * 52 = 936 = 0x03A8

    // MLT BC (ED 4C)
    bus.poke_byte(0xD00000, 0xED);
    bus.poke_byte(0xD00001, 0x4C);
    cpu.pc = 0xD00000;

    cpu.step(&mut bus);

    assert_eq!(cpu.bc & 0xFFFF, 0x03A8, "MLT BC: 0x12 * 0x34 = 0x03A8");
}

#[test]
fn test_mlt_de_multiply() {
    // MLT DE: D * E -> DE
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.de = 0x00FF10; // D=0xFF, E=0x10
    // 0xFF * 0x10 = 255 * 16 = 4080 = 0x0FF0

    // MLT DE (ED 5C)
    bus.poke_byte(0xD00000, 0xED);
    bus.poke_byte(0xD00001, 0x5C);
    cpu.pc = 0xD00000;

    cpu.step(&mut bus);

    assert_eq!(cpu.de & 0xFFFF, 0x0FF0, "MLT DE: 0xFF * 0x10 = 0x0FF0");
}

#[test]
fn test_mlt_hl_multiply() {
    // MLT HL: H * L -> HL
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.hl = 0x00FFFF; // H=0xFF, L=0xFF
    // 0xFF * 0xFF = 255 * 255 = 65025 = 0xFE01

    // MLT HL (ED 6C)
    bus.poke_byte(0xD00000, 0xED);
    bus.poke_byte(0xD00001, 0x6C);
    cpu.pc = 0xD00000;

    cpu.step(&mut bus);

    assert_eq!(cpu.hl & 0xFFFF, 0xFE01, "MLT HL: 0xFF * 0xFF = 0xFE01");
}

#[test]
fn test_mlt_sp_multiply() {
    // MLT SP: SPH * SPL -> SP (lower 16 bits)
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    cpu.sp = 0xD00A05; // SPH (bits 15-8) = 0x0A, SPL (bits 7-0) = 0x05
    // 0x0A * 0x05 = 10 * 5 = 50 = 0x0032

    // MLT SP (ED 7C)
    bus.poke_byte(0xD00000, 0xED);
    bus.poke_byte(0xD00001, 0x7C);
    cpu.pc = 0xD00000;

    cpu.step(&mut bus);

    // Lower 16 bits of SP should be the product
    assert_eq!(cpu.sp & 0xFFFF, 0x0032, "MLT SP: 0x0A * 0x05 = 0x0032");
}

#[test]
fn test_suffix_sil_add_hl_bc_uses_16bit() {
    // .SIL ADD HL,BC: ADL=true but L=false - should use 16-bit arithmetic
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    cpu.adl = true;

    // Set values that would overflow 16-bit but not 24-bit
    cpu.hl = 0x00FFFF;
    cpu.bc = 0x000002;

    // .SIL ADD HL,BC (0x52 09)
    bus.poke_byte(0xD00000, 0x52); // .SIL prefix
    bus.poke_byte(0xD00001, 0x09); // ADD HL,BC
    cpu.pc = 0xD00000;

    // Step 1: Process suffix opcode
    cpu.step(&mut bus);
    assert!(!cpu.l, ".SIL should set L=false");

    // Step 2: Process ADD HL,BC with L=false
    cpu.step(&mut bus);

    // With 16-bit arithmetic: 0xFFFF + 2 = 0x0001 (with carry)
    assert_eq!(cpu.hl & 0xFFFF, 0x0001, "ADD HL,BC should wrap at 16-bit");
    assert!(cpu.flag_c(), "Carry should be set from 16-bit overflow");
}

#[test]
fn test_ld_ix_d_l_uses_l_not_ixl() {
    // Test that LD (IX+d), L writes the L register, NOT IXL
    // This is a common bug: DD prefix should NOT substitute L->IXL for memory operations
    // Bug discovery: TI-OS format routine at 0x084AA0 uses DD 75 FB = LD (IX-5), L
    // With the bug, it was writing IXL (0x1D=29) instead of L (0x01)
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = true;
    cpu.ix = 0xD00100;  // IX = 0xD00100, so IXL = 0x00
    cpu.hl = 0xD00042;  // HL = 0xD00042, so L = 0x42

    // Verify IXL and L are different
    assert_eq!(cpu.ixl(), 0x00, "IXL should be 0x00");
    assert_eq!(cpu.l(), 0x42, "L should be 0x42");

    // Set up DD 75 FB = LD (IX-5), L at address 0
    bus.poke_byte(0, 0xDD);       // IX prefix
    bus.poke_byte(1, 0x75);       // LD (IX+d), L opcode
    bus.poke_byte(2, 0xFB);       // d = -5 (signed)

    // Execute with step_full to handle DD prefix properly
    step_full(&mut cpu, &mut bus);

    // The value at IX-5 (0xD000FB) should be L (0x42), not IXL (0x00)
    let written_value = bus.read_byte(0xD000FB);
    assert_eq!(written_value, 0x42,
        "LD (IX-5), L should write L (0x42), not IXL (0x00). Got: 0x{:02X}",
        written_value);
}

#[test]
fn test_ld_r_ix_d_uses_original_register() {
    // Test that LD L, (IX+d) writes to the L register, NOT IXL
    // The DD prefix should NOT substitute L->IXL for memory operations
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = true;
    cpu.ix = 0xD00100;
    cpu.hl = 0xD00000;  // L = 0x00 initially

    // Put test value at IX+5
    bus.poke_byte(0xD00105, 0x99);

    // Set up DD 6E 05 = LD L, (IX+5)
    bus.poke_byte(0, 0xDD);       // IX prefix
    bus.poke_byte(1, 0x6E);       // LD L, (IX+d) opcode
    bus.poke_byte(2, 0x05);       // d = +5

    step_full(&mut cpu, &mut bus);

    // L should now be 0x99, and IXL should be unchanged
    assert_eq!(cpu.l(), 0x99, "LD L, (IX+5) should load into L register");
    assert_eq!(cpu.ixl(), 0x00, "IXL should be unchanged (still 0x00)");
}

#[test]
fn test_ld_ixl_r_does_substitute() {
    // Test that LD IXL, B does use IXL (NOT L) - substitution IS correct here
    // because neither operand is a memory reference
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.adl = true;
    cpu.ix = 0xD00100;  // IXL = 0x00
    cpu.hl = 0xD00099;  // L = 0x99
    cpu.set_b(0x42);

    // Set up DD 68 = LD IXL, B
    bus.poke_byte(0, 0xDD);       // IX prefix
    bus.poke_byte(1, 0x68);       // LD L, B -> becomes LD IXL, B with DD prefix

    step_full(&mut cpu, &mut bus);

    // IXL should be 0x42, L should be unchanged
    assert_eq!(cpu.ixl(), 0x42, "LD IXL, B should write to IXL");
    assert_eq!(cpu.l(), 0x99, "L should be unchanged");
}
