//! Detailed boot trace - shows port accesses and helps identify missing hardware

use std::fs;
use std::path::Path;

use emu_core::Emu;

fn main() {
    // Try to find the ROM file
    let rom_paths = ["TI-84 CE.rom", "../TI-84 CE.rom"];

    let mut rom_data = None;
    for path in &rom_paths {
        if Path::new(path).exists() {
            println!("Found ROM at: {}", path);
            match fs::read(path) {
                Ok(data) => {
                    rom_data = Some(data);
                    break;
                }
                Err(e) => {
                    eprintln!("Failed to read ROM: {}", e);
                }
            }
        }
    }

    let rom_data = match rom_data {
        Some(data) => data,
        None => {
            eprintln!("No ROM file found. Place 'TI-84 CE.rom' in the project root.");
            return;
        }
    };

    println!("ROM size: {} bytes", rom_data.len());

    let mut emu = Emu::new();
    emu.load_rom(&rom_data).expect("Failed to load ROM");

    println!("\nTracing boot sequence...\n");

    // Run more cycles to see what happens
    let executed = emu.run_cycles(10000);

    println!("Cycles executed: {}", executed);
    println!("Stop reason: {:?}", emu.last_stop_reason());
    println!("\n{}", emu.dump_registers());

    // Dump memory around the HALT location to see what code does
    println!("\nCode around PC=0x001414:");
    for addr in (0x001400..0x001430).step_by(16) {
        print!("{:06X}: ", addr);
        for i in 0..16 {
            print!("{:02X} ", emu.peek_byte(addr + i));
        }
        println!();
    }

    // Dump code at the IN0 check location
    println!("\nCode around PC=0x000C2E (IN0 check):");
    for addr in (0x000C20..0x000C40).step_by(16) {
        print!("{:06X}: ", addr);
        for i in 0..16 {
            print!("{:02X} ", emu.peek_byte(addr + i));
        }
        println!();
    }

    // Dump code around the second check location (0x001680-0x0016A0)
    println!("\nCode around PC=0x001680 (second check):");
    for addr in (0x001670..0x0016B0).step_by(16) {
        print!("{:06X}: ", addr);
        for i in 0..16 {
            print!("{:02X} ", emu.peek_byte(addr + i));
        }
        println!();
    }

    // Check key peripheral registers
    println!("\nPeripheral Status:");

    // Interrupt controller (0xF00000)
    println!("  Interrupt latch:   {:02X}", emu.peek_byte(0xF00000));
    println!("  Interrupt raw:     {:02X}", emu.peek_byte(0xF00004));
    println!("  Interrupt enabled: {:02X}", emu.peek_byte(0xF00008));

    // LCD controller (0xE30000)
    println!("  LCD control:       {:02X}", emu.peek_byte(0xE30010));
    println!(
        "  LCD upbase:        {:06X}",
        emu.peek_byte(0xE3001C) as u32
            | ((emu.peek_byte(0xE3001D) as u32) << 8)
            | ((emu.peek_byte(0xE3001E) as u32) << 16)
    );

    // Control ports
    println!("\n  Control Ports (0xE000xx / 0xFF00xx):");
    println!("    Port 0x00 (power):  {:02X}", emu.peek_byte(0xE00000));
    println!("    Port 0x01 (speed):  {:02X}", emu.peek_byte(0xE00001));
    println!("    Port 0x02 (battery): {:02X}", emu.peek_byte(0xE00002));
    println!("    Port 0x03 (device):  {:02X}", emu.peek_byte(0xE00003));
    println!("    Port 0x05 (ctrl):   {:02X}", emu.peek_byte(0xE00005));
    println!("    Port 0x06 (unlock): {:02X}", emu.peek_byte(0xE00006));
    println!("    Port 0x08 (fixed):  {:02X}", emu.peek_byte(0xE00008));
    println!("    Port 0x09 (panel):  {:02X}", emu.peek_byte(0xE00009));
    println!("    Port 0x0D (lcd en): {:02X}", emu.peek_byte(0xE0000D));
    println!("    Port 0x0F (usb):    {:02X}", emu.peek_byte(0xE0000F));
    println!("    Port 0x1C (fixed):  {:02X}", emu.peek_byte(0xE0001C));
    println!("    Port 0x28 (flash):  {:02X}", emu.peek_byte(0xE00028));

    // Flash controller (0xE10000)
    println!("\n  Flash Controller (0xE10000):");
    println!("    Port 0x00: {:02X}", emu.peek_byte(0xE10000));
    println!("    Port 0x01: {:02X}", emu.peek_byte(0xE10001));
    println!("    Port 0x02: {:02X}", emu.peek_byte(0xE10002));
    println!("    Port 0x05: {:02X}", emu.peek_byte(0xE10005));
    println!("    Port 0x08: {:02X}", emu.peek_byte(0xE10008));

    println!("\n{}", emu.dump_history());
}
