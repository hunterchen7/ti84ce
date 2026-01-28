//! ADL mode and Z80 mode specific tests
//!
//! Tests for:
//! - ADL mode (24-bit addressing) specific behavior
//! - Z80 mode (16-bit + MBASE) backward compatibility  
//! - TI-84 CE memory map verification
//!
//! # References
//! - eZ80 CPU User Manual (Zilog UM0077)
//! - CEmu (https://github.com/CE-Programming/CEmu)

use super::*;

#[test]
fn test_adl_mode_24bit_addressing() {
    // Verify full 24-bit address space is accessible in ADL mode
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    assert!(cpu.adl, "CPU should start in ADL mode");

    // Write to high RAM addresses (>16-bit range)
    cpu.hl = 0xD00100;
    bus.poke_byte(0xD00100, 0x42);

    // LD A,(HL) should access full 24-bit address
    bus.poke_byte(0, 0x7E);
    cpu.step(&mut bus);
    assert_eq!(cpu.a, 0x42, "ADL mode should access full 24-bit address");
}

#[test]
fn test_adl_mode_24bit_arithmetic() {
    // Test 24-bit arithmetic operations
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // ADD HL,BC with 24-bit values
    cpu.hl = 0x100000;
    cpu.bc = 0x0FFFFF;

    // ADD HL,BC (09)
    bus.poke_byte(0, 0x09);
    cpu.step(&mut bus);
    assert_eq!(cpu.hl, 0x1FFFFF, "24-bit ADD HL,BC");

    // ADD HL,BC causing 24-bit wrap
    cpu.hl = 0xFFFFFF;
    cpu.bc = 0x000001;
    cpu.pc = 0;
    bus.poke_byte(0, 0x09);
    cpu.step(&mut bus);
    assert_eq!(cpu.hl, 0x000000, "24-bit wrap around");
    assert!(cpu.flag_c(), "Carry should be set on 24-bit overflow");
}

#[test]
fn test_adl_mode_adc_hl_24bit_overflow() {
    // Test ADC HL overflow detection in 24-bit mode
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // 24-bit signed overflow: 0x7FFFFF + 0x000001 = 0x800000
    // (max positive + 1 = min negative)
    cpu.hl = 0x7FFFFF;
    cpu.bc = 0x000001;
    cpu.f = 0;

    bus.poke_byte(0, 0xED);
    bus.poke_byte(1, 0x4A); // ADC HL,BC
    cpu.step(&mut bus);

    assert_eq!(cpu.hl, 0x800000);
    assert!(cpu.flag_s(), "ADC HL 24-bit overflow: S should be set");
    assert!(cpu.flag_pv(), "ADC HL 24-bit overflow: PV should be set");
    assert!(!cpu.flag_c(), "ADC HL 24-bit: no unsigned overflow");
}

#[test]
fn test_adl_mode_sbc_hl_24bit_underflow() {
    // Test SBC HL underflow detection in 24-bit mode
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // 24-bit signed underflow: 0x800000 - 0x000001 = 0x7FFFFF
    // (min negative - 1 = max positive)
    cpu.hl = 0x800000;
    cpu.bc = 0x000001;
    cpu.f = 0;

    bus.poke_byte(0, 0xED);
    bus.poke_byte(1, 0x42); // SBC HL,BC
    cpu.step(&mut bus);

    assert_eq!(cpu.hl, 0x7FFFFF);
    assert!(!cpu.flag_s(), "SBC HL 24-bit underflow: S should be clear");
    assert!(cpu.flag_pv(), "SBC HL 24-bit underflow: PV should be set");
    assert!(!cpu.flag_c(), "SBC HL 24-bit: no borrow");
}

