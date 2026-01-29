//! Debug and diagnostic tool for TI-84 CE emulator
//!
//! Consolidated tool for testing, tracing, and debugging the emulator.
//!
//! Usage:
//!   cargo run --release --example debug -- <command> [options]
//!
//! Commands:
//!   boot              Run boot test with progress reporting
//!   trace [steps]     Generate trace log for parity comparison (default: 100000)
//!   screen [output]   Render screen to image file (default: screen.png)
//!   vram              Analyze VRAM content (color histogram)
//!   compare <file>    Compare our trace with CEmu trace file
//!   help              Show this help message

use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::Command;

use emu_core::Emu;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_help();
        return;
    }

    match args[1].as_str() {
        "boot" => cmd_boot(),
        "trace" => {
            let steps = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100_000);
            cmd_trace(steps);
        }
        "screen" => {
            let output = args.get(2).map(|s| s.as_str()).unwrap_or("screen.png");
            cmd_screen(output);
        }
        "vram" => cmd_vram(),
        "compare" => {
            if args.len() < 3 {
                eprintln!("Usage: debug compare <cemu_trace_file>");
                return;
            }
            cmd_compare(&args[2]);
        }
        "help" | "--help" | "-h" => print_help(),
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_help();
        }
    }
}

fn print_help() {
    println!(
        r#"TI-84 CE Emulator Debug Tool

Usage: cargo run --release --example debug -- <command> [options]

Commands:
  boot              Run boot test with progress reporting
                    Shows boot progress, detects polling loops, analyzes final state

  trace [steps]     Generate trace log for parity comparison
                    Default: 100000 steps
                    Output: traces/ours_<timestamp>.log

  screen [output]   Render screen to image file after boot
                    Default output: screen.png
                    Runs emulator to completion and saves framebuffer

  vram              Analyze VRAM content after boot
                    Shows color histogram and pixel statistics

  compare <file>    Compare our trace with CEmu trace file
                    Reports first divergence point and statistics

  help              Show this help message

Environment Variables:
  DUMP_STEP=N       Dump memory at step N (for trace command)
  DUMP_ADDR=0xNNN   Address to dump (hex)
  DUMP_LEN=N        Number of bytes to dump

Examples:
  cargo run --release --example debug -- boot
  cargo run --release --example debug -- trace 1000000
  cargo run --release --example debug -- screen output.png
  cargo run --release --example debug -- compare traces/cemu.log
"#
    );
}

// === ROM Loading ===

fn load_rom() -> Option<Vec<u8>> {
    let rom_paths = ["TI-84 CE.rom", "../TI-84 CE.rom"];
    for path in &rom_paths {
        if Path::new(path).exists() {
            if let Ok(data) = fs::read(path) {
                eprintln!("Loaded ROM from: {} ({:.2} MB)", path, data.len() as f64 / 1024.0 / 1024.0);
                return Some(data);
            }
        }
    }
    eprintln!("ROM not found. Place 'TI-84 CE.rom' in project root or core/ directory.");
    None
}

fn create_emu() -> Option<Emu> {
    let rom_data = load_rom()?;
    let mut emu = Emu::new();
    emu.load_rom(&rom_data).expect("Failed to load ROM");
    Some(emu)
}

// === Boot Test ===

fn cmd_boot() {
    let mut emu = match create_emu() {
        Some(e) => e,
        None => return,
    };

    println!("\n=== Boot Test ===\n");
    println!("Initial state:");
    println!("{}", emu.dump_registers());

    let chunk_size = 1_000_000;
    let max_cycles = 100_000_000u64;
    let mut total_executed = 0u64;

    println!("\nBooting...");

    while total_executed < max_cycles {
        let executed = emu.run_cycles(chunk_size);
        total_executed += executed as u64;

        // Progress every 10M cycles
        if total_executed % 10_000_000 < chunk_size as u64 {
            println!(
                "[{:.1}M cycles] PC={:06X} SP={:06X} halted={}",
                total_executed as f64 / 1_000_000.0,
                emu.pc(),
                emu.sp(),
                emu.is_halted()
            );
        }

        if emu.is_halted() {
            println!(
                "\nHALT at PC={:06X} after {:.2}M cycles",
                emu.pc(),
                total_executed as f64 / 1_000_000.0
            );
            break;
        }
    }

    // Final state
    println!("\n=== Final State ===");
    println!("{}", emu.dump_registers());

    // LCD state
    let lcd = emu.lcd_snapshot();
    println!("\n=== LCD State ===");
    println!("Control: 0x{:08X} (enabled={})", lcd.control, (lcd.control & 1) != 0);
    println!("VRAM base: 0x{:06X}", lcd.upbase);

    // Screen analysis
    emu.render_frame();
    analyze_framebuffer(&emu);

    println!("\n=== Execution History ===");
    println!("{}", emu.dump_history());
}

