//! Comparison trace - outputs every instruction for parity testing with CEmu
//!
//! Usage: cargo run --release --example compare_trace [max_steps]
//!   max_steps: Number of instructions to trace (default: 50000)
//!
//! Output goes to traces/ours_<timestamp>.log

use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use chrono::Local;
use emu_core::Emu;

fn main() {
    // Parse optional max_steps argument
    let args: Vec<String> = env::args().collect();
    let max_steps: u64 = if args.len() > 1 {
        args[1].parse().unwrap_or(50_000)
    } else {
        50_000
    };

    let rom_paths = ["TI-84 CE.rom", "../TI-84 CE.rom"];

    let mut rom_data = None;
    for path in &rom_paths {
        if Path::new(path).exists() {
            if let Ok(data) = fs::read(path) {
                rom_data = Some(data);
                break;
            }
        }
    }

    let rom_data = match rom_data {
        Some(data) => data,
        None => {
            eprintln!("No ROM file found.");
            return;
        }
    };

    let mut emu = Emu::new();
    emu.load_rom(&rom_data).expect("Failed to load ROM");

    // Create traces directory if it doesn't exist (in project root, not core/)
    fs::create_dir_all("../traces").ok();

    // Generate timestamped output filename
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let output_path = format!("../traces/ours_{}.log", timestamp);
    let file = File::create(&output_path).expect("Failed to create output file");
    let mut writer = BufWriter::new(file);

    eprintln!("=== Comparison trace ({} steps) ===", max_steps);
    eprintln!("Output: {}", output_path);

    // Log initial state (before any execution)
    log_state(&mut writer, &mut emu, 0, 0);

    let mut step: u64 = 0;
    let mut total_cycles: u64 = 0;

    while step < max_steps {
        // Run one instruction
        let cycles_used = emu.run_cycles(1) as u64;
        step += 1;
        total_cycles += cycles_used;

        // Log state after each instruction
        log_state(&mut writer, &mut emu, step, total_cycles);

        // Progress indicator every 100K steps
        if step % 100_000 == 0 {
            eprintln!("Progress: {} steps ({:.1}%)", step, 100.0 * step as f64 / max_steps as f64);
        }

        // Stop at HALT
        if emu.is_halted() {
            eprintln!("HALT at step {} / cycle {}", step, total_cycles);
            break;
        }
    }

    writer.flush().expect("Failed to flush output");
    eprintln!("Trace complete: {} steps / {} cycles", step, total_cycles);
    eprintln!("Saved to: {}", output_path);
}

fn log_state(writer: &mut BufWriter<File>, emu: &mut Emu, step: u64, cycles: u64) {
    let pc = emu.pc();
    let sp = emu.sp();
    let af = ((emu.a() as u16) << 8) | (emu.f() as u16);
    let bc = emu.bc();
    let de = emu.de();
    let hl = emu.hl();
    let ix = emu.ix();
    let iy = emu.iy();
    let adl = emu.adl();
    let iff1 = emu.iff1();
    let iff2 = emu.iff2();
    let im = emu.interrupt_mode();
    let halted = emu.is_halted();

    // Read opcode bytes
    let op1 = emu.peek_byte(pc);
    let op2 = emu.peek_byte(pc.wrapping_add(1));
    let op3 = emu.peek_byte(pc.wrapping_add(2));
    let op4 = emu.peek_byte(pc.wrapping_add(3));

    let op_str = if op1 == 0xDD || op1 == 0xFD {
        if op2 == 0xCB {
            format!("{:02X}{:02X}{:02X}{:02X}", op1, op2, op3, op4)
        } else {
            format!("{:02X}{:02X}", op1, op2)
        }
    } else if op1 == 0xED || op1 == 0xCB {
        format!("{:02X}{:02X}", op1, op2)
    } else {
        format!("{:02X}", op1)
    };

    // Format: step cycles PC SP AF BC DE HL IX IY ADL IFF1 IFF2 IM HALT op
    // IM: output as "Mode0/Mode1/Mode2" to match CEmu format
    let im_str = format!("{:?}", im).replace("IM", "Mode");
    writeln!(
        writer,
        "{:06} {:08} {:06X} {:06X} {:04X} {:06X} {:06X} {:06X} {:06X} {:06X} {} {} {} {} {} {}",
        step, cycles, pc, sp, af, bc, de, hl, ix, iy,
        if adl { 1 } else { 0 },
        if iff1 { 1 } else { 0 },
        if iff2 { 1 } else { 0 },
        im_str,
        if halted { 1 } else { 0 },
        op_str
    ).expect("Failed to write trace line");
}