#[test]
fn test_adl_mode_call_ret_24bit_pc() {
    // CALL/RET should preserve full 24-bit PC
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.sp = 0xD00200;
    cpu.pc = 0x100000; // High PC value

    // CALL nn (CD nn nn nn in ADL mode - 4 byte address)
    bus.poke_byte(0x100000, 0xCD);
    bus.poke_byte(0x100001, 0x56);
    bus.poke_byte(0x100002, 0x34);
    bus.poke_byte(0x100003, 0x12); // Target: 0x123456

    cpu.step(&mut bus);

    assert_eq!(cpu.pc, 0x123456, "CALL should jump to 24-bit address");
    assert_eq!(cpu.sp, 0xD001FD, "SP should decrease by 3 in ADL mode");

    // Return address on stack should be 0x100004 (after CALL instruction)
    let ret_lo = bus.peek_byte(0xD001FD);
    let ret_mid = bus.peek_byte(0xD001FE);
    let ret_hi = bus.peek_byte(0xD001FF);
    let ret_addr = (ret_hi as u32) << 16 | (ret_mid as u32) << 8 | ret_lo as u32;
    assert_eq!(ret_addr, 0x100004, "Return address should be 24-bit");

    // RET should restore 24-bit PC
    bus.poke_byte(0x123456, 0xC9); // RET
    cpu.step(&mut bus);
    assert_eq!(cpu.pc, 0x100004, "RET should restore 24-bit PC");
}

#[test]
fn test_adl_mode_ldir_24bit() {
    // LDIR should work with 24-bit addresses
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    // Copy 3 bytes from 0xD00100 to 0xD00200
    cpu.hl = 0xD00100; // Source (24-bit)
    cpu.de = 0xD00200; // Dest (24-bit)
    cpu.bc = 0x000003; // Count

    // Set up source data
    bus.poke_byte(0xD00100, 0xAA);
    bus.poke_byte(0xD00101, 0xBB);
    bus.poke_byte(0xD00102, 0xCC);

    // LDIR (ED B0)
    bus.poke_byte(0, 0xED);
    bus.poke_byte(1, 0xB0);
    cpu.pc = 0;

    // Execute until BC = 0
    while cpu.bc != 0 {
        cpu.pc = 0;
        cpu.step(&mut bus);
    }

    // Verify copy
    assert_eq!(bus.peek_byte(0xD00200), 0xAA);
    assert_eq!(bus.peek_byte(0xD00201), 0xBB);
    assert_eq!(bus.peek_byte(0xD00202), 0xCC);

    // Verify pointers advanced
    assert_eq!(cpu.hl, 0xD00103, "HL should advance by 3");
    assert_eq!(cpu.de, 0xD00203, "DE should advance by 3");
}

#[test]
fn test_ti84ce_memory_map() {
    // Test TI-84 CE specific memory regions
    let mut bus = Bus::new();

    // RAM region (0xD00000 - 0xD657FF)
    bus.poke_byte(0xD00000, 0x42);
    assert_eq!(bus.read_byte(0xD00000), 0x42, "RAM should be read/write");

    // VRAM region (0xD40000 - 0xD657FF)
    bus.poke_byte(0xD40000, 0x55);
    assert_eq!(bus.read_byte(0xD40000), 0x55, "VRAM should be read/write");

    // Flash region (0x000000 - 0x3FFFFF) - read only
    let flash_val = bus.read_byte(0x000000);
    assert_eq!(flash_val, 0xFF, "Erased flash should read 0xFF");

    // Writes to flash should be ignored
    bus.write_byte(0x000100, 0x42);
    assert_eq!(
        bus.read_byte(0x000100),
        0xFF,
        "Flash writes should be ignored"
    );
}

#[test]
fn test_adl_mode_index_registers() {
    // IX and IY should be 24-bit in ADL mode
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.ix = 0xD00100;
    bus.poke_byte(0xD00105, 0x42);

    // LD A,(IX+5) (DD 7E 05)
    bus.poke_byte(0, 0xDD);
    bus.poke_byte(1, 0x7E);
    bus.poke_byte(2, 0x05);
    cpu.step(&mut bus);

    assert_eq!(cpu.a, 0x42, "IX should use full 24-bit address");

    // Test IY similarly
    cpu.iy = 0xD00200;
    bus.poke_byte(0xD001FB, 0x77); // -5 offset

    // LD A,(IY-5) (FD 7E FB)
    bus.poke_byte(3, 0xFD);
    bus.poke_byte(4, 0x7E);
    bus.poke_byte(5, 0xFB); // -5 as signed byte
    cpu.step(&mut bus);

    assert_eq!(
        cpu.a, 0x77,
        "IY should handle negative offsets with 24-bit base"
    );
}

