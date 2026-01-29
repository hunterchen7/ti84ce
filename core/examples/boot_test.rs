//! Boot test - loads a ROM and runs to check boot progress and screen output

use std::fs;
use std::path::Path;

// Import the emulator core
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

    println!(
        "ROM size: {} bytes ({:.2} MB)",
        rom_data.len(),
        rom_data.len() as f64 / 1024.0 / 1024.0
    );

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

    // Run in chunks and report progress
    let chunk_size = 1_000_000;
    let max_cycles = 50_000_000; // 50M cycles ~ 1 second at 48MHz
    let mut total_executed = 0u64;
    let mut last_pc = 0u32;
    let mut stuck_count = 0;

    println!("\nBooting... (up to {} cycles)", max_cycles);
    println!("{}", emu.debug_flash_status());

    while total_executed < max_cycles as u64 {
        let executed = emu.run_cycles(chunk_size);
        total_executed += executed as u64;

        let pc = emu.pc();
        let halted = emu.is_halted();

        // Check if stuck in same spot
        if pc == last_pc {
            stuck_count += 1;
            if stuck_count > 5 {
                println!(
                    "\n[{:.2}M cycles] Stuck at PC={:06X} (halted={})",
                    total_executed as f64 / 1_000_000.0,
                    pc,
                    halted
                );
                // Debug: show what port is being read
                if pc >= 0x5BA9 && pc <= 0x5BAD {
                    let port_0d = emu.control_read(0x0D);
                    let and_mask = emu.peek_byte(0x5BAC);
                    println!("  Port 0x0D = 0x{:02X}", port_0d);
                    println!("  AND mask = 0x{:02X}", and_mask);
                    println!("  Result = 0x{:02X} (loops if non-zero)", port_0d & and_mask);
                }
                break;
            }
        } else {
            stuck_count = 0;
        }
        last_pc = pc;

        // Print progress every 10M cycles
        if total_executed % 10_000_000 < chunk_size as u64 {
            println!(
                "[{:.1}M cycles] PC={:06X} SP={:06X} ADL={} halted={}",
                total_executed as f64 / 1_000_000.0,
                pc,
                emu.sp(),
                emu.adl(),
                halted
            );
        }

        // Check if we're in HALT
        if halted {
            println!(
                "\n[{:.2}M cycles] CPU halted at PC={:06X}",
                total_executed as f64 / 1_000_000.0,
                pc
            );
            println!("IFF1={} IFF2={}", emu.iff1(), emu.iff2());
            println!("IRQ pending: {}", emu.irq_pending());
            break;
        }
    }

    println!("\n=== Final State ===");
    println!("Total cycles: {} ({:.2}M)", total_executed, total_executed as f64 / 1_000_000.0);
    println!("{}", emu.dump_registers());
    println!("{}", emu.debug_flash_status());

    // Check LCD state
    let lcd = emu.lcd_snapshot();
    println!("\n=== LCD State ===");
    println!(
        "Control: {:08X} (enabled={})",
        lcd.control,
        (lcd.control & 1) != 0
    );
    println!("Upper base: {:06X}", lcd.upbase);
    println!("Lower base: {:06X}", lcd.lpbase);
    println!("Int mask: {:08X}, Int status: {:08X}", lcd.int_mask, lcd.int_status);

    // Render frame and check for non-black pixels
    emu.render_frame();
    let (width, height) = emu.framebuffer_size();
    let fb_ptr = emu.framebuffer_ptr();
    let fb = unsafe { std::slice::from_raw_parts(fb_ptr, width * height) };

    let mut non_black = 0;
    let mut sample_colors: Vec<u32> = Vec::new();
    for (i, &pixel) in fb.iter().enumerate() {
        if pixel != 0xFF000000 {
            non_black += 1;
            if sample_colors.len() < 10 && !sample_colors.contains(&pixel) {
                sample_colors.push(pixel);
            }
        }
        // Show first few rows
        if i < width * 5 && i % width == 0 {
            print!("Row {}: ", i / width);
            for j in 0..20.min(width) {
                let p = fb[i + j];
                if p == 0xFF000000 {
                    print!(".");
                } else {
                    print!("#");
                }
            }
            println!();
        }
    }

    println!("\nScreen analysis:");
    println!("  Non-black pixels: {} / {} ({:.1}%)",
        non_black,
        width * height,
        non_black as f64 / (width * height) as f64 * 100.0
    );
    if !sample_colors.is_empty() {
        println!("  Sample colors: {:08X?}", sample_colors);
    }

    println!("\n{}", emu.dump_history());
}
