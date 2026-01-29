//! Boot trace logger - captures detailed emulator state during boot for CEmu comparison

use std::fs;
use std::path::Path;

use emu_core::cpu::InterruptMode;
use emu_core::{Emu, LcdSnapshot, TimerSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Snapshot {
    pc: u32,
    sp: u32,
    im: InterruptMode,
    adl: bool,
    iff1: bool,
    iff2: bool,
    halted: bool,
    irq_pending: bool,
    on_key_wake: bool,
    intr_status: u32,
    intr_enabled: u32,
    intr_raw: u32,
    power: u8,
    cpu_speed: u8,
    unlock_status: u8,
    flash_unlock: u8,
    timer1: TimerSnapshot,
    timer2: TimerSnapshot,
    timer3: TimerSnapshot,
    lcd: LcdSnapshot,
}

impl Snapshot {
    fn capture(emu: &mut Emu) -> Self {
        Self {
            pc: emu.pc(),
            sp: emu.sp(),
            im: emu.interrupt_mode(),
            adl: emu.adl(),
            iff1: emu.iff1(),
            iff2: emu.iff2(),
            halted: emu.is_halted(),
            irq_pending: emu.irq_pending(),
            on_key_wake: emu.on_key_wake(),
            intr_status: emu.interrupt_status(),
            intr_enabled: emu.interrupt_enabled(),
            intr_raw: emu.interrupt_raw(),
            power: emu.control_read(0x00),
            cpu_speed: emu.control_read(0x01),
            unlock_status: emu.control_read(0x06),
            flash_unlock: emu.control_read(0x28),
            timer1: emu.timer_snapshot(1).expect("timer1 snapshot"),
            timer2: emu.timer_snapshot(2).expect("timer2 snapshot"),
            timer3: emu.timer_snapshot(3).expect("timer3 snapshot"),
            lcd: emu.lcd_snapshot(),
        }
    }

    fn format_line(&self) -> String {
        format!(
            "PC={:06X} SP={:06X} IM={:?} ADL={} IFF1={} IFF2={} HALT={} IRQ_PEND={} ON_WAKE={} INTR[stat={:06X} en={:06X} raw={:06X}] CTRL[pwr={:02X} spd={:02X} unlock={:02X} flash={:02X}] T1[cnt={:08X} rst={:08X} m1={:08X} m2={:08X} ctl={:02X}] T2[cnt={:08X} rst={:08X} m1={:08X} m2={:08X} ctl={:02X}] T3[cnt={:08X} rst={:08X} m1={:08X} m2={:08X} ctl={:02X}] LCD[ctl={:08X} mask={:08X} stat={:08X} up={:06X} lp={:06X} pal={:06X} fc={:08X}]",
            self.pc,
            self.sp,
            self.im,
            self.adl,
            self.iff1,
            self.iff2,
            self.halted,
            self.irq_pending,
            self.on_key_wake,
            self.intr_status & 0x3FFFFF,
            self.intr_enabled & 0x3FFFFF,
            self.intr_raw & 0x3FFFFF,
            self.power,
            self.cpu_speed,
            self.unlock_status,
            self.flash_unlock,
            self.timer1.counter,
            self.timer1.reset_value,
            self.timer1.match1,
            self.timer1.match2,
            self.timer1.control,
            self.timer2.counter,
            self.timer2.reset_value,
            self.timer2.match1,
            self.timer2.match2,
            self.timer2.control,
            self.timer3.counter,
            self.timer3.reset_value,
            self.timer3.match1,
            self.timer3.match2,
            self.timer3.control,
            self.lcd.control,
            self.lcd.int_mask,
            self.lcd.int_status,
            self.lcd.upbase,
            self.lcd.lpbase,
            self.lcd.palbase,
            self.lcd.frame_cycles,
        )
    }
}

fn peek_opcode_bytes(emu: &mut Emu, addr: u32) -> ([u8; 4], usize) {
    let read = |emu: &mut Emu, offset: u32| {
        let pc = addr.wrapping_add(offset);
        let effective = emu.mask_addr(pc);
        emu.peek_byte(effective)
    };
    let mut bytes = [0u8; 4];
    let first = read(emu, 0);
    bytes[0] = first;
    let len = match first {
        0xCB | 0xED => {
            bytes[1] = read(emu, 1);
            2
        }
        0xDD | 0xFD => {
            let second = read(emu, 1);
            bytes[1] = second;
            if second == 0xCB {
                bytes[2] = read(emu, 2);
                bytes[3] = read(emu, 3);
                4
            } else {
                2
            }
        }
        _ => 1,
    };
    (bytes, len)
}

fn format_opcode(bytes: [u8; 4], len: usize) -> String {
    bytes[..len]
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() {
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

    println!("=== Boot trace (from reset) ===\n");

    let mut step: u64 = 0;
    let log_every: u64 = 1_000_000; // Log progress every million cycles
    let mut last = Snapshot::capture(&mut emu);
    let mut last_pc = emu.pc();
    let pc = emu.pc();
    let (op_bytes, op_len) = peek_opcode_bytes(&mut emu, pc);
    println!(
        "[snapshot] step={} {} op={}",
        step,
        last.format_line(),
        format_opcode(op_bytes, op_len)
    );

    // Run until first HALT (or max cycles)
    // Need millions of cycles to get through delay loops
    for _ in 0..50_000_000 {
        emu.run_cycles(1);
        step += 1;

        let snap = Snapshot::capture(&mut emu);
        // Only log when PC leaves the delay loop region, state changes, or periodic progress
        let pc_changed = snap.pc != last_pc && (snap.pc < 0x5C55 || snap.pc > 0x5C58);
        let state_changed = snap.halted != last.halted || snap.iff1 != last.iff1;
        let periodic = step % log_every == 0;

        if pc_changed || state_changed || periodic {
            let note = if !last.halted && snap.halted {
                "HALT"
            } else if last.halted && !snap.halted {
                "WAKE"
            } else if !last.iff1 && snap.iff1 {
                "EI"
            } else if periodic {
                "progress"
            } else {
                ""
            };
            let (op_bytes, op_len) = peek_opcode_bytes(&mut emu, snap.pc);
            let op = format_opcode(op_bytes, op_len);
            eprintln!(
                "[snapshot] step={} {} op={}  {}",
                step,
                snap.format_line(),
                op,
                note
            );
            last = snap;
            last_pc = snap.pc;
        }

        if emu.is_halted() && step > 5000 {
            eprintln!("\n=== HALTED at {:06X} ===", emu.pc());
            break;
        }
    }

    eprintln!("\n=== First HALT complete at step {} ===", step);

    // Press ON key to wake from HALT
    eprintln!("\n=== Pressing ON key to wake ===\n");
    emu.press_on_key();

    // Continue execution after wake
    let wake_step = step;
    for _ in 0..50_000_000 {
        emu.run_cycles(1);
        step += 1;

        let snap = Snapshot::capture(&mut emu);
        // Only log when PC leaves the delay loop region, state changes, or periodic progress
        let pc_changed = snap.pc != last_pc && (snap.pc < 0x5C55 || snap.pc > 0x5C58);
        let state_changed = snap.halted != last.halted || snap.iff1 != last.iff1;
        let periodic = step % log_every == 0;

        if pc_changed || state_changed || periodic {
            let note = if !last.halted && snap.halted {
                "HALT"
            } else if last.halted && !snap.halted {
                "WAKE"
            } else if !last.iff1 && snap.iff1 {
                "EI"
            } else if periodic {
                "progress"
            } else {
                ""
            };
            let (op_bytes, op_len) = peek_opcode_bytes(&mut emu, snap.pc);
            let op = format_opcode(op_bytes, op_len);
            eprintln!(
                "[snapshot] step={} {} op={}  {}",
                step,
                snap.format_line(),
                op,
                note
            );
            last = snap;
            last_pc = snap.pc;
        }

        if emu.is_halted() && step > wake_step + 5000 {
            eprintln!("\n=== Second HALT at {:06X} ===", emu.pc());
            break;
        }
    }

    eprintln!("\n=== Boot phase complete at step {} ===", step);

    // Analyze VRAM contents
    eprintln!("\n=== VRAM/Framebuffer Analysis ===");

    // Get LCD state
    let lcd = emu.lcd_snapshot();
    eprintln!(
        "LCD: control={:08X} upbase={:06X}",
        lcd.control, lcd.upbase
    );

    // Check raw VRAM at the LCD upbase address
    let vram_base = lcd.upbase;
    eprintln!("\nVRAM sample at {:06X}:", vram_base);
    let mut non_zero_pixels = 0;
    let mut white_pixels = 0;
    let mut sample_values: Vec<u16> = Vec::new();
    for i in 0..320 * 240 {
        let addr = vram_base + (i * 2) as u32; // 2 bytes per RGB565 pixel
        let lo = emu.peek_byte(addr);
        let hi = emu.peek_byte(addr + 1);
        let rgb565 = (hi as u16) << 8 | (lo as u16);
        if rgb565 != 0 {
            non_zero_pixels += 1;
        }
        if rgb565 == 0xFFFF {
            white_pixels += 1;
        }
        // Sample some pixel values
        if i < 10 || (i >= 320 * 120 && i < 320 * 120 + 10) {
            sample_values.push(rgb565);
        }
    }
    eprintln!(
        "  Non-zero pixels: {} / {} ({:.1}%)",
        non_zero_pixels,
        320 * 240,
        100.0 * non_zero_pixels as f64 / (320.0 * 240.0)
    );
    eprintln!(
        "  White pixels (0xFFFF): {} / {} ({:.1}%)",
        white_pixels,
        320 * 240,
        100.0 * white_pixels as f64 / (320.0 * 240.0)
    );
    eprintln!("  First 10 pixels (row 0): {:04X?}", &sample_values[..10]);
    eprintln!(
        "  Middle 10 pixels (row 120): {:04X?}",
        &sample_values[10..20]
    );

    // Render to framebuffer and analyze
    emu.render_frame();
    let (w, h) = emu.framebuffer_size();
    let fb = emu.framebuffer_ptr();
    let fb_slice = unsafe { std::slice::from_raw_parts(fb, w * h) };

    let mut fb_non_black = 0;
    let mut fb_white = 0;
    for &pixel in fb_slice {
        if pixel != 0xFF000000 {
            // Not black
            fb_non_black += 1;
        }
        if pixel == 0xFFFFFFFF {
            fb_white += 1;
        }
    }
    eprintln!("\nFramebuffer ({w}x{h}):");
    eprintln!(
        "  Non-black pixels: {} / {} ({:.1}%)",
        fb_non_black,
        w * h,
        100.0 * fb_non_black as f64 / (w * h) as f64
    );
    eprintln!(
        "  White pixels: {} / {} ({:.1}%)",
        fb_white,
        w * h,
        100.0 * fb_white as f64 / (w * h) as f64
    );
    eprintln!("  First few pixels: {:08X?}", &fb_slice[..5]);
}