#[test]
fn test_adl_jp_indirect_24bit() {
    // JP (HL) should use full 24-bit HL in ADL mode
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.hl = 0x123456;

    // JP (HL) (E9)
    bus.poke_byte(0, 0xE9);
    cpu.step(&mut bus);

    assert_eq!(cpu.pc, 0x123456, "JP (HL) should jump to 24-bit address");
}

#[test]
fn test_adl_rst_pushes_24bit() {
    // RST should push 24-bit return address in ADL mode
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();

    cpu.sp = 0xD00200;
    cpu.pc = 0x123456;

    // RST 38h (FF)
    bus.poke_byte(0x123456, 0xFF);
    cpu.step(&mut bus);

    assert_eq!(cpu.pc, 0x000038, "RST 38h should jump to 0x38");
    assert_eq!(cpu.sp, 0xD001FD, "SP should decrease by 3");

    // Verify return address
    let ret_lo = bus.peek_byte(0xD001FD);
    let ret_mid = bus.peek_byte(0xD001FE);
    let ret_hi = bus.peek_byte(0xD001FF);
    let ret_addr = (ret_hi as u32) << 16 | (ret_mid as u32) << 8 | ret_lo as u32;
    assert_eq!(ret_addr, 0x123457, "RST should push 24-bit return address");
}

// ========== Z80 Mode Compatibility Tests ==========
// These tests verify backward compatibility with Z80 code (ADL=false)

/// Helper to set up CPU in Z80 mode with MBASE pointing to RAM
fn setup_z80_mode(cpu: &mut Cpu) {
    cpu.adl = false;
    cpu.mbase = 0xD0; // RAM starts at 0xD00000
    cpu.pc = 0x0100; // Typical Z80 program start
    cpu.sp = 0xFFFF; // Top of 64KB space
}

#[test]
fn test_z80_mode_jp_uses_mbase() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    // JP 0x200 (C3 00 02)
    bus.poke_byte(0xD00100, 0xC3);
    bus.poke_byte(0xD00101, 0x00);
    bus.poke_byte(0xD00102, 0x02);
    cpu.step(&mut bus);

    // Should jump to MBASE:0x0200 = 0xD00200
    assert_eq!(cpu.pc, 0x0200, "PC should be 16-bit value");
    // But actual address accessed should include MBASE
    let effective = cpu.mask_addr(cpu.pc);
    assert_eq!(
        effective, 0xD00200,
        "Effective address should include MBASE"
    );
}

#[test]
fn test_z80_mode_call_ret_16bit() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    cpu.sp = 0x1000;

    // CALL 0x200 (CD 00 02)
    bus.poke_byte(0xD00100, 0xCD);
    bus.poke_byte(0xD00101, 0x00);
    bus.poke_byte(0xD00102, 0x02);
    cpu.step(&mut bus);

    assert_eq!(cpu.pc, 0x0200, "Should jump to 0x0200");
    assert_eq!(cpu.sp, 0x0FFE, "SP should decrease by 2 in Z80 mode");

    // Check return address is 16-bit (0x0103)
    let ret_lo = bus.peek_byte(0xD00FFE);
    let ret_hi = bus.peek_byte(0xD00FFF);
    let ret_addr = (ret_hi as u16) << 8 | ret_lo as u16;
    assert_eq!(ret_addr, 0x0103, "Return address should be 16-bit");

    // RET (C9)
    bus.poke_byte(0xD00200, 0xC9);
    cpu.step(&mut bus);

    assert_eq!(cpu.pc, 0x0103, "Should return to 0x0103");
    assert_eq!(cpu.sp, 0x1000, "SP should be restored");
}

