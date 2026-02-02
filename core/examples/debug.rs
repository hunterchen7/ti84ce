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
        "calc" => {
            // Default to "6+7" calculation
            let expr = args.get(2).map(|s| s.as_str()).unwrap_or("6+7");
            cmd_calc(expr);
        }
        "mathprint" => cmd_mathprint_trace(),
        "watchpoint" => cmd_watchpoint_mathprint(),
        "ports" => cmd_ports(),
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

  calc [expr]       Run a calculation and trace it
                    Default: "6+7"
                    Boots, types expression, captures trace on ENTER
                    Supported chars: 0-9, +, -, *, /

  mathprint         Trace writes to MathPrint flag (0xD000C4) during boot
                    Investigates why emulator boots into Classic mode

  watchpoint        Single-step boot and capture PC when 0xD000C4 is written
                    Provides exact code location making MathPrint decision

  ports             Dump control port values after boot
                    Useful for comparing with CEmu

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
    // Check environment variable for flash mode
    // Default is parallel flash (Bus::new() default), set SERIAL_FLASH=1 for serial flash
    let serial_flash = std::env::var("SERIAL_FLASH").map(|v| v == "1").unwrap_or(false);
    create_emu_with_serial_flash(serial_flash)
}

fn create_emu_with_serial_flash(serial_flash: bool) -> Option<Emu> {
    let rom_data = load_rom()?;
    let mut emu = Emu::new();
    emu.load_rom(&rom_data).expect("Failed to load ROM");
    emu.set_serial_flash(serial_flash);
    // Power on the calculator (required before run_cycles will execute)
    emu.press_on_key();
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
    let max_cycles = 300_000_000u64;
    let mut total_executed = 0u64;

    println!("\nBooting...");

    // Track how many times we see the idle loop address
    let mut idle_loop_count = 0;
    const IDLE_LOOP_THRESHOLD: u32 = 3;

    while total_executed < max_cycles {
        let executed = emu.run_cycles(chunk_size);
        total_executed += executed as u64;

        let pc = emu.pc();


        // Progress every 10M cycles
        if total_executed % 10_000_000 < chunk_size as u64 {
            println!(
                "[{:.1}M cycles] PC={:06X} SP={:06X} halted={}",
                total_executed as f64 / 1_000_000.0,
                pc,
                emu.sp(),
                emu.is_halted()
            );
        }

        // Check for idle loop - TI-OS idle is at 085B7D-085B80 (EI; NOP; HALT; PUSH HL)
        // The CPU continuously wakes from HALT due to interrupts, so we detect
        // boot completion by seeing this PC range repeatedly with LCD enabled.
        if (0x085B7D..=0x085B80).contains(&pc) {
            idle_loop_count += 1;
            let lcd = emu.lcd_snapshot();
            if idle_loop_count >= IDLE_LOOP_THRESHOLD && lcd.control & 1 != 0 {
                println!(
                    "\nBoot complete: Reached idle loop at PC={:06X} after {:.2}M cycles",
                    pc,
                    total_executed as f64 / 1_000_000.0
                );
                break;
            }
        }
        // Don't reset idle_loop_count - PC can be at other addresses during interrupt handling

        // Check for HALT - but don't break if we're in the idle loop area
        // The TI-OS idle loop (EI; NOP; HALT) continuously halts and wakes
        if emu.is_halted() {
            // If we've hit the idle loop at least once with LCD enabled, this is normal
            // operation - boot is complete. Check for LCD enabled.
            let lcd = emu.lcd_snapshot();
            if (0x085B7D..=0x085B80).contains(&pc) && lcd.control & 1 != 0 {
                println!(
                    "\nBoot complete: CPU halted at idle loop PC={:06X} after {:.2}M cycles",
                    pc,
                    total_executed as f64 / 1_000_000.0
                );
                break;
            }
            // Otherwise, unexpected HALT - report and break
            println!(
                "\nHALT at PC={:06X} after {:.2}M cycles",
                pc,
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

    // Log initial state - use bus_cycles() which includes memory timing
    // CEmu's cpu.cycles also includes memory access cycles (flash wait states, etc.)
    let initial_cycles = emu.bus_cycles();
    log_trace_line(&mut writer, &mut emu, 0, initial_cycles);

    let mut step = 0u64;
    let mut cycles = initial_cycles;

    while step < max_steps {
        // Debug: capture state before execution for first few steps
        let pc_before = emu.pc();
        let cycles_before = emu.bus_cycles();

        emu.run_cycles(1);
        step += 1;

        // Use total bus cycles (CPU + memory timing, matches CEmu's approach)
        cycles = emu.bus_cycles();

        // Debug: print detailed info for early steps
        if step <= 10 {
            let pc_after = emu.pc();
            eprintln!("Step {}: PC {:06X} -> {:06X}, cycles {} -> {} (delta {})",
                     step, pc_before, pc_after, cycles_before, cycles,
                     cycles as i64 - cycles_before as i64);
        }

        log_trace_line(&mut writer, &mut emu, step, cycles);

        if step % 100_000 == 0 {
            eprintln!("Progress: {} steps ({:.1}%)", step, 100.0 * step as f64 / max_steps as f64);
        }

        if emu.is_halted() {
            eprintln!("HALT at step {} / cycle {}", step, cycles);
            break;
        }
    }

    writer.flush().expect("Failed to flush output");
    println!("Trace complete: {} steps / {} cycles", step, cycles);
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

// === Calculation Test ===

/// Key mappings for TI-84 CE (row, col)
fn char_to_key(c: char) -> Option<(usize, usize)> {
    match c {
        '0' => Some((3, 0)),
        '1' => Some((3, 1)),
        '2' => Some((4, 1)),
        '3' => Some((5, 1)),
        '4' => Some((3, 2)),
        '5' => Some((4, 2)),
        '6' => Some((5, 2)),
        '7' => Some((3, 3)),
        '8' => Some((4, 3)),
        '9' => Some((5, 3)),
        '+' => Some((6, 1)),
        '-' => Some((6, 2)),
        '*' => Some((6, 3)),
        '/' => Some((6, 4)),
        _ => None,
    }
}

fn cmd_calc(expr: &str) {
    let mut emu = match create_emu() {
        Some(e) => e,
        None => return,
    };

    println!("\n=== Calculation Test: {} ===\n", expr);

    // Boot and wait for auto-init (65M cycles + margin)
    println!("Booting...");
    let boot_cycles = 70_000_000u32;
    let mut total = 0u64;
    while total < boot_cycles as u64 {
        let executed = emu.run_cycles(1_000_000);
        total += executed as u64;
        if total % 10_000_000 < 1_000_000 {
            println!("  {:.1}M cycles...", total as f64 / 1_000_000.0);
        }
    }
    println!("Boot complete ({:.1}M cycles)", total as f64 / 1_000_000.0);

    // Release the ON key that was pressed during power_on
    emu.release_on_key();
    // Give TI-OS time to process the key release
    emu.run_cycles(1_000_000);

    // Send initialization ENTER to get TI-OS expression parser ready
    // See findings.md: first ENTER after boot shows "Done" instead of result
    // This initializes the parser state so subsequent calculations work
    println!("Sending init ENTER...");
    emu.set_key(6, 0, true);
    emu.run_cycles(500_000);
    emu.set_key(6, 0, false);
    emu.run_cycles(2_000_000);  // Give time to process

    // Helper to show OP1
    fn show_op1(emu: &mut Emu, label: &str) {
        print!("  OP1 {}: ", label);
        for i in 0..9 {
            print!("{:02X} ", emu.peek_byte(0xD005F8 + i));
        }
        println!();
    }

    show_op1(&mut emu, "after boot");
    let int_status = emu.interrupt_status();
    let int_enabled = emu.interrupt_enabled();
    println!("  Keypad mode: {}, CPU halted: {}, IFF1: {}", emu.keypad_mode(), emu.is_halted(), emu.iff1());
    println!("  int_status: 0x{:08X}, int_enabled: 0x{:08X}, pending: 0x{:08X}",
             int_status, int_enabled, int_status & int_enabled);

    // Type the expression
    println!("\nTyping expression: {}", expr);
    for c in expr.chars() {
        if let Some((row, col)) = char_to_key(c) {
            println!("  Key '{}' -> row={}, col={}", c, row, col);
            emu.set_key(row, col, true);
            let cycles = emu.run_cycles(500_000);  // ~10ms hold
            let int_pending = emu.interrupt_status() & emu.interrupt_enabled();
            println!("    Executed {} cycles, halted={}, PC=0x{:06X}, int_pending=0x{:08X}",
                     cycles, emu.is_halted(), emu.pc(), int_pending);
            emu.set_key(row, col, false);
            emu.run_cycles(500_000);  // ~10ms release
            show_op1(&mut emu, &format!("after '{}'", c));
        } else {
            eprintln!("  Unknown key: '{}'", c);
        }
    }

    // Press ENTER and trace the calculation
    println!("\nPressing ENTER and tracing calculation...");

    // Create trace file
    fs::create_dir_all("../traces").ok();
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let output_path = format!("../traces/calc_{}_{}.log", expr.replace("+", "plus").replace("-", "minus"), timestamp);
    let file = File::create(&output_path).expect("Failed to create trace file");
    let mut writer = BufWriter::new(file);

    // Press ENTER
    emu.set_key(6, 0, true);

    // Trace execution
    let trace_steps = 50_000u64;
    let mut step = 0u64;
    let mut trace_cycles = 0u64;

    while step < trace_steps {
        let cycles = emu.run_cycles(1) as u64;
        trace_cycles += cycles;
        log_trace_line(&mut writer, &mut emu, step, trace_cycles);
        step += 1;

        if step % 10_000 == 0 {
            eprintln!("  Traced {} steps...", step);
        }
    }

    // Release ENTER
    emu.set_key(6, 0, false);

    writer.flush().expect("Failed to flush");
    println!("\nTrace saved to: {}", output_path);

    // Render and save screen
    emu.run_cycles(5_000_000);  // Let display update
    emu.render_frame();
    let screen_path = format!("calc_{}_result.ppm", expr.replace("+", "plus").replace("-", "minus"));
    save_framebuffer_ppm(&emu, &screen_path);
    println!("Screen saved to: {}", screen_path);

    // Show OP1 (result register)
    println!("\n=== Result (OP1 at 0xD005F8) ===");
    for i in 0..9 {
        print!("{:02X} ", emu.peek_byte(0xD005F8 + i));
    }
    println!();
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

    // TI-OS cursor and display state variables
    println!("\n=== TI-OS Display State ===");
    let currow = emu.peek_byte(0xD00595);
    let curcol = emu.peek_byte(0xD00596);
    let penrow = emu.peek_byte(0xD008D5) as u16 | ((emu.peek_byte(0xD008D6) as u16) << 8);
    let pencol = emu.peek_byte(0xD008D2) as u16 | ((emu.peek_byte(0xD008D3) as u16) << 8);
    println!("curRow: {} curCol: {}", currow, curcol);
    println!("penRow: {} penCol: {}", penrow, pencol);

    // MathPrint vs Classic mode flags
    let mathprint_flags = emu.peek_byte(0xD000C4);
    let mathprint_backup = emu.peek_byte(0xD003E6);
    println!("mathprintFlags (0xD000C4): 0x{:02X}", mathprint_flags);
    println!("mathprintBackup (0xD003E6): 0x{:02X}", mathprint_backup);

    // Check some system flags area for MathPrint enabled bit
    // mathprintEnabled is apparently at offset 0x0005 in some flags structure
    let flags_base_d00080 = 0xD00080u32;
    let flags = emu.peek_byte(flags_base_d00080 + 5);
    println!("flags@0xD00085 (mathprintEnabled?): 0x{:02X}", flags);

    // Check LCD timing registers for display dimensions
    println!("\n=== LCD Timing ===");
    println!("timing[0]: 0x{:08X}", lcd.timing[0]);
    println!("timing[1]: 0x{:08X}", lcd.timing[1]);
    println!("timing[2]: 0x{:08X}", lcd.timing[2]);
    println!("timing[3]: 0x{:08X}", lcd.timing[3]);

    // Decode timing like CEmu does
    let ppl = ((lcd.timing[0] >> 2) & 0x3F) + 1;
    let lpp = ((lcd.timing[1] >> 0) & 0x3FF) + 1;
    let vfp = (lcd.timing[1] >> 16) & 0xFF;
    let vbp = (lcd.timing[1] >> 24) & 0xFF;
    println!("PPL (pixels/line): {} -> {} pixels", ppl, ppl * 16);
    println!("LPP (lines/panel): {}", lpp);
    println!("VFP (vertical front porch): {}", vfp);
    println!("VBP (vertical back porch): {}", vbp);

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

// === MathPrint Investigation ===

/// Trace writes to MathPrint flag area during boot
/// MathPrint flag is at 0xD000C4, bit 5
fn cmd_mathprint_trace() {
    let mut emu = match create_emu() {
        Some(e) => e,
        None => return,
    };

    println!("\n=== MathPrint Flag Write Trace ===\n");
    println!("Target addresses:");
    println!("  0xD000C4 - mathprintFlagsLoc (bit 5 = MathPrint enabled)");
    println!("  0xD003E6 - mathprintBackup");
    println!("  0xD00080-0xD000FF - System flags area");
    println!();

    // Enable write tracing for the system flags area (includes MathPrint)
    // We'll trace a wider range to see what's being initialized
    emu.set_write_trace_filter(0xD00080, 0xD00100);
    emu.enable_write_tracing();

    println!("Running boot with write tracing enabled...\n");

    // Boot the emulator
    let boot_cycles = 70_000_000u64;
    let mut total_cycles = 0u64;
    let mut steps = 0u64;
    let mut last_report = 0u64;

    while total_cycles < boot_cycles {
        let executed = emu.run_cycles(100_000);
        if executed == 0 {
            break;
        }
        total_cycles += executed as u64;
        steps += 1;

        // Report progress every 10M cycles
        if total_cycles - last_report >= 10_000_000 {
            println!("  ... {} cycles, {} writes so far", total_cycles, emu.write_trace_total());
            last_report = total_cycles;
        }
    }

    println!("\nBoot complete: {} cycles, {} steps\n", total_cycles, steps);

    // Get the detailed write log
    let write_log = emu.get_write_log();
    println!("Total writes to filter range: {}", write_log.len());
    println!();

    // Show writes to specific addresses of interest
    let mathprint_addr = 0xD000C4u32;
    let backup_addr = 0xD003E6u32;

    println!("=== Writes to MathPrint Flag (0xD000C4) ===");
    let mp_writes: Vec<_> = write_log.iter()
        .filter(|(addr, _, _)| *addr == mathprint_addr)
        .collect();

    if mp_writes.is_empty() {
        println!("NO WRITES to 0xD000C4 during boot!");
    } else {
        for (addr, value, cycle) in &mp_writes {
            println!("  Cycle {:10}: 0x{:06X} <- 0x{:02X} (bit5={})",
                cycle, addr, value,
                if value & 0x20 != 0 { "SET (MathPrint ON)" } else { "CLEAR (Classic)" });
        }
    }

    println!();

    // Now trace writes to the backup address too (need to run again with wider filter)
    // Actually, let's just check the final values
    println!("=== Final Values ===");
    let final_mp = emu.peek_byte(mathprint_addr);
    let final_backup = emu.peek_byte(backup_addr);
    println!("mathprintFlagsLoc (0xD000C4): 0x{:02X} (bit5={})",
        final_mp,
        if final_mp & 0x20 != 0 { "SET - MathPrint ON" } else { "CLEAR - Classic mode" });
    println!("mathprintBackup (0xD003E6): 0x{:02X}", final_backup);

    // Also show all writes to the flag area
    println!("\n=== All Writes to System Flags (0xD00080-0xD00100) ===");
    println!("Address  Value  Cycle");
    for (addr, value, cycle) in write_log.iter().take(50) {
        println!("0x{:06X}  0x{:02X}  {:10}", addr, value, cycle);
    }
    if write_log.len() > 50 {
        println!("... and {} more writes", write_log.len() - 50);
    }

    // Check write count for the specific address
    println!("\n=== Write Counts for Key Addresses ===");
    println!("0xD000C4: {} writes", emu.address_write_count(mathprint_addr));
    for offset in 0..16u32 {
        let addr = 0xD000C0 + offset;
        let count = emu.address_write_count(addr);
        if count > 0 {
            println!("0x{:06X}: {} writes (final value: 0x{:02X})",
                addr, count, emu.peek_byte(addr));
        }
    }
}

// === Control Port Dump ===

/// Dump control port values after boot for comparison with CEmu
fn cmd_ports() {
    let mut emu = match create_emu() {
        Some(e) => e,
        None => return,
    };

    println!("\n=== Control Port Dump ===\n");

    // Boot the emulator first
    println!("Booting emulator...");
    let boot_cycles = 70_000_000u64;
    let mut total_cycles = 0u64;

    while total_cycles < boot_cycles {
        let executed = emu.run_cycles(1_000_000);
        if executed == 0 {
            break;
        }
        total_cycles += executed as u64;
        if total_cycles % 10_000_000 < 1_000_000 {
            println!("  {:.1}M cycles...", total_cycles as f64 / 1_000_000.0);
        }
    }

    println!("\nBoot complete: {} cycles\n", total_cycles);

    // Dump control ports
    println!("{}", emu.dump_control_ports());

    // Also show some key memory values that might affect MathPrint
    println!("\n=== Key Memory Values ===");
    let mathprint_flags = emu.peek_byte(0xD000C4);
    println!("0xD000C4 (mathprintFlags): 0x{:02X} (bit5={} -> {})",
        mathprint_flags,
        if mathprint_flags & 0x20 != 0 { "SET" } else { "CLEAR" },
        if mathprint_flags & 0x20 != 0 { "MathPrint" } else { "Classic" });

    let mathprint_backup = emu.peek_byte(0xD003E6);
    println!("0xD003E6 (mathprintBackup): 0x{:02X}", mathprint_backup);

    // Show system flags
    println!("\n=== System Flags Area (0xD00080-0xD000FF) ===");
    for base in (0xD00080u32..0xD00100).step_by(16) {
        print!("0x{:06X}: ", base);
        for offset in 0..16u32 {
            print!("{:02X} ", emu.peek_byte(base + offset));
        }
        println!();
    }

    // Test: Poke MathPrint flag to 0x20 and render to see if status bar changes
    println!("\n=== Poking MathPrint Flag Test ===");
    println!("Before: 0xD000C4 = 0x{:02X}", emu.peek_byte(0xD000C4));
    emu.poke_byte(0xD000C4, 0x20); // Set bit 5 (MathPrint enabled)
    println!("After:  0xD000C4 = 0x{:02X}", emu.peek_byte(0xD000C4));
    println!("Rendering screen to ports_mathprint.ppm...");
    emu.render_frame();
    save_framebuffer_ppm(&emu, "ports_mathprint.ppm");

    // Also render without the flag for comparison
    emu.poke_byte(0xD000C4, 0x00); // Clear bit 5 (Classic mode)
    println!("Rendering screen to ports_classic.ppm...");
    emu.render_frame();
    save_framebuffer_ppm(&emu, "ports_classic.ppm");
    println!("Compare the two screenshots to see if status bar changes");
}

// === Watchpoint for MathPrint Investigation ===

/// Single-step through boot and capture PC when 0xD000C4 is written
/// This gives us the exact code location making the MathPrint/Classic decision
fn cmd_watchpoint_mathprint() {
    let mut emu = match create_emu() {
        Some(e) => e,
        None => return,
    };

    println!("\n=== MathPrint Watchpoint ===\n");
    println!("Single-stepping through boot to find writes to 0xD000C4...");
    println!("This will capture the PC when the MathPrint flag is set.\n");

    const MATHPRINT_ADDR: u32 = 0xD000C4;
    const MAX_CYCLES: u64 = 70_000_000; // Stop after 70M cycles (full boot)

    let mut total_cycles = 0u64;
    let mut step_count = 0u64;
    let mut prev_value = emu.peek_byte(MATHPRINT_ADDR);
    let mut writes_found: Vec<(u64, u32, u8, u8)> = Vec::new(); // (cycle, pc, old, new)
    let mut last_report = 0u64;

    println!("Initial value at 0xD000C4: 0x{:02X}", prev_value);
    println!();

    while total_cycles < MAX_CYCLES {
        // Capture PC before stepping
        let pc_before = emu.pc();

        // Single step
        let cycles = emu.run_cycles(1) as u64;
        if cycles == 0 {
            break;
        }
        total_cycles += cycles;
        step_count += 1;

        // Check if 0xD000C4 changed
        let new_value = emu.peek_byte(MATHPRINT_ADDR);
        if new_value != prev_value {
            writes_found.push((total_cycles, pc_before, prev_value, new_value));

            let mode = if new_value & 0x20 != 0 { "MathPrint" } else { "Classic" };
            println!("=== WRITE DETECTED ===");
            println!("  Cycle: {}", total_cycles);
            println!("  Step: {}", step_count);
            println!("  PC: 0x{:06X}", pc_before);
            println!("  Old value: 0x{:02X}", prev_value);
            println!("  New value: 0x{:02X} ({})", new_value, mode);

            // Dump registers at time of write
            println!("\n  === Registers ===");
            println!("  AF={:02X}{:02X} BC={:06X} DE={:06X} HL={:06X}",
                emu.a(), emu.f(), emu.bc(), emu.de(), emu.hl());
            println!("  IX={:06X} IY={:06X} SP={:06X}", emu.ix(), emu.iy(), emu.sp());

            // Read instruction bytes at PC
            print!("  Instruction at PC: ");
            for i in 0..6 {
                print!("{:02X} ", emu.peek_byte(pc_before.wrapping_add(i)));
            }
            println!();

            // Dump surrounding memory context
            println!("\n  === Memory around write ===");
            let base = (MATHPRINT_ADDR & !0xF) as u32;
            for row in 0..4u32 {
                let addr = base + row * 16;
                print!("  0x{:06X}: ", addr);
                for offset in 0..16u32 {
                    let byte = emu.peek_byte(addr + offset);
                    if addr + offset == MATHPRINT_ADDR {
                        print!("[{:02X}]", byte);
                    } else {
                        print!("{:02X} ", byte);
                    }
                }
                println!();
            }

            // Dump execution history
            println!("\n  === Recent execution history ===");
            println!("{}", emu.dump_history());

            prev_value = new_value;
        }

        // Progress report every 5M cycles
        if total_cycles - last_report >= 5_000_000 {
            println!("  ... {} cycles ({} steps), {} writes found so far",
                total_cycles, step_count, writes_found.len());
            last_report = total_cycles;
        }
    }

    println!("\n=== Summary ===");
    println!("Total cycles: {}", total_cycles);
    println!("Total steps: {}", step_count);
    println!("Writes to 0xD000C4: {}", writes_found.len());

    if !writes_found.is_empty() {
        println!("\nAll writes:");
        for (cycle, pc, old, new) in &writes_found {
            let mode = if new & 0x20 != 0 { "MathPrint" } else { "Classic" };
            println!("  Cycle {:10} | PC=0x{:06X} | 0x{:02X} -> 0x{:02X} ({})",
                cycle, pc, old, new, mode);
        }
    }

    // Final state
    let final_value = emu.peek_byte(MATHPRINT_ADDR);
    println!("\nFinal value at 0xD000C4: 0x{:02X} ({})",
        final_value,
        if final_value & 0x20 != 0 { "MathPrint" } else { "Classic" });
}