// === Trace Generation ===

fn cmd_trace(max_steps: u64) {
    let mut emu = match create_emu() {
        Some(e) => e,
        None => return,
    };

    // Create traces directory
    fs::create_dir_all("../traces").ok();
    fs::create_dir_all("traces").ok();

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let output_path = format!("../traces/ours_{}.log", timestamp);
    let file = File::create(&output_path).expect("Failed to create output file");
    let mut writer = BufWriter::new(file);

    println!("=== Trace Generation ({} steps) ===", max_steps);
    println!("Output: {}", output_path);

    // Log initial state
    log_trace_line(&mut writer, &mut emu, 0, 0);

    let mut step = 0u64;
    let mut total_cycles = 0u64;

    while step < max_steps {
        let cycles_used = emu.run_cycles(1) as u64;
        step += 1;
        total_cycles += cycles_used;

        log_trace_line(&mut writer, &mut emu, step, total_cycles);

        if step % 100_000 == 0 {
            eprintln!("Progress: {} steps ({:.1}%)", step, 100.0 * step as f64 / max_steps as f64);
        }

        if emu.is_halted() {
            eprintln!("HALT at step {} / cycle {}", step, total_cycles);
            break;
        }
    }

    writer.flush().expect("Failed to flush output");
    println!("Trace complete: {} steps / {} cycles", step, total_cycles);
    println!("Saved to: {}", output_path);
}