#[test]
fn test_z80_mode_push_pop_16bit() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    cpu.sp = 0x1000;
    cpu.bc = 0xABCD; // Only lower 16 bits matter in Z80 mode

    // PUSH BC (C5)
    bus.poke_byte(0xD00100, 0xC5);
    cpu.step(&mut bus);

    assert_eq!(cpu.sp, 0x0FFE, "SP should decrease by 2");
    assert_eq!(bus.peek_byte(0xD00FFE), 0xCD, "Low byte");
    assert_eq!(bus.peek_byte(0xD00FFF), 0xAB, "High byte");

    // POP DE (D1)
    bus.poke_byte(0xD00101, 0xD1);
    cpu.step(&mut bus);

    assert_eq!(cpu.sp, 0x1000, "SP should be restored");
    assert_eq!(cpu.de & 0xFFFF, 0xABCD, "DE should have popped value");
}

#[test]
fn test_z80_mode_ld_nn_a() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    cpu.a = 0x42;

    // LD (0x5000),A (32 00 50)
    bus.poke_byte(0xD00100, 0x32);
    bus.poke_byte(0xD00101, 0x00);
    bus.poke_byte(0xD00102, 0x50);
    cpu.step(&mut bus);

    // Should write to MBASE:0x5000 = 0xD05000
    assert_eq!(bus.peek_byte(0xD05000), 0x42, "Should write to MBASE+nn");
}

#[test]
fn test_z80_mode_ld_a_nn() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    bus.poke_byte(0xD05000, 0x77);

    // LD A,(0x5000) (3A 00 50)
    bus.poke_byte(0xD00100, 0x3A);
    bus.poke_byte(0xD00101, 0x00);
    bus.poke_byte(0xD00102, 0x50);
    cpu.step(&mut bus);

    assert_eq!(cpu.a, 0x77, "Should read from MBASE+nn");
}

#[test]
fn test_z80_mode_ldir_16bit_counters() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    // Source at 0xD00200, dest at 0xD00300, count = 3
    cpu.hl = 0x0200;
    cpu.de = 0x0300;
    cpu.bc = 0x0003;

    bus.poke_byte(0xD00200, 0x11);
    bus.poke_byte(0xD00201, 0x22);
    bus.poke_byte(0xD00202, 0x33);

    // LDIR (ED B0)
    bus.poke_byte(0xD00100, 0xED);
    bus.poke_byte(0xD00101, 0xB0);

    // Run until BC = 0
    while cpu.bc != 0 {
        cpu.step(&mut bus);
    }

    // Verify data copied
    assert_eq!(bus.peek_byte(0xD00300), 0x11);
    assert_eq!(bus.peek_byte(0xD00301), 0x22);
    assert_eq!(bus.peek_byte(0xD00302), 0x33);

    // BC should be 0, PV should be 0
    assert_eq!(cpu.bc, 0);
    assert!(!cpu.flag_pv(), "PV should be 0 when BC=0");

    // HL and DE should have incremented by 3 (16-bit wrap)
    assert_eq!(cpu.hl & 0xFFFF, 0x0203);
    assert_eq!(cpu.de & 0xFFFF, 0x0303);
}

#[test]
fn test_z80_mode_jr_relative() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    // JR +5 (18 05) - should jump from 0x100 to 0x107
    bus.poke_byte(0xD00100, 0x18);
    bus.poke_byte(0xD00101, 0x05);
    cpu.step(&mut bus);

    assert_eq!(cpu.pc, 0x0107, "JR should work with 16-bit PC");

    // JR -10 (18 F6) - should jump backward
    bus.poke_byte(0xD00107, 0x18);
    bus.poke_byte(0xD00108, 0xF6); // -10 in two's complement
    cpu.step(&mut bus);

    assert_eq!(cpu.pc, 0x00FF, "JR backward should wrap in 16-bit");
}

#[test]
fn test_z80_mode_djnz() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    cpu.set_b(3);

    // DJNZ -2 (10 FE) - loop back to itself
    bus.poke_byte(0xD00100, 0x10);
    bus.poke_byte(0xD00101, 0xFE);

    cpu.step(&mut bus);
    assert_eq!(cpu.b(), 2);
    assert_eq!(cpu.pc, 0x0100, "Should loop back");

    cpu.step(&mut bus);
    assert_eq!(cpu.b(), 1);
    assert_eq!(cpu.pc, 0x0100, "Should loop back");

    cpu.step(&mut bus);
    assert_eq!(cpu.b(), 0);
    assert_eq!(cpu.pc, 0x0102, "Should fall through when B=0");
}

