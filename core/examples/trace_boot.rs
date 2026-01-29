//! Test ON key wake with pushed return address

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
    let log_every: u64 = 1; // Log every step for proper comparison with CEmu
    let mut last = Snapshot::capture(&mut emu);
    let pc = emu.pc();
    let (op_bytes, op_len) = peek_opcode_bytes(&mut emu, pc);
    println!(
        "[snapshot] step={} {} op={}",
        step,
        last.format_line(),
        format_opcode(op_bytes, op_len)
    );

    // Run until first HALT (or max cycles)
    for _ in 0..20000 {
        emu.run_cycles(1);
        step += 1;

        let snap = Snapshot::capture(&mut emu);
        if snap != last || (step % log_every == 0) {
            let note = if !last.halted && snap.halted {
                "HALT"
            } else if last.halted && !snap.halted {
                "WAKE"
            } else if last.irq_pending && !snap.irq_pending && !snap.iff1 {
                "IRQ taken"
            } else {
                ""
            };
            let (op_bytes, op_len) = peek_opcode_bytes(&mut emu, snap.pc);
            let op = format_opcode(op_bytes, op_len);
            if note.is_empty() {
                println!("[snapshot] step={} {} op={}", step, snap.format_line(), op);
            } else {
                println!(
                    "[snapshot] step={} {} op={}  {}",
                    step,
                    snap.format_line(),
                    op,
                    note
                );
            }
            last = snap;
        }

        if emu.is_halted() && step > 5000 {
            println!("\n=== HALTED at {:06X} ===", emu.pc());
            break;
        }
    }

    println!("\n=== Press ON key ===");
    emu.press_on_key();
    let snap = Snapshot::capture(&mut emu);
    println!("[snapshot] step={} {}", step, snap.format_line());
    last = snap;

    // Run after ON key to see if interrupt path executes
    for _ in 0..20000 {
        emu.run_cycles(1);
        step += 1;
        let snap = Snapshot::capture(&mut emu);
        if snap != last || (step % log_every == 0) {
            let note = if !last.halted && snap.halted {
                "HALT"
            } else if last.halted && !snap.halted {
                "WAKE"
            } else if last.irq_pending && !snap.irq_pending && !snap.iff1 {
                "IRQ taken"
            } else {
                ""
            };
            let (op_bytes, op_len) = peek_opcode_bytes(&mut emu, snap.pc);
            let op = format_opcode(op_bytes, op_len);
            if note.is_empty() {
                println!("[snapshot] step={} {} op={}", step, snap.format_line(), op);
            } else {
                println!(
                    "[snapshot] step={} {} op={}  {}",
                    step,
                    snap.format_line(),
                    op,
                    note
                );
            }
            last = snap;
        }

        if emu.is_halted() && step > 5000 {
            println!("\n=== HALTED at {:06X} ===", emu.pc());
            break;
        }
    }
}