fn log_trace_line(writer: &mut BufWriter<File>, emu: &mut Emu, step: u64, cycles: u64) {
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

// === Screen Rendering ===

fn cmd_screen(output: &str) {
    let mut emu = match create_emu() {
        Some(e) => e,
        None => return,
    };

    println!("Running emulator to completion...");

    let mut steps = 0u64;
    loop {
        let cycles = emu.run_cycles(1);
        if cycles == 0 || emu.is_halted() {
            break;
        }
        steps += 1;
        if steps > 5_000_000 {
            break;
        }
    }

    println!("Ran {} steps, halted={}", steps, emu.is_halted());

    // Render frame
    emu.render_frame();

    // Save as PPM first
    let ppm_path = output.replace(".png", ".ppm");
    save_framebuffer_ppm(&emu, &ppm_path);

    // Try to convert to PNG using sips (macOS)
    if output.ends_with(".png") {
        let result = Command::new("sips")
            .args(["-s", "format", "png", &ppm_path, "--out", output])
            .output();

        if result.is_ok() {
            fs::remove_file(&ppm_path).ok();
            println!("Saved: {}", output);
        } else {
            println!("Saved: {} (PNG conversion failed, keeping PPM)", ppm_path);
        }
    } else {
        println!("Saved: {}", ppm_path);
    }
}

fn save_framebuffer_ppm(emu: &Emu, path: &str) {
    let (width, height) = emu.framebuffer_size();
    let fb_ptr = emu.framebuffer_ptr();

    let file = File::create(path).expect("Failed to create output file");
    let mut writer = BufWriter::new(file);

    writeln!(writer, "P6").unwrap();
    writeln!(writer, "{} {}", width, height).unwrap();
    writeln!(writer, "255").unwrap();

    for y in 0..height {
        for x in 0..width {
            let pixel = unsafe { *fb_ptr.add(y * width + x) };
            let r = ((pixel >> 16) & 0xFF) as u8;
            let g = ((pixel >> 8) & 0xFF) as u8;
            let b = (pixel & 0xFF) as u8;
            writer.write_all(&[r, g, b]).unwrap();
        }
    }
}

// === VRAM Analysis ===

fn cmd_vram() {
    let mut emu = match create_emu() {
        Some(e) => e,
        None => return,
    };

    println!("Running emulator to completion...");

    let mut steps = 0u64;
    loop {
        let cycles = emu.run_cycles(1);
        if cycles == 0 || emu.is_halted() {
            break;
        }
        steps += 1;
        if steps > 5_000_000 {
            break;
        }
    }

    println!("Ran {} steps, halted={}", steps, emu.is_halted());

    // LCD state
    let lcd = emu.lcd_snapshot();
    println!("\n=== LCD State ===");
    println!("Control: 0x{:08X}", lcd.control);
    println!("VRAM base: 0x{:06X}", lcd.upbase);

    // VRAM analysis
    let upbase = lcd.upbase;
    let mut histogram: HashMap<u16, u32> = HashMap::new();

    for y in 0..240 {
        for x in 0..320 {
            let offset = (y * 320 + x) * 2;
            let lo = emu.peek_byte(upbase + offset as u32);
            let hi = emu.peek_byte(upbase + offset as u32 + 1);
            let rgb565 = (hi as u16) << 8 | lo as u16;
            *histogram.entry(rgb565).or_insert(0) += 1;
        }
    }

    let mut colors: Vec<_> = histogram.into_iter().collect();
    colors.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

    println!("\n=== VRAM Color Histogram ===");
    println!("Unique colors: {}", colors.len());
    println!("\nTop 10 colors:");
    for (color, count) in colors.iter().take(10) {
        let r = (color >> 11) & 0x1F;
        let g = (color >> 5) & 0x3F;
        let b = color & 0x1F;
        let pct = 100.0 * (*count as f64) / (320.0 * 240.0);
        println!(
            "  0x{:04X} (R{:02} G{:02} B{:02}): {:6} pixels ({:5.1}%)",
            color, r, g, b, count, pct
        );
    }

    // Render and analyze framebuffer
    emu.render_frame();
    println!("\n=== Framebuffer Analysis ===");
    analyze_framebuffer(&emu);
}

fn analyze_framebuffer(emu: &Emu) {
    let (width, height) = emu.framebuffer_size();
    let fb_ptr = emu.framebuffer_ptr();

    let mut non_black = 0;
    let mut white = 0;
    let total = width * height;

    for i in 0..total {
        let pixel = unsafe { *fb_ptr.add(i) };
        if pixel != 0xFF000000 {
            non_black += 1;
        }
        if pixel == 0xFFFFFFFF {
            white += 1;
        }
    }

    println!(
        "Non-black pixels: {} / {} ({:.1}%)",
        non_black, total, 100.0 * non_black as f64 / total as f64
    );
    println!(
        "White pixels: {} / {} ({:.1}%)",
        white, total, 100.0 * white as f64 / total as f64
    );
}

// === Trace Comparison ===

fn cmd_compare(cemu_file: &str) {
    // Find our latest trace
    let traces_dir = Path::new("../traces");
    let mut our_traces: Vec<_> = fs::read_dir(traces_dir)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_name()
                        .to_str()
                        .map(|s| s.starts_with("ours_"))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();

    our_traces.sort_by_key(|e| e.file_name());

    let our_file = match our_traces.last() {
        Some(f) => f.path(),
        None => {
            eprintln!("No trace files found in ../traces/");
            eprintln!("Run: cargo run --release --example debug -- trace");
            return;
        }
    };

    println!("=== Trace Comparison ===");
    println!("Our trace: {}", our_file.display());
    println!("CEmu trace: {}", cemu_file);

    let our_reader = BufReader::new(File::open(&our_file).expect("Failed to open our trace"));
    let cemu_reader = BufReader::new(File::open(cemu_file).expect("Failed to open CEmu trace"));

    let mut our_lines = our_reader.lines();
    let mut cemu_lines = cemu_reader.lines();
    let mut line_num = 0;
    let mut first_divergence: Option<(usize, String, String)> = None;
    let mut pc_match_count = 0;
    let mut full_match_count = 0;

    loop {
        let our_line = our_lines.next();
        let cemu_line = cemu_lines.next();

        match (our_line, cemu_line) {
            (Some(Ok(ours)), Some(Ok(cemu))) => {
                line_num += 1;

                // Parse PC from both lines (field 3, 0-indexed field 2)
                let our_fields: Vec<&str> = ours.split_whitespace().collect();
                let cemu_fields: Vec<&str> = cemu.split_whitespace().collect();

                if our_fields.len() >= 3 && cemu_fields.len() >= 3 {
                    let our_pc = our_fields[2];
                    let cemu_pc = cemu_fields[2];

                    if our_pc == cemu_pc {
                        pc_match_count += 1;
                    }

                    // Check full line match (ignoring cycles which may differ)
                    let our_key = our_fields[2..].join(" ");
                    let cemu_key = cemu_fields[2..].join(" ");

                    if our_key == cemu_key {
                        full_match_count += 1;
                    } else if first_divergence.is_none() {
                        first_divergence = Some((line_num, ours.clone(), cemu.clone()));
                    }
                }
            }
            (None, None) => break,
            (Some(_), None) => {
                println!("\nCEmu trace ended at line {}", line_num);
                break;
            }
            (None, Some(_)) => {
                println!("\nOur trace ended at line {}", line_num);
                break;
            }
            _ => break,
        }
    }

    println!("\n=== Results ===");
    println!("Lines compared: {}", line_num);
    println!("PC matches: {} ({:.1}%)", pc_match_count, 100.0 * pc_match_count as f64 / line_num as f64);
    println!("Full matches: {} ({:.1}%)", full_match_count, 100.0 * full_match_count as f64 / line_num as f64);

    if let Some((line, ours, cemu)) = first_divergence {
        println!("\n=== First Divergence at Line {} ===", line);
        println!("Ours: {}", ours);
        println!("CEmu: {}", cemu);
    } else {
        println!("\nNo divergence found - traces match completely!");
    }
}