#[test]
fn test_z80_mode_rst() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    cpu.sp = 0x1000;

    // RST 38h (FF)
    bus.poke_byte(0xD00100, 0xFF);
    cpu.step(&mut bus);

    assert_eq!(cpu.pc, 0x0038, "RST should jump to vector");
    assert_eq!(cpu.sp, 0x0FFE, "SP should decrease by 2");

    // Check 16-bit return address
    let ret_lo = bus.peek_byte(0xD00FFE);
    let ret_hi = bus.peek_byte(0xD00FFF);
    assert_eq!(ret_lo, 0x01);
    assert_eq!(ret_hi, 0x01);
}

#[test]
fn test_z80_mode_ex_sp_hl() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    cpu.sp = 0x1000;
    cpu.hl = 0x1234;
    bus.poke_byte(0xD01000, 0xCD);
    bus.poke_byte(0xD01001, 0xAB);

    // EX (SP),HL (E3)
    bus.poke_byte(0xD00100, 0xE3);
    cpu.step(&mut bus);

    // HL should have old stack value
    assert_eq!(cpu.hl & 0xFFFF, 0xABCD, "HL should have stack value");
    // Stack should have old HL
    assert_eq!(bus.peek_byte(0xD01000), 0x34);
    assert_eq!(bus.peek_byte(0xD01001), 0x12);
}

#[test]
fn test_z80_mode_add_flags() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    // Test overflow detection uses 8-bit (same as ADL mode for A)
    cpu.a = 0x7F;
    cpu.set_b(0x01);

    // ADD A,B (80)
    bus.poke_byte(0xD00100, 0x80);
    cpu.step(&mut bus);

    assert_eq!(cpu.a, 0x80);
    assert!(cpu.flag_s(), "Sign should be set");
    assert!(!cpu.flag_z(), "Zero should be clear");
    assert!(cpu.flag_pv(), "Overflow: 0x7F + 0x01 = 0x80 overflows");
    assert!(cpu.flag_h(), "Half-carry: 0xF + 1 = 0x10");
}

#[test]
fn test_z80_mode_adc_hl_16bit_overflow() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    // In Z80 mode, ADC HL uses 16-bit overflow detection
    cpu.hl = 0x7FFF;
    cpu.bc = 0x0001;
    cpu.set_flag_c(false);

    // ADC HL,BC (ED 4A)
    bus.poke_byte(0xD00100, 0xED);
    bus.poke_byte(0xD00101, 0x4A);
    cpu.step(&mut bus);

    assert_eq!(cpu.hl & 0xFFFF, 0x8000);
    assert!(cpu.flag_s(), "Sign bit 15 should be set");
    assert!(
        cpu.flag_pv(),
        "Overflow: 0x7FFF + 1 = 0x8000 overflows in 16-bit"
    );
}

#[test]
fn test_z80_mode_sbc_hl_16bit_overflow() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    // In Z80 mode, SBC HL uses 16-bit overflow detection
    cpu.hl = 0x8000;
    cpu.bc = 0x0001;
    cpu.set_flag_c(false);

    // SBC HL,BC (ED 42)
    bus.poke_byte(0xD00100, 0xED);
    bus.poke_byte(0xD00101, 0x42);
    cpu.step(&mut bus);

    assert_eq!(cpu.hl & 0xFFFF, 0x7FFF);
    assert!(!cpu.flag_s(), "Sign bit 15 should be clear");
    assert!(
        cpu.flag_pv(),
        "Overflow: 0x8000 - 1 = 0x7FFF overflows in 16-bit"
    );
}

#[test]
fn test_z80_mode_inc_dec_16bit_no_mbase() {
    let mut cpu = Cpu::new();
    let mut bus = Bus::new();
    setup_z80_mode(&mut cpu);

    // INC/DEC on register pairs should not add MBASE
    cpu.hl = 0xFFFF;

    // INC HL (23)
    bus.poke_byte(0xD00100, 0x23);
    cpu.step(&mut bus);

    // Should wrap to 0x0000, not 0xD00000
    assert_eq!(cpu.hl & 0xFFFF, 0x0000, "HL should wrap at 16-bit boundary");
}
