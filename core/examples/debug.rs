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
use std::time::Instant;

use emu_core::{Emu, StepInfo, IoTarget, IoOpType, disassemble};

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
        "fulltrace" => {
            let steps = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000);
            cmd_fulltrace(steps);
        }
        "fullcompare" => {
            if args.len() < 4 {
                eprintln!("Usage: debug fullcompare <ours.json> <cemu.json>");
                return;
            }
            cmd_fullcompare(&args[2], &args[3]);
        }
        "sendfile" => {
            if args.len() < 3 {
                eprintln!("Usage: debug sendfile <file.8xp> [file2.8xv ...]");
                return;
            }
            cmd_sendfile(&args[2..]);
        }
        "bakerom" => {
            if args.len() < 3 {
                eprintln!("Usage: debug bakerom <output.rom> [file.8xp file2.8xv ...]");
                return;
            }
            cmd_bakerom(&args[2], &args[3..]);
        }
        "rundoom" => {
            // Load baked ROM, boot, simulate pressing prgm → down → enter → enter
            cmd_rundoom();
        }
        "runprog" => {
            if args.len() < 3 {
                eprintln!("Usage: debug runprog <file.8xp> [lib1.8xv lib2.8xv ...] [--run-cycles N]");
                return;
            }
            // Parse --run-cycles if provided, default to 100M
            let mut run_cycles = 100_000_000u64;
            let mut file_args: Vec<&str> = Vec::new();
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--run-cycles" {
                    if let Some(val) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                        run_cycles = val;
                    }
                    i += 2;
                } else {
                    file_args.push(&args[i]);
                    i += 1;
                }
            }
            cmd_runprog(&file_args, run_cycles);
        }
        "run" => {
            if args.len() < 3 {
                eprintln!("Usage: debug run <file.8xp> [lib1.8xv ...] [--timeout <secs>] [--speed <multiplier>]");
                return;
            }
            let mut timeout_secs = 30u64;
            let mut speed: Option<f64> = None;
            let mut file_args: Vec<&str> = Vec::new();
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--timeout" {
                    if let Some(val) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                        timeout_secs = val;
                    }
                    i += 2;
                } else if args[i] == "--speed" {
                    if let Some(val) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                        speed = Some(val);
                    }
                    i += 2;
                } else {
                    file_args.push(&args[i]);
                    i += 1;
                }
            }
            cmd_run(&file_args, timeout_secs, speed);
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

  fulltrace [steps] Generate comprehensive trace with I/O operations
                    Default: 1000 steps
                    Output: JSON with full instruction and I/O details

  fullcompare <ours> <cemu>
                    Compare two JSON trace files and report divergence
                    Reports first difference in PC, registers, or I/O ops

  sendfile <file.8xp> [file2.8xv ...]
                    Load ROM, inject .8xp/.8xv files into flash, boot, and
                    render a screenshot. For games using graphx, include the
                    CE C library .8xv files (graphx, keypadc, libload, etc.)

  bakerom <output.rom> [file.8xp file2.8xv ...]
                    Create a new ROM with .8xp/.8xv files pre-installed in
                    the flash archive. The output ROM can be loaded directly
                    and programs will appear in TI-OS without needing sendfile.

  run <file.8xp> [lib.8xv ...]
                    Run a program headless with debug output capture.
                    Boots TI-OS, injects files, launches via Asm(prgm<NAME>).
                    Captures CE toolchain debug output (0xFB0000) to stdout.
                    Terminates on null sentinel, timeout, or power-off.
                    Options: --timeout <secs> (default: 30)
                             --speed <N> (e.g. 1=real-time, default: unthrottled)

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
  cargo run --release --example debug -- sendfile DOOM.8xp clibs/*.8xv
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

    let mut step_count = 0u64;

    while step_count < max_steps {
        // Execute one instruction and get pre-execution state
        let step_info = match emu.step() {
            Some(info) => info,
            None => break,
        };

        // Log the step with pre-execution PC/opcode but post-execution registers
        // This matches CEmu's trace format where registers show the result of execution
        log_step_info_post(&mut writer, step_count, &step_info, &emu);

        // Debug: print detailed info for early steps
        if step_count < 10 {
            let pc_after = emu.pc();
            eprintln!("Step {}: PC {:06X} -> {:06X}, cycles {} (delta {})",
                     step_count, step_info.pc, pc_after, step_info.total_cycles, step_info.cycles);
        }

        step_count += 1;

        if step_count % 100_000 == 0 {
            eprintln!("Progress: {} steps ({:.1}%)", step_count, 100.0 * step_count as f64 / max_steps as f64);
        }

        if emu.is_halted() {
            eprintln!("HALT at step {} / cycle {}", step_count, step_info.total_cycles);
            break;
        }
    }

    writer.flush().expect("Failed to flush output");
    println!("Trace complete: {} steps", step_count);
    println!("Saved to: {}", output_path);
}

/// Generate comprehensive trace with I/O operations (JSON format)
/// NOTE: To match CEmu's format, "regs_before" actually contains the state AFTER
/// the instruction executes (CEmu's naming is misleading).
fn cmd_fulltrace(max_steps: u64) {
    let mut emu = match create_emu() {
        Some(e) => e,
        None => return,
    };

    // Enable full I/O tracing
    emu.enable_full_trace();

    // Create traces directory
    fs::create_dir_all("../traces").ok();
    fs::create_dir_all("traces").ok();

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
    let output_path = format!("../traces/fulltrace_{}.json", timestamp);
    let file = File::create(&output_path).expect("Failed to create output file");
    let mut writer = BufWriter::new(file);

    println!("=== Full Trace Generation ({} steps) ===", max_steps);
    println!("Output: {}", output_path);

    // Write JSON array start
    writeln!(writer, "[").expect("Failed to write");

    let mut step_count = 0u64;
    let mut first_entry = true;

    // Buffer to hold previous step's info - we write prev step with current regs
    // This matches CEmu's format where "regs_before" is actually post-execution state
    let mut prev_step: Option<StepInfo> = None;

    while step_count < max_steps {
        // Execute one instruction and get state with I/O ops
        let step_info = match emu.step() {
            Some(info) => info,
            None => break,
        };

        // Write the PREVIOUS step's entry using CURRENT registers (post-execution)
        if let Some(prev) = prev_step.take() {
            if !first_entry {
                writeln!(writer, ",").expect("Failed to write");
            }
            first_entry = false;

            // Write prev step's PC/opcode but current step's registers (post-execution state)
            write_fulltrace_json_with_post_regs(&mut writer, step_count - 1, &prev, &step_info);
        }

        step_count += 1;
        prev_step = Some(step_info.clone());

        if step_count % 10_000 == 0 {
            eprintln!("Progress: {} steps ({:.1}%)", step_count, 100.0 * step_count as f64 / max_steps as f64);
        }

        if emu.is_halted() {
            eprintln!("HALT at step {} / cycle {}", step_count, step_info.total_cycles);
            // Write final step - use current emulator state as post-execution registers
            if let Some(prev) = prev_step.take() {
                if !first_entry {
                    writeln!(writer, ",").expect("Failed to write");
                }
                // Create pseudo StepInfo with current state
                let final_regs = StepInfo {
                    pc: emu.pc(),
                    sp: emu.sp(),
                    a: emu.a(),
                    f: emu.f(),
                    bc: emu.bc(),
                    de: emu.de(),
                    hl: emu.hl(),
                    ix: emu.ix(),
                    iy: emu.iy(),
                    adl: emu.adl(),
                    iff1: emu.iff1(),
                    iff2: emu.iff2(),
                    im: emu.interrupt_mode(),
                    halted: emu.is_halted(),
                    opcode: [0; 4],
                    opcode_len: 0,
                    cycles: 0,
                    total_cycles: emu.total_cycles(),
                    io_ops: vec![],
                };
                write_fulltrace_json_with_post_regs(&mut writer, step_count - 1, &prev, &final_regs);
            }
            break;
        }
    }

    // Write final step if we didn't hit HALT
    // For the last step, we need to capture current emulator state as post-execution registers
    if let Some(prev) = prev_step {
        if !first_entry {
            writeln!(writer, ",").expect("Failed to write");
        }
        // Create a pseudo StepInfo with current emulator state for the final entry's registers
        let final_regs = StepInfo {
            pc: emu.pc(),
            sp: emu.sp(),
            a: emu.a(),
            f: emu.f(),
            bc: emu.bc(),
            de: emu.de(),
            hl: emu.hl(),
            ix: emu.ix(),
            iy: emu.iy(),
            adl: emu.adl(),
            iff1: emu.iff1(),
            iff2: emu.iff2(),
            im: emu.interrupt_mode(),
            halted: emu.is_halted(),
            opcode: [0; 4],
            opcode_len: 0,
            cycles: 0,
            total_cycles: emu.total_cycles(),
            io_ops: vec![],
        };
        write_fulltrace_json_with_post_regs(&mut writer, step_count - 1, &prev, &final_regs);
    }

    // Write JSON array end
    writeln!(writer, "\n]").expect("Failed to write");

    writer.flush().expect("Failed to flush output");
    println!("Full trace complete: {} steps", step_count);
    println!("Saved to: {}", output_path);
}

/// Write trace entry using previous step's PC/opcode but current step's registers
/// This matches CEmu's format where "regs_before" is actually post-execution state
fn write_fulltrace_json_with_post_regs(
    writer: &mut BufWriter<File>,
    step: u64,
    prev_info: &StepInfo,
    curr_info: &StepInfo,
) {
    // Disassemble the instruction
    let disasm = disassemble(&prev_info.opcode[..prev_info.opcode_len], prev_info.adl);

    // Format opcode bytes
    let opcode_hex = prev_info.opcode[..prev_info.opcode_len]
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ");

    // CEmu reports cpu.cycles which is cycles AFTER instruction execution
    // Use total_cycles directly to match CEmu's format
    let cycles_after = prev_info.total_cycles;

    // Start JSON object
    write!(writer, "  {{\n").expect("Failed to write");
    write!(writer, "    \"step\": {},\n", step).expect("Failed to write");
    write!(writer, "    \"cycle\": {},\n", cycles_after).expect("Failed to write");
    write!(writer, "    \"type\": \"instruction\",\n").expect("Failed to write");
    write!(writer, "    \"pc\": \"0x{:06X}\",\n", prev_info.pc).expect("Failed to write");

    // Opcode info
    write!(writer, "    \"opcode\": {{\n").expect("Failed to write");
    write!(writer, "      \"bytes\": \"{}\",\n", opcode_hex).expect("Failed to write");
    write!(writer, "      \"mnemonic\": \"{}\"\n", escape_json(&disasm.mnemonic)).expect("Failed to write");
    write!(writer, "    }},\n").expect("Failed to write");

    // Registers - use CURRENT step's pre-state (which is prev step's post-state)
    write!(writer, "    \"regs_before\": {{\n").expect("Failed to write");
    write!(writer, "      \"A\": \"0x{:02X}\",\n", curr_info.a).expect("Failed to write");
    write!(writer, "      \"F\": \"0x{:02X}\",\n", curr_info.f).expect("Failed to write");
    write!(writer, "      \"BC\": \"0x{:06X}\",\n", curr_info.bc).expect("Failed to write");
    write!(writer, "      \"DE\": \"0x{:06X}\",\n", curr_info.de).expect("Failed to write");
    write!(writer, "      \"HL\": \"0x{:06X}\",\n", curr_info.hl).expect("Failed to write");
    write!(writer, "      \"IX\": \"0x{:06X}\",\n", curr_info.ix).expect("Failed to write");
    write!(writer, "      \"IY\": \"0x{:06X}\",\n", curr_info.iy).expect("Failed to write");
    write!(writer, "      \"SP\": \"0x{:06X}\",\n", curr_info.sp).expect("Failed to write");
    write!(writer, "      \"IFF1\": {},\n", curr_info.iff1).expect("Failed to write");
    write!(writer, "      \"IFF2\": {},\n", curr_info.iff2).expect("Failed to write");
    write!(writer, "      \"IM\": \"{:?}\",\n", curr_info.im).expect("Failed to write");
    write!(writer, "      \"ADL\": {},\n", curr_info.adl).expect("Failed to write");
    write!(writer, "      \"halted\": {}\n", curr_info.halted).expect("Failed to write");
    write!(writer, "    }},\n").expect("Failed to write");

    // I/O operations from the previous step
    write!(writer, "    \"io_ops\": [\n").expect("Failed to write");
    for (i, io_op) in prev_info.io_ops.iter().enumerate() {
        let target_str = match io_op.target {
            IoTarget::Ram => "ram",
            IoTarget::Flash => "flash",
            IoTarget::MmioPort => "mmio",
            IoTarget::CpuPort => "port",
        };
        let op_type_str = match io_op.op_type {
            IoOpType::Read => "read",
            IoOpType::Write => "write",
        };

        write!(writer, "      {{\n").expect("Failed to write");
        write!(writer, "        \"type\": \"{}\",\n", op_type_str).expect("Failed to write");
        write!(writer, "        \"target\": \"{}\",\n", target_str).expect("Failed to write");
        write!(writer, "        \"addr\": \"0x{:06X}\",\n", io_op.addr).expect("Failed to write");
        if matches!(io_op.op_type, IoOpType::Write) {
            write!(writer, "        \"old\": \"0x{:02X}\",\n", io_op.old_value).expect("Failed to write");
            write!(writer, "        \"new\": \"0x{:02X}\"\n", io_op.new_value).expect("Failed to write");
        } else {
            write!(writer, "        \"value\": \"0x{:02X}\"\n", io_op.new_value).expect("Failed to write");
        }
        if i < prev_info.io_ops.len() - 1 {
            write!(writer, "      }},\n").expect("Failed to write");
        } else {
            write!(writer, "      }}\n").expect("Failed to write");
        }
    }
    write!(writer, "    ],\n").expect("Failed to write");

    // Cycles used by this instruction
    write!(writer, "    \"cycles\": {}\n", prev_info.cycles).expect("Failed to write");
    write!(writer, "  }}").expect("Failed to write");
}

/// Write a single trace entry in JSON format
fn write_fulltrace_json(writer: &mut BufWriter<File>, step: u64, info: &StepInfo) {
    // Disassemble the instruction
    let disasm = disassemble(&info.opcode[..info.opcode_len], info.adl);

    // Format opcode bytes
    let opcode_hex = info.opcode[..info.opcode_len]
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ");

    // CEmu reports cpu.cycles which is cycles AFTER instruction execution
    // Use total_cycles directly to match CEmu's format
    let cycles_after = info.total_cycles;

    // Start JSON object
    write!(writer, "  {{\n").expect("Failed to write");
    write!(writer, "    \"step\": {},\n", step).expect("Failed to write");
    write!(writer, "    \"cycle\": {},\n", cycles_after).expect("Failed to write");
    write!(writer, "    \"type\": \"instruction\",\n").expect("Failed to write");
    write!(writer, "    \"pc\": \"0x{:06X}\",\n", info.pc).expect("Failed to write");

    // Opcode info
    write!(writer, "    \"opcode\": {{\n").expect("Failed to write");
    write!(writer, "      \"bytes\": \"{}\",\n", opcode_hex).expect("Failed to write");
    write!(writer, "      \"mnemonic\": \"{}\"\n", escape_json(&disasm.mnemonic)).expect("Failed to write");
    write!(writer, "    }},\n").expect("Failed to write");

    // Registers before
    write!(writer, "    \"regs_before\": {{\n").expect("Failed to write");
    write!(writer, "      \"A\": \"0x{:02X}\",\n", info.a).expect("Failed to write");
    write!(writer, "      \"F\": \"0x{:02X}\",\n", info.f).expect("Failed to write");
    write!(writer, "      \"BC\": \"0x{:06X}\",\n", info.bc).expect("Failed to write");
    write!(writer, "      \"DE\": \"0x{:06X}\",\n", info.de).expect("Failed to write");
    write!(writer, "      \"HL\": \"0x{:06X}\",\n", info.hl).expect("Failed to write");
    write!(writer, "      \"IX\": \"0x{:06X}\",\n", info.ix).expect("Failed to write");
    write!(writer, "      \"IY\": \"0x{:06X}\",\n", info.iy).expect("Failed to write");
    write!(writer, "      \"SP\": \"0x{:06X}\",\n", info.sp).expect("Failed to write");
    write!(writer, "      \"IFF1\": {},\n", info.iff1).expect("Failed to write");
    write!(writer, "      \"IFF2\": {},\n", info.iff2).expect("Failed to write");
    write!(writer, "      \"IM\": \"{:?}\",\n", info.im).expect("Failed to write");
    write!(writer, "      \"ADL\": {},\n", info.adl).expect("Failed to write");
    write!(writer, "      \"halted\": {}\n", info.halted).expect("Failed to write");
    write!(writer, "    }},\n").expect("Failed to write");

    // I/O operations
    write!(writer, "    \"io_ops\": [\n").expect("Failed to write");
    for (i, io_op) in info.io_ops.iter().enumerate() {
        let target_str = match io_op.target {
            IoTarget::Ram => "ram",
            IoTarget::Flash => "flash",
            IoTarget::MmioPort => "mmio",
            IoTarget::CpuPort => "port",
        };
        let op_type_str = match io_op.op_type {
            IoOpType::Read => "read",
            IoOpType::Write => "write",
        };

        write!(writer, "      {{\n").expect("Failed to write");
        write!(writer, "        \"type\": \"{}\",\n", op_type_str).expect("Failed to write");
        write!(writer, "        \"target\": \"{}\",\n", target_str).expect("Failed to write");
        write!(writer, "        \"addr\": \"0x{:06X}\",\n", io_op.addr).expect("Failed to write");
        if matches!(io_op.op_type, IoOpType::Write) {
            write!(writer, "        \"old\": \"0x{:02X}\",\n", io_op.old_value).expect("Failed to write");
            write!(writer, "        \"new\": \"0x{:02X}\"\n", io_op.new_value).expect("Failed to write");
        } else {
            write!(writer, "        \"value\": \"0x{:02X}\"\n", io_op.new_value).expect("Failed to write");
        }
        if i < info.io_ops.len() - 1 {
            write!(writer, "      }},\n").expect("Failed to write");
        } else {
            write!(writer, "      }}\n").expect("Failed to write");
        }
    }
    write!(writer, "    ],\n").expect("Failed to write");

    // Cycles used by this instruction
    write!(writer, "    \"cycles\": {}\n", info.cycles).expect("Failed to write");
    write!(writer, "  }}").expect("Failed to write");
}

/// Escape special characters for JSON string
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
     .replace('"', "\\\"")
     .replace('\n', "\\n")
     .replace('\r', "\\r")
     .replace('\t', "\\t")
}

/// Compare two fulltrace JSON files and report divergence
fn cmd_fullcompare(ours_path: &str, cemu_path: &str) {
    println!("=== Full Trace Comparison ===");
    println!("Our trace:  {}", ours_path);
    println!("CEmu trace: {}", cemu_path);

    // Read both files
    let ours_content = match fs::read_to_string(ours_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read our trace: {}", e);
            return;
        }
    };

    let cemu_content = match fs::read_to_string(cemu_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to read CEmu trace: {}", e);
            return;
        }
    };

    // Parse JSON entries (simple regex-based parsing for key values)
    let ours_entries = parse_trace_entries(&ours_content);
    let cemu_entries = parse_trace_entries(&cemu_content);

    println!("\nOur trace:  {} entries", ours_entries.len());
    println!("CEmu trace: {} entries", cemu_entries.len());

    let min_entries = ours_entries.len().min(cemu_entries.len());
    let mut first_divergence: Option<usize> = None;
    let mut divergence_count = 0;

    println!("\nComparing {} entries...\n", min_entries);

    for i in 0..min_entries {
        let ours = &ours_entries[i];
        let cemu = &cemu_entries[i];

        let mut diffs = Vec::new();

        // Compare step
        if ours.step != cemu.step {
            diffs.push(format!("step: {} vs {}", ours.step, cemu.step));
        }

        // Compare PC
        if ours.pc != cemu.pc {
            diffs.push(format!("PC: 0x{:06X} vs 0x{:06X}", ours.pc, cemu.pc));
        }

        // Compare registers
        if ours.a != cemu.a { diffs.push(format!("A: 0x{:02X} vs 0x{:02X}", ours.a, cemu.a)); }
        if ours.f != cemu.f { diffs.push(format!("F: 0x{:02X} vs 0x{:02X}", ours.f, cemu.f)); }
        if ours.bc != cemu.bc { diffs.push(format!("BC: 0x{:06X} vs 0x{:06X}", ours.bc, cemu.bc)); }
        if ours.de != cemu.de { diffs.push(format!("DE: 0x{:06X} vs 0x{:06X}", ours.de, cemu.de)); }
        if ours.hl != cemu.hl { diffs.push(format!("HL: 0x{:06X} vs 0x{:06X}", ours.hl, cemu.hl)); }
        if ours.ix != cemu.ix { diffs.push(format!("IX: 0x{:06X} vs 0x{:06X}", ours.ix, cemu.ix)); }
        if ours.iy != cemu.iy { diffs.push(format!("IY: 0x{:06X} vs 0x{:06X}", ours.iy, cemu.iy)); }
        if ours.sp != cemu.sp { diffs.push(format!("SP: 0x{:06X} vs 0x{:06X}", ours.sp, cemu.sp)); }

        // Compare flags
        if ours.adl != cemu.adl { diffs.push(format!("ADL: {} vs {}", ours.adl, cemu.adl)); }
        if ours.iff1 != cemu.iff1 { diffs.push(format!("IFF1: {} vs {}", ours.iff1, cemu.iff1)); }
        if ours.iff2 != cemu.iff2 { diffs.push(format!("IFF2: {} vs {}", ours.iff2, cemu.iff2)); }

        // Compare cycle count
        if ours.cycle != cemu.cycle {
            diffs.push(format!("cycles: {} vs {}", ours.cycle, cemu.cycle));
        }

        // Compare I/O operations count
        if ours.io_ops_count != cemu.io_ops_count {
            diffs.push(format!("io_ops: {} vs {}", ours.io_ops_count, cemu.io_ops_count));
        }

        if !diffs.is_empty() {
            divergence_count += 1;
            if first_divergence.is_none() {
                first_divergence = Some(i);
            }

            // Only print first 10 divergences in detail
            if divergence_count <= 10 {
                println!("=== DIVERGENCE at step {} ===", i);
                println!("Our PC:  0x{:06X}  Opcode: {}", ours.pc, ours.opcode);
                println!("CEmu PC: 0x{:06X}  Opcode: {}", cemu.pc, cemu.opcode);
                println!("Differences:");
                for diff in &diffs {
                    println!("  - {}", diff);
                }
                println!();
            }
        }
    }

    // Summary
    println!("=== Summary ===");
    println!("Entries compared: {}", min_entries);
    println!("Divergences: {}", divergence_count);

    if let Some(idx) = first_divergence {
        println!("First divergence at step: {}", idx);
    } else {
        println!("No divergences found!");
    }

    if ours_entries.len() != cemu_entries.len() {
        println!("Warning: Different number of entries ({} vs {})",
                 ours_entries.len(), cemu_entries.len());
    }
}

/// Parsed trace entry
#[derive(Default)]
struct TraceEntry {
    step: u64,
    cycle: u64,
    pc: u32,
    a: u8,
    f: u8,
    bc: u32,
    de: u32,
    hl: u32,
    ix: u32,
    iy: u32,
    sp: u32,
    adl: bool,
    iff1: bool,
    iff2: bool,
    opcode: String,
    io_ops_count: usize,
}

/// Parse trace entries from JSON content (simple regex-based parsing)
fn parse_trace_entries(content: &str) -> Vec<TraceEntry> {
    let mut entries = Vec::new();
    let mut current = TraceEntry::default();
    let mut in_io_ops = false;

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with("{") && !line.contains("io_ops") {
            current = TraceEntry::default();
        } else if line.starts_with("}") && !in_io_ops {
            entries.push(std::mem::take(&mut current));
        }

        // Parse fields
        if let Some(v) = extract_json_u64(line, "\"step\"") {
            current.step = v;
        }
        if let Some(v) = extract_json_u64(line, "\"cycle\"") {
            current.cycle = v;
        }
        if let Some(v) = extract_json_hex32(line, "\"pc\"") {
            current.pc = v;
        }
        if let Some(v) = extract_json_hex8(line, "\"A\"") {
            current.a = v;
        }
        if let Some(v) = extract_json_hex8(line, "\"F\"") {
            current.f = v;
        }
        if let Some(v) = extract_json_hex32(line, "\"BC\"") {
            current.bc = v;
        }
        if let Some(v) = extract_json_hex32(line, "\"DE\"") {
            current.de = v;
        }
        if let Some(v) = extract_json_hex32(line, "\"HL\"") {
            current.hl = v;
        }
        if let Some(v) = extract_json_hex32(line, "\"IX\"") {
            current.ix = v;
        }
        if let Some(v) = extract_json_hex32(line, "\"IY\"") {
            current.iy = v;
        }
        if let Some(v) = extract_json_hex32(line, "\"SP\"") {
            current.sp = v;
        }
        if line.contains("\"ADL\"") {
            current.adl = line.contains("true");
        }
        if line.contains("\"IFF1\"") {
            current.iff1 = line.contains("true");
        }
        if line.contains("\"IFF2\"") {
            current.iff2 = line.contains("true");
        }
        if let Some(v) = extract_json_string(line, "\"bytes\"") {
            current.opcode = v;
        }

        // Track I/O ops count
        if line.contains("\"io_ops\"") {
            in_io_ops = true;
        }
        if in_io_ops && line.contains("\"type\"") {
            current.io_ops_count += 1;
        }
        if in_io_ops && line.starts_with("]") {
            in_io_ops = false;
        }
    }

    entries
}

fn extract_json_u64(line: &str, key: &str) -> Option<u64> {
    if !line.contains(key) {
        return None;
    }
    let rest = line.split(':').nth(1)?;
    let value = rest.trim().trim_end_matches(',');
    value.parse().ok()
}

fn extract_json_hex32(line: &str, key: &str) -> Option<u32> {
    if !line.contains(key) {
        return None;
    }
    let rest = line.split(':').nth(1)?;
    let value = rest.trim().trim_matches(|c| c == '"' || c == ',' || c == ' ');
    let hex_str = value.trim_start_matches("0x").trim_start_matches("0X");
    u32::from_str_radix(hex_str, 16).ok()
}

fn extract_json_hex8(line: &str, key: &str) -> Option<u8> {
    if !line.contains(key) {
        return None;
    }
    let rest = line.split(':').nth(1)?;
    let value = rest.trim().trim_matches(|c| c == '"' || c == ',' || c == ' ');
    let hex_str = value.trim_start_matches("0x").trim_start_matches("0X");
    u8::from_str_radix(hex_str, 16).ok()
}

fn extract_json_string(line: &str, key: &str) -> Option<String> {
    if !line.contains(key) {
        return None;
    }
    let rest = line.split(':').nth(1)?;
    let value = rest.trim().trim_matches(|c| c == '"' || c == ',' || c == ' ');
    Some(value.to_string())
}

/// Log a step using pre-execution PC/opcode but post-execution registers
/// This matches CEmu's trace behavior where:
/// - PC and opcode are captured BEFORE execution (shows what instruction ran)
/// - Registers are captured AFTER execution (shows the result)
fn log_step_info_post(writer: &mut BufWriter<File>, step: u64, info: &StepInfo, emu: &Emu) {
    // Use POST-execution register values from emu (like CEmu does)
    let af = ((emu.a() as u16) << 8) | (emu.f() as u16);

    // Format opcode bytes (from pre-execution state)
    let op_str = match info.opcode_len {
        4 => format!("{:02X}{:02X}{:02X}{:02X}", info.opcode[0], info.opcode[1], info.opcode[2], info.opcode[3]),
        3 => format!("{:02X}{:02X}{:02X}", info.opcode[0], info.opcode[1], info.opcode[2]),
        2 => format!("{:02X}{:02X}", info.opcode[0], info.opcode[1]),
        _ => format!("{:02X}", info.opcode[0]),
    };

    let im = emu.interrupt_mode();
    let im_str = format!("{:?}", im).replace("IM", "Mode");

    // Use total_cycles after execution
    let cycles_after = info.total_cycles;

    writeln!(
        writer,
        "{:06} {:08} {:06X} {:06X} {:04X} {:06X} {:06X} {:06X} {:06X} {:06X} {} {} {} {} {} {}",
        step, cycles_after, info.pc, emu.sp(), af, emu.bc(), emu.de(), emu.hl(), emu.ix(), emu.iy(),
        if emu.adl() { 1 } else { 0 },
        if emu.iff1() { 1 } else { 0 },
        if emu.iff2() { 1 } else { 0 },
        im_str,
        if emu.is_halted() { 1 } else { 0 },
        op_str
    ).expect("Failed to write trace line");
}

/// Log a step using pre-execution state from StepInfo (legacy, for compatibility)
#[allow(dead_code)]
fn log_step_info(writer: &mut BufWriter<File>, step: u64, info: &StepInfo) {
    let af = ((info.a as u16) << 8) | (info.f as u16);

    // Format opcode bytes
    let op_str = match info.opcode_len {
        4 => format!("{:02X}{:02X}{:02X}{:02X}", info.opcode[0], info.opcode[1], info.opcode[2], info.opcode[3]),
        3 => format!("{:02X}{:02X}{:02X}", info.opcode[0], info.opcode[1], info.opcode[2]),
        2 => format!("{:02X}{:02X}", info.opcode[0], info.opcode[1]),
        _ => format!("{:02X}", info.opcode[0]),
    };

    let im_str = format!("{:?}", info.im).replace("IM", "Mode");

    // Use total_cycles - cycles to get cycles BEFORE this instruction (for step 0 parity)
    let cycles_before = info.total_cycles.saturating_sub(info.cycles as u64);

    writeln!(
        writer,
        "{:06} {:08} {:06X} {:06X} {:04X} {:06X} {:06X} {:06X} {:06X} {:06X} {} {} {} {} {} {}",
        step, cycles_before, info.pc, info.sp, af, info.bc, info.de, info.hl, info.ix, info.iy,
        if info.adl { 1 } else { 0 },
        if info.iff1 { 1 } else { 0 },
        if info.iff2 { 1 } else { 0 },
        im_str,
        if info.halted { 1 } else { 0 },
        op_str
    ).expect("Failed to write trace line");
}

/// Legacy log function for compatibility (uses current emu state)
#[allow(dead_code)]
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

    // Check the source of MathPrint value
    let mp_source = 0xD01171u32;
    println!("\n=== MathPrint Source Address ===");
    println!("0xD01171 (source of MathPrint value): 0x{:02X}", emu.peek_byte(mp_source));
    println!("  This value is loaded and written to 0xD000C4 by ROM code at 0x0008AED0");

    // Check if there's anything interesting around 0xD01170
    println!("\n=== Memory around 0xD01170 ===");
    for offset in 0..16u32 {
        let addr = 0xD01170 + offset;
        print!("{:02X} ", emu.peek_byte(addr));
    }
    println!();
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

// === Send File Command ===

/// Load ROM, inject .8xp/.8xv files into flash archive, boot, and render screenshot
fn cmd_sendfile(files: &[String]) {
    // Load ROM
    let rom_data = match load_rom() {
        Some(data) => data,
        None => return,
    };

    let mut emu = Emu::new();
    emu.load_rom(&rom_data).expect("Failed to load ROM");

    println!("\n=== Send File Test ===\n");

    // Inject each file
    let mut total_entries = 0;
    for file_path in files {
        let file_data = match fs::read(file_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read {}: {}", file_path, e);
                continue;
            }
        };

        println!("Sending: {} ({} bytes)", file_path, file_data.len());

        match emu.send_file(&file_data) {
            Ok(count) => {
                println!("  Injected {} entries", count);
                total_entries += count;
            }
            Err(code) => {
                eprintln!("  ERROR: send_file returned {}", code);
            }
        }
    }

    if total_entries == 0 {
        eprintln!("\nNo files were injected. Aborting.");
        return;
    }

    println!("\nTotal entries injected: {}", total_entries);

    // Power on and boot
    println!("\nBooting with injected files...");
    emu.press_on_key();

    let chunk_size = 1_000_000;
    let max_cycles = 300_000_000u64;
    let mut total_executed = 0u64;
    let mut idle_loop_count = 0;
    const IDLE_LOOP_THRESHOLD: u32 = 3;

    while total_executed < max_cycles {
        let executed = emu.run_cycles(chunk_size);
        total_executed += executed as u64;

        let pc = emu.pc();

        // Progress every 10M cycles
        if total_executed % 10_000_000 < chunk_size as u64 {
            println!(
                "[{:.1}M cycles] PC={:06X} halted={}",
                total_executed as f64 / 1_000_000.0,
                pc,
                emu.is_halted()
            );
        }

        // Check for idle loop
        if (0x085B7D..=0x085B80).contains(&pc) {
            idle_loop_count += 1;
            let lcd = emu.lcd_snapshot();
            if idle_loop_count >= IDLE_LOOP_THRESHOLD && lcd.control & 1 != 0 {
                println!(
                    "\nBoot complete at PC={:06X} after {:.2}M cycles",
                    pc,
                    total_executed as f64 / 1_000_000.0
                );
                break;
            }
        }

        if emu.is_halted() {
            let lcd = emu.lcd_snapshot();
            if (0x085B7D..=0x085B80).contains(&pc) && lcd.control & 1 != 0 {
                println!(
                    "\nBoot complete (halted) at PC={:06X} after {:.2}M cycles",
                    pc,
                    total_executed as f64 / 1_000_000.0
                );
                break;
            }
            println!(
                "\nHALT at PC={:06X} after {:.2}M cycles",
                pc,
                total_executed as f64 / 1_000_000.0
            );
            break;
        }
    }

    // Render frame and save screenshot
    emu.render_frame();

    let lcd = emu.lcd_snapshot();
    let bpp_mode = emu.lcd_snapshot().control >> 1 & 0x7;
    println!("\n=== LCD State ===");
    println!("Control: 0x{:08X} (enabled={}, bpp_mode={})", lcd.control, (lcd.control & 1) != 0, bpp_mode);
    println!("VRAM base: 0x{:06X}", lcd.upbase);

    // Save as PPM
    let output = "sendfile_result.ppm";
    save_framebuffer_ppm(&emu, output);
    println!("\nScreenshot saved to: {}", output);

    // Try to convert to PNG
    let png_output = "sendfile_result.png";
    let result = Command::new("sips")
        .args(["-s", "format", "png", output, "--out", png_output])
        .output();

    if result.is_ok() {
        fs::remove_file(output).ok();
        println!("Converted to: {}", png_output);
    }

    // Analyze framebuffer
    println!("\n=== Framebuffer Analysis ===");
    analyze_framebuffer(&emu);

    // Check if programs are visible in VAT
    println!("\n=== Flash Archive Check ===");
    let archive_start = 0x0C0000u32;
    let archive_end = 0x3B0000u32;
    let sector_size = 0x10000u32;
    let mut addr = archive_start;
    let mut found = 0;
    let mut deleted = 0;
    // Scan sector by sector
    while addr < archive_end {
        let sector_base = addr & !(sector_size - 1);
        // If at sector boundary, byte 0 is sector status — skip to byte 1
        if addr == sector_base {
            let status = emu.peek_byte(addr);
            if status == 0xFF {
                break; // Empty sector = end of archive
            }
            println!("  Sector 0x{:06X}: status=0x{:02X}", sector_base, status);
            addr += 1; // Skip sector status byte, entries start at byte 1
            continue;
        }
        let flag = emu.peek_byte(addr);
        if flag == 0xFF {
            // End of entries in this sector — move to next sector
            addr = sector_base + sector_size;
            continue;
        }
        if flag == 0xF0 {
            // Deleted entry — skip using size field
            let size = u16::from_le_bytes([
                emu.peek_byte(addr + 1),
                emu.peek_byte(addr + 2),
            ]) as u32;
            if size > 0 && size < sector_size {
                deleted += 1;
                addr += 3 + size;
                continue;
            } else {
                addr = sector_base + sector_size;
                continue;
            }
        }
        if flag != 0xFC && flag != 0xFE {
            // Unknown flag — skip to next sector
            addr = sector_base + sector_size;
            continue;
        }

        // Valid entry (0xFC) or in-progress (0xFE)
        let size = u16::from_le_bytes([
            emu.peek_byte(addr + 1),
            emu.peek_byte(addr + 2),
        ]) as u32;
        // Read: type1, type2, version, self-addr(3), namelen, name
        let var_type = emu.peek_byte(addr + 3);
        let _type2 = emu.peek_byte(addr + 4);
        let _version = emu.peek_byte(addr + 5);
        let name_len = emu.peek_byte(addr + 9) as u32;
        let mut name = String::new();
        for i in 0..name_len.min(8) {
            let ch = emu.peek_byte(addr + 10 + i);
            if ch >= 0x20 && ch < 0x7F {
                name.push(ch as char);
            }
        }
        let type_name = match var_type {
            0x05 => "Program",
            0x06 => "ProtProg",
            0x15 => "AppVar",
            _ => "Unknown",
        };
        println!(
            "  0x{:06X}: {} \"{}\" (type=0x{:02X}, payload={})",
            addr, type_name, name, var_type, size
        );
        found += 1;

        // Advance past entry using size field
        if size > 0 && size < 0x10000 {
            addr += 3 + size;
        } else {
            addr += 1;
        }

        if found > 20 {
            println!("  ... (truncated)");
            break;
        }
    }
    println!("Archive: {} valid, {} deleted, free at 0x{:06X}", found, deleted, addr);
}

// === Bake ROM Command ===

/// Load ROM, inject .8xp/.8xv files into flash archive, and save modified ROM
fn cmd_bakerom(output_path: &str, files: &[String]) {
    // Load ROM
    let rom_data = match load_rom() {
        Some(data) => data,
        None => return,
    };

    let mut emu = Emu::new();
    emu.load_rom(&rom_data).expect("Failed to load ROM");

    println!("\n=== Bake ROM ===\n");
    println!("Original ROM: {} bytes", rom_data.len());

    // Inject each file
    let mut total_entries = 0;
    for file_path in files {
        let file_data = match fs::read(file_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read {}: {}", file_path, e);
                continue;
            }
        };

        println!("Injecting: {} ({} bytes)", file_path, file_data.len());

        match emu.send_file(&file_data) {
            Ok(count) => {
                println!("  {} entries", count);
                total_entries += count;
            }
            Err(code) => {
                eprintln!("  ERROR: send_file returned {}", code);
            }
        }
    }

    if total_entries == 0 && !files.is_empty() {
        eprintln!("\nNo files were injected. Aborting.");
        return;
    }

    println!("\nTotal entries injected: {}", total_entries);

    // Write modified flash to output file
    let flash = emu.flash_data();
    fs::write(output_path, flash).expect("Failed to write output ROM");

    println!("Wrote {} to: {} ({} bytes)",
        if total_entries > 0 { "baked ROM" } else { "ROM copy" },
        output_path, flash.len());
    println!("\nThis ROM can be loaded directly — programs will appear in TI-OS.");
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

// === Run DOOM Test ===

/// Count non-white pixels in VRAM (quick hash for change detection)
fn vram_pixel_count(emu: &mut Emu) -> (u32, u32) {
    let upbase = 0xD40000u32;
    let mut non_white = 0u32;
    let mut black = 0u32;
    for i in (0..320*240*2).step_by(2) {
        let lo = emu.peek_byte(upbase + i as u32);
        let hi = emu.peek_byte(upbase + i as u32 + 1);
        let pixel = (lo as u16) | ((hi as u16) << 8);
        if pixel != 0xFFFF { non_white += 1; }
        if pixel == 0x0000 { black += 1; }
    }
    (non_white, black)
}

/// Dump key TI-OS variables for debugging text rendering
fn dump_os_state(emu: &mut Emu, label: &str) {
    let cur_row = emu.peek_byte(0xD00595);
    let cur_col = emu.peek_byte(0xD00596);
    let pen_col = emu.peek_byte(0xD008D2) as u16 | ((emu.peek_byte(0xD008D3) as u16) << 8);
    let pen_row = emu.peek_byte(0xD008D5);
    let fg_lo = emu.peek_byte(0xD02688);
    let fg_hi = emu.peek_byte(0xD02689);
    let bg_lo = emu.peek_byte(0xD0268A);
    let bg_hi = emu.peek_byte(0xD0268B);
    let fg = (fg_lo as u16) | ((fg_hi as u16) << 8);
    let bg = (bg_lo as u16) | ((bg_hi as u16) << 8);
    let mathprint = emu.peek_byte(0xD000C4);
    let lcd_ctrl = emu.peek_byte(0xE30018) as u32
        | ((emu.peek_byte(0xE30019) as u32) << 8)
        | ((emu.peek_byte(0xE3001A) as u32) << 16)
        | ((emu.peek_byte(0xE3001B) as u32) << 24);
    let bpp = (lcd_ctrl >> 1) & 7;
    let upbase = emu.peek_byte(0xE30010) as u32
        | ((emu.peek_byte(0xE30011) as u32) << 8)
        | ((emu.peek_byte(0xE30012) as u32) << 16)
        | ((emu.peek_byte(0xE30013) as u32) << 24);

    println!("  OS state [{}]:", label);
    println!("    curRow={} curCol={} penCol={} penRow={}", cur_row, cur_col, pen_col, pen_row);
    println!("    drawFGColor={:04X} drawBGColor={:04X} ({})",
        fg, bg, if fg == bg { "SAME! invisible text" } else { "ok" });
    println!("    mathprint_flags={:02X} ({}) LCD_CTRL={:08X} BPP={} UPBASE={:06X}",
        mathprint, if mathprint & 0x20 != 0 { "MathPrint" } else { "Classic" },
        lcd_ctrl, bpp, upbase);

    // Check plotSScreen for content (0xD52C00, 8400 bytes)
    let mut plot_nonzero = 0u32;
    for i in 0..8400u32 {
        if emu.peek_byte(0xD52C00 + i) != 0 { plot_nonzero += 1; }
    }
    println!("    plotSScreen: {} non-zero bytes (of 8400)", plot_nonzero);

    // Check a sample of VRAM at row 40 center (where text might be)
    let sample_addr = 0xD40000u32 + 40 * 640 + 160 * 2; // row 40, col 160
    print!("    VRAM@row40,col160: ");
    for i in 0..8 { print!("{:02X}", emu.peek_byte(sample_addr + i)); }
    println!();

    // Check userMem region (D1A881) for program code
    let user_mem = 0xD1A881u32;
    print!("    userMem@{:06X}: ", user_mem);
    for i in 0..16 { print!("{:02X} ", emu.peek_byte(user_mem + i)); }
    println!();
}

fn cmd_run(files: &[&str], timeout_secs: u64, speed: Option<f64>) {
    if files.is_empty() {
        eprintln!("No program file specified.");
        return;
    }

    let prog_path = files[0];
    let prog_name = Path::new(prog_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("UNKNOWN")
        .to_uppercase();

    // Load ROM
    let rom_data = match load_rom() {
        Some(data) => data,
        None => return,
    };

    let mut emu = Emu::new();
    emu.load_rom(&rom_data).expect("Failed to load ROM");

    // Inject all files (program + libraries)
    for file_path in files {
        let file_data = match fs::read(file_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read {}: {}", file_path, e);
                continue;
            }
        };
        eprintln!("Injecting: {} ({} bytes)", file_path, file_data.len());
        match emu.send_file(&file_data) {
            Ok(count) => eprintln!("  {} entries", count),
            Err(code) => eprintln!("  ERROR: send_file returned {}", code),
        }
    }

    // Enable debug port interception
    emu.enable_debug_ports();

    // Boot TI-OS
    eprintln!("Booting TI-OS...");
    emu.press_on_key();
    let mut total = 0u64;
    while total < 175_000_000 {
        total += emu.run_cycles(1_000_000) as u64;
    }
    emu.release_on_key();
    emu.run_cycles(2_000_000);
    eprintln!("Boot complete at {:.1}M cycles, PC={:06X}", total as f64 / 1e6, emu.pc());

    // Launch via sendKey: ENTER → CLEAR → Asm( → prgm → <NAME> → ENTER
    eprintln!("Launching Asm(prgm{})...", prog_name);
    send_os_key_wait(&mut emu, 0x05, "ENTER-init");
    send_os_key_wait(&mut emu, 0x09, "CLEAR");
    send_os_key_wait(&mut emu, 0xFC9C, "Asm(");
    send_os_key_wait(&mut emu, 0xDA, "prgm");
    for ch in prog_name.chars() {
        if ch.is_ascii_uppercase() {
            let key = 0x9A + (ch as u16 - 'A' as u16);
            send_os_key_wait(&mut emu, key, &format!("'{}'", ch));
        } else if ch.is_ascii_digit() {
            let key = 0x80 + (ch as u16 - '0' as u16);
            send_os_key_wait(&mut emu, key, &format!("'{}'", ch));
        }
    }
    send_os_key_wait(&mut emu, 0x05, "ENTER-exec");
    eprintln!("Program launched.");

    // Run loop with debug output capture
    let timeout_cycles = timeout_secs * 48_000_000;
    let mut exec_cycles = 0u64;
    let wall_start = Instant::now();

    // Speed control: None = unthrottled, Some(N) = N * 800K cycles per 16ms frame
    let throttled = speed.is_some();
    let cycles_per_frame = speed.map(|s| (800_000.0 * s) as u32).unwrap_or(1_000_000);
    let frame_duration = std::time::Duration::from_millis(16);

    if throttled {
        eprintln!("Running at {:.1}x speed (timeout: {}s)...", speed.unwrap(), timeout_secs);
    } else {
        eprintln!("Running unthrottled (timeout: {}s)...", timeout_secs);
    }

    loop {
        let frame_start = Instant::now();

        exec_cycles += emu.run_cycles(cycles_per_frame) as u64;

        // Drain and print debug output
        for line in emu.take_debug_stdout() {
            print!("{}", line);
        }
        for line in emu.take_debug_stderr() {
            eprint!("{}", line);
        }

        // Check termination conditions
        if emu.debug_terminated() {
            // Flush any remaining buffered output
            for line in emu.take_debug_stdout() {
                print!("{}", line);
            }
            let wall_elapsed = wall_start.elapsed().as_secs_f64();
            eprintln!("\n[Terminated via null sentinel after {:.2}M cycles, {:.2}s wall time]",
                exec_cycles as f64 / 1e6, wall_elapsed);
            break;
        }
        if exec_cycles >= timeout_cycles {
            let wall_elapsed = wall_start.elapsed().as_secs_f64();
            eprintln!("\n[Timeout after {}s ({:.2}M cycles, {:.2}s wall time)]",
                timeout_secs, exec_cycles as f64 / 1e6, wall_elapsed);
            break;
        }
        if emu.is_off() {
            let wall_elapsed = wall_start.elapsed().as_secs_f64();
            eprintln!("\n[Calculator powered off after {:.2}M cycles, {:.2}s wall time]",
                exec_cycles as f64 / 1e6, wall_elapsed);
            break;
        }

        // Throttle if speed-limited
        if throttled {
            let elapsed = frame_start.elapsed();
            if elapsed < frame_duration {
                std::thread::sleep(frame_duration - elapsed);
            }
        }
    }
}

fn cmd_runprog(files: &[&str], post_launch_cycles: u64) {
    // First file must be the .8xp program
    let prog_path = files[0];
    let prog_name = Path::new(prog_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("UNKNOWN")
        .to_uppercase();

    println!("\n=== Run Program: {} ===\n", prog_name);

    // Load ROM
    let rom_data = match load_rom() {
        Some(data) => data,
        None => return,
    };

    let mut emu = Emu::new();
    emu.load_rom(&rom_data).expect("Failed to load ROM");

    // Inject all files (program + libraries)
    let mut total_entries = 0;
    for file_path in files {
        let file_data = match fs::read(file_path) {
            Ok(data) => data,
            Err(e) => {
                eprintln!("Failed to read {}: {}", file_path, e);
                continue;
            }
        };
        println!("Injecting: {} ({} bytes)", file_path, file_data.len());
        match emu.send_file(&file_data) {
            Ok(count) => {
                println!("  {} entries", count);
                total_entries += count;
            }
            Err(code) => {
                eprintln!("  ERROR: send_file returned {}", code);
            }
        }
    }

    if total_entries == 0 {
        eprintln!("No files were injected. Aborting.");
        return;
    }

    // Phase 1: Boot TI-OS
    println!("\nPhase 1: Booting TI-OS...");
    emu.press_on_key();
    let mut total = 0u64;
    while total < 175_000_000 {
        total += emu.run_cycles(1_000_000) as u64;
    }
    println!("Boot complete. PC={:06X}, halted={}", emu.pc(), emu.is_halted());

    emu.release_on_key();
    emu.run_cycles(2_000_000);

    // Dump VAT to verify program is registered
    dump_vat(&mut emu);

    // Phase 2: Launch via sendKey — type Asm(prgm<NAME>)
    println!("\nPhase 2: Launching {} via sendKey...", prog_name);

    // ENTER to dismiss boot screen
    println!("  ENTER (dismiss boot screen)");
    send_os_key_wait(&mut emu, 0x05, "ENTER-init");

    // CLEAR for clean homescreen
    println!("  CLEAR");
    send_os_key_wait(&mut emu, 0x09, "CLEAR");

    // Asm( token
    println!("  Asm( token");
    send_os_key_wait(&mut emu, 0xFC9C, "Asm(");

    // prgm token
    println!("  prgm token");
    send_os_key_wait(&mut emu, 0xDA, "prgm");

    // Type program name letter by letter
    println!("  Type {}", prog_name);
    for ch in prog_name.chars() {
        if ch.is_ascii_uppercase() {
            let key = 0x9A + (ch as u16 - 'A' as u16);
            send_os_key_wait(&mut emu, key, &format!("'{}'", ch));
        } else if ch.is_ascii_digit() {
            // Digits: 0=0x80, 1=0x81, ... 9=0x89
            let key = 0x80 + (ch as u16 - '0' as u16);
            send_os_key_wait(&mut emu, key, &format!("'{}'", ch));
        }
    }

    // Screenshot before execution
    emu.render_frame();
    save_framebuffer_ppm(&emu, "/tmp/runprog_before.ppm");
    convert_ppm_to_png("/tmp/runprog_before.ppm", "/tmp/runprog_before.png");
    println!("\n  Pre-exec screenshot: /tmp/runprog_before.png");

    // Phase 3: Execute!
    println!("\nPhase 3: ENTER to execute Asm(prgm{})...", prog_name);
    send_os_key_wait(&mut emu, 0x05, "ENTER-exec");

    // Run for the requested number of cycles
    println!("  Running {:.0}M cycles...", post_launch_cycles as f64 / 1_000_000.0);
    let mut ran = 0u64;
    while ran < post_launch_cycles {
        let chunk = std::cmp::min(1_000_000, (post_launch_cycles - ran) as u32);
        ran += emu.run_cycles(chunk) as u64;
    }
    println!("  PC={:06X} halted={}", emu.pc(), emu.is_halted());

    // Final screenshot
    emu.render_frame();
    save_framebuffer_ppm(&emu, "/tmp/runprog_after.ppm");
    convert_ppm_to_png("/tmp/runprog_after.ppm", "/tmp/runprog_after.png");

    println!("\nScreenshots:");
    println!("  Before: /tmp/runprog_before.png");
    println!("  After:  /tmp/runprog_after.png");
}

fn cmd_rundoom() {
    // Load the baked ROM that has DOOM + clibs pre-installed
    let rom_path = "/tmp/TI84CE-DOOM.rom";
    let rom_data = match fs::read(rom_path) {
        Ok(data) => {
            eprintln!("Loaded baked ROM from: {} ({:.2} MB)", rom_path, data.len() as f64 / 1024.0 / 1024.0);
            data
        }
        Err(_) => {
            eprintln!("Baked ROM not found at {}. Create it first with:", rom_path);
            eprintln!("  cargo run --release --example debug -- bakerom /tmp/TI84CE-DOOM.rom ~/Downloads/DOOM.8xp ~/Downloads/*.8xv");
            return;
        }
    };

    let mut emu = Emu::new();
    emu.load_rom(&rom_data).expect("Failed to load ROM");
    emu.press_on_key();

    println!("\n=== DOOM Launch Test (sendKey approach) ===\n");

    // Phase 1: Boot TI-OS
    println!("Phase 1: Booting TI-OS...");
    let mut total = 0u64;
    while total < 175_000_000 {
        total += emu.run_cycles(1_000_000) as u64;
    }
    println!("Boot complete. PC={:06X}, halted={}", emu.pc(), emu.is_halted());

    emu.release_on_key();
    emu.run_cycles(2_000_000);

    // Dump VAT to verify DOOM is registered
    dump_vat(&mut emu);

    // Phase 2: Use CEmu-style sendKey to launch Asm(prgmDOOM)
    // This bypasses the hardware keypad and writes directly to OS key buffer
    println!("\nPhase 2: Launching DOOM via sendKey (CEmu-style)...");

    // Step 1: ENTER to dismiss boot screen and init parser
    println!("  Step 1: ENTER (dismiss boot screen)");
    send_os_key_wait(&mut emu, 0x05, "ENTER-init");

    // Step 2: CLEAR to go to clean homescreen
    println!("  Step 2: CLEAR");
    send_os_key_wait(&mut emu, 0x09, "CLEAR");

    // Step 3: Type Asm( token (0xFC9C = kExtendEcho2 | kAsm)
    println!("  Step 3: Asm( token");
    send_os_key_wait(&mut emu, 0xFC9C, "Asm(");

    // Step 4: PRGM token (0xDA) - inserts 'prgm' on homescreen
    println!("  Step 4: prgm token");
    send_os_key_wait(&mut emu, 0xDA, "prgm");

    // Step 5: Type DOOM letter by letter
    println!("  Step 5: Type DOOM");
    for ch in ['D', 'O', 'O', 'M'] {
        let key = 0x9A + (ch as u16 - 'A' as u16);
        send_os_key_wait(&mut emu, key, &format!("'{}'", ch));
    }

    // Take screenshot showing Asm(prgmDOOM on homescreen
    // Note: CEmu doesn't send close paren - TI-OS handles it implicitly
    emu.render_frame();
    save_framebuffer_ppm(&emu, "/tmp/doom_before_exec.ppm");
    convert_ppm_to_png("/tmp/doom_before_exec.ppm", "/tmp/doom_before_exec.png");

    // Dump homescreen state
    let cursor_row = emu.peek_byte(0xD00595);
    let cursor_col = emu.peek_byte(0xD00598);
    println!("\n  Homescreen: curRow={}, curCol={}", cursor_row, cursor_col);

    // Check what's in kbdKey area
    print!("  kbdKey @D0058C: ");
    for i in 0..8 { print!("{:02X} ", emu.peek_byte(0xD0058C + i)); }
    println!();

    // Check userMem before execution
    print!("  userMem @D1A881 (before): ");
    for i in 0..16 { print!("{:02X} ", emu.peek_byte(0xD1A881 + i)); }
    println!();

    // Phase 3: Execute!
    println!("\nPhase 3: ENTER to execute Asm(prgmDOOM)...");
    send_os_key_wait(&mut emu, 0x05, "ENTER-exec");

    // Run 50M cycles to get into the DOOM/LibLoad execution
    println!("  Running 50M cycles to reach LibLoad...");
    emu.run_cycles(50_000_000);
    println!("  PC={:06X} halted={}", emu.pc(), emu.is_halted());

    // Now enable instruction tracing to capture the loop
    println!("  Enabling instruction trace (1000 instructions)...");
    let _ = fs::remove_file("emu.log"); // clear old log
    emu_core::enable_inst_trace(1000);
    emu.run_cycles(5_000_000); // run a bit with tracing
    emu_core::disable_inst_trace();

    // Read and analyze the trace
    if let Ok(log_data) = fs::read_to_string("emu.log") {
        let lines: Vec<&str> = log_data.lines()
            .filter(|l| l.starts_with("INST["))
            .collect();
        println!("  Captured {} traced instructions", lines.len());

        // Show first 20 and last 20
        println!("\n  === First 20 instructions ===");
        for line in lines.iter().take(20) {
            println!("  {}", line);
        }
        if lines.len() > 40 {
            println!("\n  ... ({} instructions omitted) ...", lines.len() - 40);
            println!("\n  === Last 20 instructions ===");
            for line in lines.iter().skip(lines.len() - 20) {
                println!("  {}", line);
            }
        }

        // PC histogram
        let mut pc_hist: HashMap<String, u32> = HashMap::new();
        for line in &lines {
            if let Some(pc_start) = line.find("PC=") {
                let pc_str = &line[pc_start+3..pc_start+9];
                *pc_hist.entry(pc_str.to_string()).or_insert(0) += 1;
            }
        }
        let mut pc_counts: Vec<_> = pc_hist.iter().collect();
        pc_counts.sort_by(|a, b| b.1.cmp(a.1));
        println!("\n  === Top 15 PCs (most visited) ===");
        for (pc, count) in pc_counts.iter().take(15) {
            println!("    PC={}: {} times ({:.1}%)", pc, count, **count as f64 / lines.len() as f64 * 100.0);
        }
    } else {
        println!("  WARNING: Could not read emu.log");
    }

    // Check final state
    println!("\n=== Final State ===");
    let bpp = (emu.peek_byte(0xE30018) >> 1) & 0x7;
    let errno = emu.peek_byte(0xD008DF);
    println!("PC={:06X} halted={} BPP={} errno=0x{:02X}", emu.pc(), emu.is_halted(), bpp, errno);

    print!("userMem @D1A881: ");
    for i in 0..32 { print!("{:02X} ", emu.peek_byte(0xD1A881 + i)); }
    println!();

    // Dump the archive data around the LibLoad library
    println!("\n  LibLoad archive entry @0C6CBE:");
    print!("    Header: ");
    for i in 0..20 { print!("{:02X} ", emu.peek_byte(0x0C6CBE + i)); }
    println!();

    // Check what's at key flash addresses the CPU is visiting
    println!("\n  Flash data at key PCs:");
    for addr in [0x0C6DB0u32, 0x0C6DC0, 0x0C6DCA, 0x0B3B70] {
        print!("    @{:06X}: ", addr);
        for i in 0..16 { print!("{:02X} ", emu.peek_byte(addr + i)); }
        println!();
    }

    // Take final screenshot
    emu.render_frame();
    save_framebuffer_ppm(&emu, "/tmp/doom_after_exec.ppm");
    convert_ppm_to_png("/tmp/doom_after_exec.ppm", "/tmp/doom_after_exec.png");

    println!("\nScreenshots: /tmp/doom_after_exec.png");
}

/// Send an OS-level key via CEmu's sendKey mechanism and wait for consumption
fn send_os_key_wait(emu: &mut Emu, key: u16, name: &str) {
    const CE_GRAPH_FLAGS2: u32 = 0xD0009F;
    const CE_KEY_READY: u8 = 1 << 5;

    // Wait for OS to consume previous key
    let mut wait_cycles = 0u64;
    loop {
        let flags = emu.peek_byte(CE_GRAPH_FLAGS2);
        if flags & CE_KEY_READY == 0 { break; }
        if wait_cycles >= 50_000_000 {
            println!("    TIMEOUT: OS didn't consume previous key before {} (waited 50M cycles)", name);
            return;
        }
        emu.run_cycles(48_000);
        wait_cycles += 48_000;
    }

    // Send the key
    let ok = emu.send_key(key);
    if !ok {
        println!("    FAILED: send_key returned false for {} (0x{:04X})", name, key);
        return;
    }

    // Wait for OS to consume this key (run cycles until keyReady clears)
    let mut consumed_cycles = 0u64;
    loop {
        emu.run_cycles(48_000);
        consumed_cycles += 48_000;
        let flags = emu.peek_byte(CE_GRAPH_FLAGS2);
        if flags & CE_KEY_READY == 0 { break; }
        if consumed_cycles >= 50_000_000 {
            println!("    WARNING: OS didn't consume {} after 50M cycles, PC={:06X}, halted={}",
                name, emu.pc(), emu.is_halted());
            return;
        }
    }

    // Extra settle time for the OS to process the key action
    emu.run_cycles(2_000_000);

    println!("    {} (0x{:04X}): consumed after {}K cycles, PC={:06X}",
        name, key, consumed_cycles / 1000, emu.pc());
}

fn press_key(emu: &mut Emu, row: usize, col: usize, hold_cycles: u32, release_cycles: u32) {
    emu.set_key(row, col, true);
    emu.run_cycles(hold_cycles);
    emu.set_key(row, col, false);
    emu.run_cycles(release_cycles);
}

fn press_key_verbose(emu: &mut Emu, name: &str, row: usize, col: usize, hold_cycles: u32, release_cycles: u32) {
    println!("  Pressing {} (row={}, col={})...", name, row, col);
    emu.set_key(row, col, true);
    let held = emu.run_cycles(hold_cycles);
    println!("    Hold: ran {} of {} cycles, PC={:06X}, halted={}, is_off={}",
        held, hold_cycles, emu.pc(), emu.is_halted(), emu.is_off());
    emu.set_key(row, col, false);
    let released = emu.run_cycles(release_cycles);
    println!("    Release: ran {} of {} cycles, PC={:06X}, halted={}, is_off={}",
        released, release_cycles, emu.pc(), emu.is_halted(), emu.is_off());
}

fn convert_ppm_to_png(ppm_path: &str, png_path: &str) {
    let _ = Command::new("sips")
        .args(["-s", "format", "png", ppm_path, "--out", png_path])
        .output();
    let _ = fs::remove_file(ppm_path);
}

/// Dump TI-OS Variable Allocation Table to see registered programs
fn dump_vat(emu: &mut Emu) {
    // Read key pointers from RAM
    let prog_ptr = emu.peek_byte(0xD0259D) as u32
        | (emu.peek_byte(0xD0259E) as u32) << 8
        | (emu.peek_byte(0xD0259F) as u32) << 16;
    let p_temp = emu.peek_byte(0xD0259A) as u32
        | (emu.peek_byte(0xD0259B) as u32) << 8
        | (emu.peek_byte(0xD0259C) as u32) << 16;
    let op_base = emu.peek_byte(0xD02590) as u32
        | (emu.peek_byte(0xD02591) as u32) << 8
        | (emu.peek_byte(0xD02592) as u32) << 16;

    println!("\n  === VAT State ===");
    println!("  progPtr = {:06X}", prog_ptr);
    println!("  pTemp   = {:06X}", p_temp);
    println!("  OPBase  = {:06X}", op_base);
    println!("  symTable = D3FFFF");

    // Scan VAT entries backwards from symTable
    let sym_table = 0xD3FFFFu32;
    let user_mem = 0xD1A881u32;
    let mut vat = sym_table;
    let mut count = 0;
    let max_entries = 60;

    println!("\n  VAT Entries (scanning from D3FFFF backward):");
    while vat > user_mem && vat > op_base && vat <= sym_table && count < max_entries {
        let type1 = emu.peek_byte(vat);
        vat -= 1;
        let type2 = emu.peek_byte(vat);
        vat -= 1;
        let version = emu.peek_byte(vat);
        vat -= 1;
        let addr_lo = emu.peek_byte(vat) as u32;
        vat -= 1;
        let addr_mid = emu.peek_byte(vat) as u32;
        vat -= 1;
        let addr_hi = emu.peek_byte(vat) as u32;
        vat -= 1;
        let address = addr_lo | (addr_mid << 8) | (addr_hi << 16);

        // Check if this is a named entry (between pTemp and progPtr)
        let named = vat > p_temp && vat <= prog_ptr;
        // Print when we first enter the named (program) section
        if named && count > 0 {
            static mut PRINTED_BOUNDARY: bool = false;
            unsafe {
                if !PRINTED_BOUNDARY {
                    println!("    --- entered named/program section (vat={:06X}) ---", vat);
                    PRINTED_BOUNDARY = true;
                }
            }
        }

        let (namelen, name) = if named {
            let nl = emu.peek_byte(vat) as usize;
            vat -= 1;
            if nl == 0 || nl > 8 {
                println!("    #{}: INVALID namelen={}", count, nl);
                break;
            }
            let mut name_bytes = Vec::new();
            for _ in 0..nl {
                name_bytes.push(emu.peek_byte(vat));
                vat -= 1;
            }
            let name_str: String = name_bytes.iter()
                .map(|&b| if b >= 0x20 && b < 0x7F { b as char } else { '.' })
                .collect();
            (nl, name_str)
        } else {
            // Unnamed entries have 3-byte token names
            let mut name_bytes = Vec::new();
            for _ in 0..3 {
                name_bytes.push(emu.peek_byte(vat));
                vat -= 1;
            }
            let name_str: String = name_bytes.iter()
                .map(|&b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join("");
            (3, name_str)
        };

        let var_type = type1 & 0x3F;
        let archived = address > 0xC0000 && address < 0x400000;
        let type_name = match var_type {
            0x05 => "Program",
            0x06 => "ProtProg",
            0x15 => "AppVar",
            0x00 => "Real",
            0x01 => "List",
            0x04 => "String",
            _ => "Other",
        };

        println!("    #{:2}: type={:02X}({:8}) type2={:02X} ver={:02X} addr={:06X} name={:8} arch={} named={}",
            count, var_type, type_name, type2, version, address, name, archived, named);

        count += 1;

        // Sanity check: if address is 0 or all FF, VAT is probably empty/corrupt
        if address == 0 || address == 0xFFFFFF {
            println!("    (stopping: likely end of VAT)");
            break;
        }
    }
    println!("  Total VAT entries found: {}", count);
}

/// Comprehensive VRAM dump — counts all non-white pixels (including black text)
fn dump_vram_full(emu: &mut Emu, label: &str) {
    let upbase = 0xD40000u32;
    let mut non_white_count = 0u32;
    let mut black_count = 0u32;
    let mut content_rows = Vec::new(); // rows with ANY non-white pixels

    for row in 0..240u32 {
        let row_base = upbase + row * 640;
        let mut row_non_white = 0u32;
        let mut row_black = 0u32;
        for col in (0..640u32).step_by(2) {
            let lo = emu.peek_byte(row_base + col);
            let hi = emu.peek_byte(row_base + col + 1);
            let pixel = (lo as u16) | ((hi as u16) << 8);
            if pixel != 0xFFFF {
                non_white_count += 1;
                row_non_white += 1;
                if pixel == 0x0000 {
                    black_count += 1;
                    row_black += 1;
                }
            }
        }
        if row_non_white > 0 && content_rows.len() < 20 {
            content_rows.push((row, row_non_white, row_black));
        }
    }

    println!("  VRAM [{}]: {} non-white px ({} black) across {} rows",
        label, non_white_count, black_count, content_rows.len());
    for &(row, nw, bl) in content_rows.iter().take(10) {
        println!("    row {:3}: {} non-white ({} black)", row, nw, bl);
    }
    if content_rows.len() > 10 {
        println!("    ... and {} more rows", content_rows.len() - 10);
    }
}
