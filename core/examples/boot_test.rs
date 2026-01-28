//! Boot test - loads a ROM and runs some cycles to see early boot behavior

use std::fs;
use std::path::Path;

// Import the emulator core
use emu_core::Emu;

fn main() {
    // Try to find the ROM file
    let rom_paths = [
        "TI-84 CE.rom",
        "../TI-84 CE.rom",
    ];

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

    println!("ROM size: {} bytes ({:.2} MB)", rom_data.len(), rom_data.len() as f64 / 1024.0 / 1024.0);

    // Create emulator and load ROM
    let mut emu = Emu::new();

    match emu.load_rom(&rom_data) {
        Ok(()) => println!("ROM loaded successfully"),
        Err(e) => {
            eprintln!("Failed to load ROM: error code {}", e);
            return;
        }
    }

    println!("\nInitial state:");
    println!("{}", emu.dump_registers());

    // Run some cycles
    let cycles_to_run = 1000;
    println!("\nRunning {} cycles...", cycles_to_run);

    let executed = emu.run_cycles(cycles_to_run);

    println!("\nAfter execution:");
    println!("Cycles executed: {}", executed);
    println!("Total cycles: {}", emu.total_cycles());
    println!("Stop reason: {:?}", emu.last_stop_reason());
    println!("\n{}", emu.dump_registers());

    println!("\n{}", emu.dump_history());
}
