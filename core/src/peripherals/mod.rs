//! TI-84 Plus CE Peripheral Emulation
//!
//! This module contains the memory-mapped peripheral controllers:
//! - Control Ports (0xE00000, 0xFF0000)
//! - Flash Controller (0xE10000)
//! - Interrupt Controller (0xF00000)
//! - Timers (0xF20000)
//! - LCD Controller (0xE30000)
//! - Keypad Controller (0xF50000)
//! - Watchdog Timer (0xF60000)
//! - Backlight Controller (0xFB0000)

pub mod backlight;
pub mod control;
pub mod flash;
pub mod interrupt;
pub mod keypad;
pub mod lcd;
pub mod panel;
pub mod rtc;
pub mod sha256;
pub mod spi;
pub mod timer;
pub mod watchdog;

pub use backlight::Backlight;
pub use control::ControlPorts;
pub use flash::FlashController;
pub use interrupt::InterruptController;
pub use keypad::{KeypadController, KEYPAD_COLS, KEYPAD_ROWS};
pub use lcd::{LcdController, LCD_HEIGHT, LCD_WIDTH};
pub use rtc::RtcController;
pub use sha256::Sha256Controller;
pub use spi::SpiController;
pub use timer::GeneralTimers;
pub use watchdog::WatchdogController;

use interrupt::sources;

/// Port address regions (offsets from 0xE00000)
const CONTROL_BASE: u32 = 0x000000; // 0xE00000
const CONTROL_END: u32 = 0x000100;
const FLASH_BASE: u32 = 0x010000; // 0xE10000
const FLASH_END: u32 = 0x010100;
const SHA256_BASE: u32 = 0x020000; // 0xE20000
const SHA256_END: u32 = 0x020100;
const CONTROL_ALT_BASE: u32 = 0x1F0000; // 0xFF0000 (accessed via OUT0/IN0)
const CONTROL_ALT_END: u32 = 0x1F0100;
const LCD_BASE: u32 = 0x030000; // 0xE30000
const LCD_END: u32 = 0x031000; // 0xE31000 — includes palette (0x200-0x3FF) and periph ID (0xFE0)
const INT_BASE: u32 = 0x100000; // 0xF00000
const INT_END: u32 = 0x100020;
const TIMER_BASE: u32 = 0x120000; // 0xF20000
const TIMER_END: u32 = 0x120040;
const KEYPAD_BASE: u32 = 0x150000; // 0xF50000
const KEYPAD_END: u32 = 0x150048; // Covers up to GPIO status (index 0x11)
const WATCHDOG_BASE: u32 = 0x160000; // 0xF60000
const WATCHDOG_END: u32 = 0x160100;
const RTC_BASE: u32 = 0x180000; // 0xF80000
const RTC_END: u32 = 0x180100;
const BACKLIGHT_BASE: u32 = 0x1B0000; // 0xFB0000
const BACKLIGHT_END: u32 = 0x1B0100;

/// Peripheral subsystem containing all hardware controllers
#[derive(Debug, Clone)]
pub struct Peripherals {
    /// Control ports (0xE00000, 0xFF0000)
    pub control: ControlPorts,
    /// Flash controller (0xE10000)
    pub flash: FlashController,
    /// Interrupt controller
    pub interrupt: InterruptController,
    /// General purpose timers (3 timers with shared control/status)
    pub timers: GeneralTimers,
    /// LCD controller
    pub lcd: LcdController,
    /// Keypad controller
    pub keypad: KeypadController,
    /// Watchdog controller
    pub watchdog: WatchdogController,
    /// RTC controller
    pub rtc: RtcController,
    /// SHA256 accelerator
    pub sha256: Sha256Controller,
    /// Backlight controller
    pub backlight: Backlight,
    /// Fallback register storage for unmapped ports
    fallback: Vec<u8>,
    /// Keypad state (updated by Emu)
    key_state: [[bool; KEYPAD_COLS]; KEYPAD_ROWS],
    /// OS Timer state (32KHz crystal-based timer, bit 4 interrupt)
    os_timer_state: bool,
    /// OS Timer cycle accumulator
    os_timer_cycles: u64,
}

impl Peripherals {
    /// Size of fallback register storage
    const FALLBACK_SIZE: usize = 0x200000;

    /// OS Timer tick intervals (in 32KHz ticks) based on CPU speed
    /// From CEmu: ost_ticks[4] = { 73, 153, 217, 313 }
    const OS_TIMER_TICKS: [u32; 4] = [73, 153, 217, 313];

    /// 32KHz crystal frequency
    const CLOCK_32K: u32 = 32768;

    /// Create new peripheral subsystem
    pub fn new() -> Self {
        Self {
            control: ControlPorts::new(),
            flash: FlashController::new(),
            interrupt: InterruptController::new(),
            timers: GeneralTimers::new(),
            lcd: LcdController::new(),
            keypad: KeypadController::new(),
            watchdog: WatchdogController::new(),
            rtc: RtcController::new(),
            sha256: Sha256Controller::new(),
            backlight: Backlight::new(),
            fallback: vec![0x00; Self::FALLBACK_SIZE],
            key_state: [[false; KEYPAD_COLS]; KEYPAD_ROWS],
            os_timer_state: false,
            os_timer_cycles: 0,
        }
    }

    /// Update keypad state from emulator
    /// Sets key_state and edge flag, and raises keypad interrupt on press.
    ///
    /// CEmu's emu_keypad_event sets the atomic flags and signals CPU.
    /// The TI-OS then checks keypad registers during interrupt handling.
    pub fn set_key(&mut self, row: usize, col: usize, pressed: bool) {
        if row < KEYPAD_ROWS && col < KEYPAD_COLS {
            self.key_state[row][col] = pressed;

            // Set edge flag on key press (CEmu sets both current and edge bits)
            // Edge flag persists until queried by any_key_check, allowing
            // detection of quick press/release even if released before query
            self.keypad.set_key_edge(row, col, pressed);

            // Raise keypad interrupt on key press so TI-OS will check the keypad
            // This is critical for TI-OS to detect keys when the keypad is in mode 0
            if pressed {
                self.interrupt.raise(sources::KEYPAD);
            }
        }
    }

    /// Get current key state
    pub fn key_state(&self) -> &[[bool; KEYPAD_COLS]; KEYPAD_ROWS] {
        &self.key_state
    }

    /// Reset all peripherals
    pub fn reset(&mut self) {
        self.control.reset();
        self.flash.reset();
        self.interrupt.reset();
        self.timers.reset();
        self.lcd.reset();
        self.keypad.reset();
        self.watchdog.reset();
        self.rtc.reset();
        self.sha256.reset();
        self.fallback.fill(0x00);
        self.key_state = [[false; KEYPAD_COLS]; KEYPAD_ROWS];
        self.os_timer_state = false;
        self.os_timer_cycles = 0;
    }

    /// Read from a port address
    /// addr is offset from 0xE00000
    /// key_state is the current keyboard matrix
    /// current_cycles: CPU cycle count for timing-sensitive peripherals
    pub fn read(
        &mut self,
        addr: u32,
        key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS],
        current_cycles: u64,
    ) -> u8 {
        // Get CPU speed for timing calculations
        let cpu_speed = self.control.cpu_speed();
        match addr {
            // Control Ports (0xE00000 - 0xE000FF)
            a if a >= CONTROL_BASE && a < CONTROL_END => self.control.read(a - CONTROL_BASE),

            // Flash Controller (0xE10000 - 0xE100FF)
            a if a >= FLASH_BASE && a < FLASH_END => self.flash.read(a - FLASH_BASE),

            // SHA256 Accelerator (0xE20000 - 0xE200FF)
            a if a >= SHA256_BASE && a < SHA256_END => self.sha256.read(a - SHA256_BASE),

            // Control Ports alternate (0xFF0000 - 0xFF00FF, via OUT0/IN0)
            a if a >= CONTROL_ALT_BASE && a < CONTROL_ALT_END => {
                self.control.read(a - CONTROL_ALT_BASE)
            }

            // LCD Controller (0xE30000 - 0xE300FF)
            a if a >= LCD_BASE && a < LCD_END => self.lcd.read(a - LCD_BASE),

            // Interrupt Controller (0xF00000 - 0xF0001F)
            a if a >= INT_BASE && a < INT_END => self.interrupt.read(a - INT_BASE),

            // Timers (0xF20000 - 0xF2003F)
            a if a >= TIMER_BASE && a < TIMER_END => self.timers.read(a - TIMER_BASE),

            // Keypad Controller (0xF50000 - 0xF5003F)
            a if a >= KEYPAD_BASE && a < KEYPAD_END => self.keypad.read(a - KEYPAD_BASE, key_state),

            // Watchdog Controller (0xF60000 - 0xF600FF)
            a if a >= WATCHDOG_BASE && a < WATCHDOG_END => self.watchdog.read(a - WATCHDOG_BASE),

            // RTC Controller (0xF80000 - 0xF800FF)
            a if a >= RTC_BASE && a < RTC_END => self.rtc.read(a - RTC_BASE, current_cycles, cpu_speed),

            // Backlight Controller (0xFB0000 - 0xFB00FF)
            a if a >= BACKLIGHT_BASE && a < BACKLIGHT_END => self.backlight.read(a - BACKLIGHT_BASE),

            // Unmapped - return from fallback storage
            _ => {
                let offset = (addr as usize) % Self::FALLBACK_SIZE;
                self.fallback[offset]
            }
        }
    }

    /// Write to a port address
    /// addr is offset from 0xE00000
    /// current_cycles: CPU cycle count for timing-sensitive peripherals
    pub fn write(&mut self, addr: u32, value: u8, current_cycles: u64) {
        // Get CPU speed for timing calculations
        let cpu_speed = self.control.cpu_speed();

        match addr {
            // Control Ports (0xE00000 - 0xE000FF)
            a if a >= CONTROL_BASE && a < CONTROL_END => self.control.write(a - CONTROL_BASE, value),

            // Flash Controller (0xE10000 - 0xE100FF)
            a if a >= FLASH_BASE && a < FLASH_END => self.flash.write(a - FLASH_BASE, value),

            // SHA256 Accelerator (0xE20000 - 0xE200FF)
            a if a >= SHA256_BASE && a < SHA256_END => self.sha256.write(a - SHA256_BASE, value),

            // Control Ports alternate (0xFF0000 - 0xFF00FF, via OUT0/IN0)
            a if a >= CONTROL_ALT_BASE && a < CONTROL_ALT_END => {
                self.control.write(a - CONTROL_ALT_BASE, value)
            }

            // LCD Controller (0xE30000 - 0xE300FF)
            a if a >= LCD_BASE && a < LCD_END => self.lcd.write(a - LCD_BASE, value),

            // Interrupt Controller (0xF00000 - 0xF0001F)
            a if a >= INT_BASE && a < INT_END => self.interrupt.write(a - INT_BASE, value),

            // Timers (0xF20000 - 0xF2003F)
            a if a >= TIMER_BASE && a < TIMER_END => {
                self.timers.write(a - TIMER_BASE, value);
                // CEmu: after any timer register write, recalculate interrupt state
                // for all 3 timers based on (status & mask). This is critical for the
                // ISR to clear timer interrupts by writing to the status register.
                let int_state = self.timers.interrupt_state();
                let timer_sources = [sources::TIMER1, sources::TIMER2, sources::TIMER3];
                for (i, &src) in timer_sources.iter().enumerate() {
                    if int_state & (1 << i) != 0 {
                        self.interrupt.raise(src);
                    } else {
                        self.interrupt.clear_raw(src);
                    }
                }
            }

            // Keypad Controller (0xF50000 - 0xF5003F)
            a if a >= KEYPAD_BASE && a < KEYPAD_END => {
                let offset = a - KEYPAD_BASE;

                let flag_before = self.keypad.needs_any_key_check;
                self.keypad.write(offset, value);
                let flag_after = self.keypad.needs_any_key_check;

                if flag_after && !flag_before {
                    crate::emu::log_evt!("KEYPAD: offset=0x{:02X} set needs_any_key_check flag", offset);
                }

                // CEmu calls keypad_any_check() after certain writes (STATUS, SIZE, CONTROL mode 0/1)
                // This updates data registers with current key state
                if self.keypad.needs_any_key_check {
                    self.keypad.needs_any_key_check = false;

                    let should_interrupt = self.keypad.any_key_check(&self.key_state);

                    // Update keypad interrupt state
                    if should_interrupt {
                        self.interrupt.raise(sources::KEYPAD);
                    } else {
                        self.interrupt.clear_raw(sources::KEYPAD);
                    }
                }
            }

            // Watchdog Controller (0xF60000 - 0xF600FF)
            a if a >= WATCHDOG_BASE && a < WATCHDOG_END => self.watchdog.write(a - WATCHDOG_BASE, value),

            // RTC Controller (0xF80000 - 0xF800FF)
            a if a >= RTC_BASE && a < RTC_END => {
                self.rtc.write(a - RTC_BASE, value, current_cycles, cpu_speed)
            }

            // Backlight Controller (0xFB0000 - 0xFB00FF)
            a if a >= BACKLIGHT_BASE && a < BACKLIGHT_END => {
                self.backlight.write(a - BACKLIGHT_BASE, value)
            }

            // Unmapped - store in fallback
            _ => {
                let offset = (addr as usize) % Self::FALLBACK_SIZE;
                self.fallback[offset] = value;
            }
        }
    }

    /// Tick all peripherals
    /// delay_remaining: CPU cycles remaining until the TimerDelay event fires (0 if not active)
    /// Returns true if any interrupt is pending
    pub fn tick(&mut self, cycles: u32, delay_remaining: u64) -> bool {
        // Tick timers (pass CPU speed for 32kHz clock source support)
        // Timer interrupts are deferred through the 2-cycle delay pipeline.
        // The caller (emu.rs) checks timers.needs_delay_event and schedules
        // EventId::TimerDelay, which calls process_delay() to apply status
        // and raise interrupts.
        let cpu_speed = self.control.cpu_speed();
        self.timers.tick(cycles, cpu_speed, delay_remaining);

        // Sync timer interrupt state after tick — ensures stale raw bits are cleared
        // when timers no longer have active status. Without this, timer raw bits
        // accumulate from scheduler events but are only cleared when the ISR writes
        // to the timer registers. If the interrupt wasn't enabled, the ISR never ran,
        // and raw stays permanently set.
        let int_state = self.timers.interrupt_state();
        let timer_sources = [sources::TIMER1, sources::TIMER2, sources::TIMER3];
        for (i, &src) in timer_sources.iter().enumerate() {
            if int_state & (1 << i) != 0 {
                self.interrupt.raise(src);
            } else {
                self.interrupt.clear_raw(src);
            }
        }

        // LCD interrupts are now driven by scheduler events (EventId::Lcd / EventId::LcdDma)
        // in emu.rs, matching CEmu's lcd_event()/lcd_dma() architecture.
        // Check LCD scheduling flags set by control register writes.
        // (The actual scheduling is done by emu.rs which checks these flags.)

        // Tick keypad scan timing and update interrupt state
        // CEmu calls intrpt_set(INT_KEYPAD, status & enable) which sets OR clears raw
        let keypad_scan_irq = self.keypad.tick(cycles, &self.key_state);
        let keypad_any_irq = self.keypad.check_interrupt(&self.key_state);
        if keypad_scan_irq || keypad_any_irq {
            self.interrupt.raise(sources::KEYPAD);
        } else {
            self.interrupt.clear_raw(sources::KEYPAD);
        }

        // Tick OS Timer (32KHz crystal-based timer)
        self.tick_os_timer(cycles);

        self.interrupt.irq_pending()
    }

    /// Tick the OS Timer (32KHz crystal timer, generates bit 4 interrupt)
    /// Based on CEmu's ost_event in timers.c
    ///
    /// CEmu order (from timers.c ost_event):
    ///   1. intrpt_set(INT_OSTIMER, gpt.osTimerState)  — set interrupt to OLD state
    ///   2. sched_repeat(id, ...)                       — reschedule
    ///   3. gpt.osTimerState = !gpt.osTimerState        — toggle state
    fn tick_os_timer(&mut self, cycles: u32) {
        // Get CPU speed from control port (bits 0-1)
        let speed = (self.control.read(0x01) & 0x03) as usize;

        // CPU clock rates: 6MHz, 12MHz, 24MHz, 48MHz
        let cpu_clock: u64 = match speed {
            0 => 6_000_000,
            1 => 12_000_000,
            2 => 24_000_000,
            _ => 48_000_000,
        };

        // Cycles per 32KHz tick at current CPU speed
        let cycles_per_32k_tick = cpu_clock / Self::CLOCK_32K as u64;

        self.os_timer_cycles += cycles as u64;

        // Check if enough cycles have passed to toggle state
        loop {
            // OS Timer interval in 32K ticks depends on state:
            // - When state is false: wait ost_ticks[speed] ticks
            // - When state is true: wait 1 tick
            let ticks_needed = if self.os_timer_state {
                1u64
            } else {
                Self::OS_TIMER_TICKS[speed] as u64
            };
            let cycles_needed = ticks_needed * cycles_per_32k_tick;

            if self.os_timer_cycles < cycles_needed {
                break;
            }

            self.os_timer_cycles -= cycles_needed;

            // CEmu order: set interrupt to OLD state FIRST, then toggle
            // intrpt_set(INT_OSTIMER, gpt.osTimerState) with old state
            if self.os_timer_state {
                self.interrupt.raise(sources::OSTIMER);
            } else {
                self.interrupt.clear_raw(sources::OSTIMER);
            }

            // Toggle state AFTER setting interrupt
            self.os_timer_state = !self.os_timer_state;
        }
    }

    /// Check if any interrupt is pending
    pub fn irq_pending(&self) -> bool {
        self.interrupt.irq_pending()
    }

    // ========== State Persistence ==========

    /// Size of peripheral state snapshot in bytes
    /// Control(32) + Flash(8) + Interrupt(32) + Timers(3×24) + LCD(40) + Keypad(16) + RTC(16) + OS Timer(16) + KeyState(8) + padding = ~256
    pub const SNAPSHOT_SIZE: usize = 256;

    /// Save peripheral state to bytes
    pub fn to_bytes(&self) -> [u8; Self::SNAPSHOT_SIZE] {
        let mut buf = [0u8; Self::SNAPSHOT_SIZE];
        let mut pos = 0;

        // Control ports - essential registers (32 bytes)
        buf[pos] = self.control.read(0x00); pos += 1;  // power
        buf[pos] = self.control.read(0x01); pos += 1;  // cpu_speed
        buf[pos] = self.control.read(0x03); pos += 1;  // device_type
        buf[pos] = self.control.read(0x05); pos += 1;  // control_flags
        buf[pos] = self.control.read(0x06); pos += 1;  // unlock_status
        buf[pos] = self.control.read(0x0D); pos += 1;  // lcd_enable
        buf[pos] = self.control.read(0x0F); pos += 1;  // usb_control
        buf[pos] = self.control.read(0x28); pos += 1;  // flash_unlock
        // Privileged boundary (3 bytes at 0x1D-0x1F)
        buf[pos] = self.control.read(0x1D); pos += 1;
        buf[pos] = self.control.read(0x1E); pos += 1;
        buf[pos] = self.control.read(0x1F); pos += 1;
        // Protected port boundaries (6 bytes at 0x20-0x25)
        buf[pos] = self.control.read(0x20); pos += 1;
        buf[pos] = self.control.read(0x21); pos += 1;
        buf[pos] = self.control.read(0x22); pos += 1;
        buf[pos] = self.control.read(0x23); pos += 1;
        buf[pos] = self.control.read(0x24); pos += 1;
        buf[pos] = self.control.read(0x25); pos += 1;
        pos += 15; // Padding to 32 bytes

        // Flash controller (8 bytes)
        buf[pos] = self.flash.read(0x00); pos += 1;  // enabled
        buf[pos] = self.flash.read(0x01); pos += 1;  // size_config
        buf[pos] = self.flash.read(0x02); pos += 1;  // map_select
        buf[pos] = self.flash.read(0x05); pos += 1;  // wait_states
        pos += 4; // Padding to 8 bytes

        // Interrupt controller (32 bytes)
        buf[pos..pos+4].copy_from_slice(&self.interrupt.status_word(0).to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.interrupt.status_word(1).to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.interrupt.enabled_word(0).to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.interrupt.enabled_word(1).to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.interrupt.latched_word(0).to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.interrupt.latched_word(1).to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.interrupt.inverted_word(0).to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.interrupt.inverted_word(1).to_le_bytes()); pos += 4;

        // Timers (3 × 24 bytes = 72 bytes)
        for i in 0..3 {
            buf[pos..pos+4].copy_from_slice(&self.timers.counter(i).to_le_bytes()); pos += 4;
            buf[pos..pos+4].copy_from_slice(&self.timers.reset_value(i).to_le_bytes()); pos += 4;
            buf[pos..pos+4].copy_from_slice(&self.timers.match_val(i, 0).to_le_bytes()); pos += 4;
            buf[pos..pos+4].copy_from_slice(&self.timers.match_val(i, 1).to_le_bytes()); pos += 4;
            buf[pos..pos+4].copy_from_slice(&self.timers.accum_cycles(i).to_le_bytes()); pos += 4;
            pos += 4; // Padding to 24 bytes
        }

        // LCD controller (24 bytes)
        buf[pos..pos+4].copy_from_slice(&self.lcd.control().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.lcd.upbase().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.lcd.int_mask().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.lcd.int_status().to_le_bytes()); pos += 4;
        buf[pos] = self.lcd.compare_state(); pos += 1;
        pos += 7; // Padding to 24 bytes

        // OS Timer state (16 bytes)
        buf[pos] = if self.os_timer_state { 1 } else { 0 }; pos += 1;
        pos += 7; // Align to 8 bytes
        buf[pos..pos+8].copy_from_slice(&self.os_timer_cycles.to_le_bytes()); pos += 8;

        // Key state as bit-packed (8 bytes - 64 bits for 8x8 matrix)
        for row in 0..KEYPAD_ROWS {
            let mut row_bits = 0u8;
            for col in 0..KEYPAD_COLS {
                if self.key_state[row][col] {
                    row_bits |= 1 << col;
                }
            }
            buf[pos] = row_bits;
            pos += 1;
        }

        // LCD DMA state (32 bytes) — timing registers + DMA progress
        // Without these, LCD DMA runs with zero timing after state restore,
        // causing ~100x faster refresh cycles and massive CPU overhead.
        let timing = self.lcd.timing();
        for t in &timing {
            buf[pos..pos+4].copy_from_slice(&t.to_le_bytes()); pos += 4;
        }
        buf[pos..pos+4].copy_from_slice(&self.lcd.upcurr().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.lcd.cur_row().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.lcd.cur_col().to_le_bytes()); pos += 4;
        buf[pos] = if self.lcd.prefill() { 1 } else { 0 }; pos += 1;
        buf[pos] = self.lcd.pos(); pos += 1;
        pos += 2; // Padding to 32 bytes

        // Timer control/status/mask words (12 bytes)
        buf[pos..pos+4].copy_from_slice(&self.timers.control_word().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.timers.status_word().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.timers.mask_word().to_le_bytes());

        buf
    }

    /// Load peripheral state from bytes
    pub fn from_bytes(&mut self, buf: &[u8]) -> Result<(), i32> {
        if buf.len() < Self::SNAPSHOT_SIZE {
            return Err(-105);
        }

        let mut pos = 0;

        // Control ports
        self.control.write(0x00, buf[pos]); pos += 1;
        self.control.write(0x01, buf[pos]); pos += 1;
        self.control.write(0x03, buf[pos]); pos += 1;
        self.control.write(0x05, buf[pos]); pos += 1;
        self.control.write(0x06, buf[pos]); pos += 1;
        self.control.write(0x0D, buf[pos]); pos += 1;
        self.control.write(0x0F, buf[pos]); pos += 1;
        self.control.write(0x28, buf[pos]); pos += 1;
        self.control.write(0x1D, buf[pos]); pos += 1;
        self.control.write(0x1E, buf[pos]); pos += 1;
        self.control.write(0x1F, buf[pos]); pos += 1;
        self.control.write(0x20, buf[pos]); pos += 1;
        self.control.write(0x21, buf[pos]); pos += 1;
        self.control.write(0x22, buf[pos]); pos += 1;
        self.control.write(0x23, buf[pos]); pos += 1;
        self.control.write(0x24, buf[pos]); pos += 1;
        self.control.write(0x25, buf[pos]); pos += 1;
        pos += 15;

        // Flash controller
        self.flash.write(0x00, buf[pos]); pos += 1;
        self.flash.write(0x01, buf[pos]); pos += 1;
        self.flash.write(0x02, buf[pos]); pos += 1;
        self.flash.write(0x05, buf[pos]); pos += 1;
        pos += 4;

        // Interrupt controller
        self.interrupt.set_status_word(0, u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.interrupt.set_status_word(1, u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.interrupt.set_enabled_word(0, u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.interrupt.set_enabled_word(1, u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.interrupt.set_latched_word(0, u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.interrupt.set_latched_word(1, u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.interrupt.set_inverted_word(0, u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.interrupt.set_inverted_word(1, u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;

        // Timers (3 × 24 bytes)
        for i in 0..3 {
            self.timers.set_counter(i, u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
            self.timers.set_reset_value(i, u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
            self.timers.set_match_val(i, 0, u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
            self.timers.set_match_val(i, 1, u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
            self.timers.set_accum_cycles(i, u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
            pos += 4; // Padding
        }

        // LCD controller
        self.lcd.set_control(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.lcd.set_upbase(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.lcd.set_int_mask(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.lcd.set_int_status(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.lcd.set_compare_state(buf[pos]); pos += 1;
        pos += 7;

        // OS Timer state
        self.os_timer_state = buf[pos] != 0; pos += 1;
        pos += 7;
        self.os_timer_cycles = u64::from_le_bytes(buf[pos..pos+8].try_into().unwrap()); pos += 8;

        // Key state
        for row in 0..KEYPAD_ROWS {
            let row_bits = buf[pos];
            for col in 0..KEYPAD_COLS {
                self.key_state[row][col] = (row_bits & (1 << col)) != 0;
            }
            pos += 1;
        }

        // LCD DMA state (32 bytes) — timing registers + DMA progress
        let mut timing = [0u32; 4];
        for t in &mut timing {
            *t = u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap()); pos += 4;
        }
        self.lcd.set_timing(timing);
        self.lcd.set_upcurr(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.lcd.set_cur_row(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.lcd.set_cur_col(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.lcd.set_prefill(buf[pos] != 0); pos += 1;
        self.lcd.set_pos(buf[pos]); pos += 1;
        pos += 2; // Skip padding

        // Re-derive timing parameters from restored timing registers
        self.lcd.recompute_timing();

        // Timer control/status/mask words (12 bytes)
        self.timers.set_control_word(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timers.set_status_word(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timers.set_mask_word(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap()));

        Ok(())
    }
}

impl Default for Peripherals {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_keys() -> [[bool; KEYPAD_COLS]; KEYPAD_ROWS] {
        [[false; KEYPAD_COLS]; KEYPAD_ROWS]
    }

    /// Helper trait for testing - adds read/write methods that don't require cycles
    trait PeripheralsTestExt {
        fn read_test(&mut self, addr: u32, keys: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> u8;
        fn write_test(&mut self, addr: u32, value: u8);
    }

    impl PeripheralsTestExt for Peripherals {
        fn read_test(&mut self, addr: u32, keys: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> u8 {
            self.read(addr, keys, 0)
        }

        fn write_test(&mut self, addr: u32, value: u8) {
            self.write(addr, value, 0)
        }
    }

    #[test]
    fn test_new() {
        let p = Peripherals::new();
        assert!(!p.irq_pending());
    }

    #[test]
    fn test_reset() {
        let mut p = Peripherals::new();

        // Modify some state
        p.interrupt.raise(sources::TIMER1);
        // Enable interrupt via write
        p.write_test(INT_BASE + 0x04, sources::TIMER1 as u8);
        p.timers.write(0x30, 0x01); // Enable timer 0
        p.set_key(0, 0, true);

        assert!(p.irq_pending());
        assert!(p.timers.is_enabled(0));
        assert!(p.key_state()[0][0]);

        // Reset
        p.reset();

        assert!(!p.irq_pending());
        assert!(!p.timers.is_enabled(0));
        assert!(!p.key_state()[0][0]);
    }

    #[test]
    fn test_set_key_bounds_check() {
        let mut p = Peripherals::new();

        // Should not panic for out-of-bounds
        p.set_key(100, 100, true);
        p.set_key(KEYPAD_ROWS, 0, true);
        p.set_key(0, KEYPAD_COLS, true);
    }

    #[test]
    fn test_lcd_routing() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Write to LCD upbase register (offset 0x10-0x13)
        p.write_test(LCD_BASE + 0x10, 0x00);
        p.write_test(LCD_BASE + 0x11, 0x00);
        p.write_test(LCD_BASE + 0x12, 0xD5);

        assert_eq!(p.lcd.upbase(), 0xD50000);

        // Read back
        assert_eq!(p.read_test(LCD_BASE + 0x10, &keys), 0x00);
        assert_eq!(p.read_test(LCD_BASE + 0x12, &keys), 0xD5);
    }

    #[test]
    fn test_timer_routing() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Write to timer 1 counter
        p.write_test(TIMER_BASE, 0x12);
        p.write_test(TIMER_BASE + 1, 0x34);

        // Read back
        assert_eq!(p.read_test(TIMER_BASE, &keys), 0x12);
        assert_eq!(p.read_test(TIMER_BASE + 1, &keys), 0x34);

        // Write timer control (bit 0 = timer 0 enable)
        p.write_test(TIMER_BASE + 0x30, 0x01);
        assert!(p.timers.is_enabled(0));
    }

    #[test]
    fn test_keypad_routing() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // All keys released - should read 0x00 (active-high: no bits set)
        assert_eq!(p.read_test(KEYPAD_BASE + 0x10, &keys), 0x00);

        // Set mode to 1 (any-key detection mode, like TI-OS uses)
        p.write_test(KEYPAD_BASE + 0x00, 0x01);

        // Press a key via set_key() which sets edge flag
        p.set_key(0, 3, true);

        // Trigger keypad check by writing to INT_STATUS (like TI-OS does)
        // This calls any_key_check which populates data registers from edges
        p.write_test(KEYPAD_BASE + 0x08, 0xFF);

        // Now reading should show bit 3 set (active-high: 1 = pressed)
        // In mode 1, data contains combined key data from any_key_check
        // Copy key_state first to avoid borrow conflict
        let keys_copy = *p.key_state();
        assert_eq!(p.read_test(KEYPAD_BASE + 0x10, &keys_copy), 1 << 3);
    }

    #[test]
    fn test_interrupt_routing() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Enable timer 1 interrupt
        p.write_test(INT_BASE + 0x04, sources::TIMER1 as u8);

        // Raise timer 1 interrupt
        p.interrupt.raise(sources::TIMER1);

        assert!(p.irq_pending());

        // Read interrupt status
        assert_eq!(p.read_test(INT_BASE, &keys), sources::TIMER1 as u8);
    }

    #[test]
    fn test_control_routing() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Write to control port (CPU speed)
        p.write_test(CONTROL_BASE + 0x01, 0x02); // 24 MHz
        assert_eq!(p.read_test(CONTROL_BASE + 0x01, &keys), 0x02);
        assert_eq!(p.control.cpu_speed(), 0x02);
    }

    #[test]
    fn test_control_alt_routing() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Write via alternate address (0xFF0000, which is offset 0x1F0000)
        p.write_test(CONTROL_ALT_BASE + 0x01, 0x01); // 12 MHz
        assert_eq!(p.read_test(CONTROL_ALT_BASE + 0x01, &keys), 0x01);

        // Should also be readable via primary address
        assert_eq!(p.read_test(CONTROL_BASE + 0x01, &keys), 0x01);
    }

    #[test]
    fn test_tick_timer_interrupt() {
        let mut p = Peripherals::new();

        // New control format: bit 0=enable, bit 2=overflow enable (count up = not inverted)
        p.write_test(TIMER_BASE + 0x30, 0x05); // Timer 0: enable + overflow
        // Set timer mask to trigger on overflow (bit 2 = timer 0 overflow)
        p.write_test(TIMER_BASE + 0x38, 0x04);
        // Set counter to max via write API
        p.write_test(TIMER_BASE, 0xFF);
        p.write_test(TIMER_BASE + 1, 0xFF);
        p.write_test(TIMER_BASE + 2, 0xFF);
        p.write_test(TIMER_BASE + 3, 0xFF);

        // Enable timer 1 interrupt in interrupt controller
        p.write_test(INT_BASE + 0x04, sources::TIMER1 as u8);

        // Tick should overflow timer — but interrupt is deferred through delay pipeline
        p.tick(2, 0);
        assert!(p.timers.needs_delay_event);

        // Simulate the delay pipeline processing (normally done by emu.rs TimerDelay event)
        process_timer_delays(&mut p);

        assert!(p.irq_pending());
    }

    #[test]
    fn test_lcd_enable_sets_scheduling_flag() {
        let mut p = Peripherals::new();

        // LCD interrupts are now event-driven (not tick-based).
        // Verify that enabling LCD sets the needs_lcd_event flag.
        assert!(!p.lcd.needs_lcd_event);
        p.write_test(LCD_BASE + 0x18, 0x01); // ENABLE (control bit 0)
        assert!(p.lcd.needs_lcd_event);

        // Verify that disabling LCD sets the needs_lcd_clear flag
        p.lcd.needs_lcd_event = false;
        p.write_test(LCD_BASE + 0x18, 0x00); // Disable
        assert!(p.lcd.needs_lcd_clear);
    }

    #[test]
    fn test_tick_keypad_interrupt() {
        let mut p = Peripherals::new();

        // Enable keypad in continuous mode with interrupt via write API
        p.write_test(KEYPAD_BASE + 0x00, 0x02); // CONTINUOUS mode
        p.write_test(KEYPAD_BASE + 0x0C, 0x04); // Enable any key interrupt

        // Enable keypad interrupt in interrupt controller (bit 10 - in byte 1)
        p.write_test(INT_BASE + 0x05, (sources::KEYPAD >> 8) as u8);

        // Press a key via internal key_state
        p.set_key(0, 0, true);

        // Tick should detect key and raise interrupt
        let pending = p.tick(1, 0);
        assert!(pending);
    }

    /// Helper to process all pending delay tiers and raise timer interrupts
    fn process_timer_delays(p: &mut Peripherals) {
        loop {
            if !p.timers.needs_delay_schedule() {
                break;
            }
            let (_status, intrpt, has_more) = p.timers.process_delay();
            for i in 0..3 {
                if intrpt & (1 << i) != 0 {
                    let source = match i {
                        0 => sources::TIMER1,
                        1 => sources::TIMER2,
                        2 => sources::TIMER3,
                        _ => unreachable!(),
                    };
                    p.interrupt.raise(source);
                }
            }
            if !has_more {
                break;
            }
        }
    }

    #[test]
    fn test_tick_multiple_timers() {
        let mut p = Peripherals::new();

        // New control: enable all 3 timers (bits 0,3,6) + overflow enable (bits 2,5,8)
        // All count up (not inverted)
        let ctrl: u32 = 0x01 | 0x04   // Timer 0: enable + overflow
                       | 0x08 | 0x20  // Timer 1: enable + overflow
                       | 0x40 | 0x100; // Timer 2: enable + overflow
        p.write_test(TIMER_BASE + 0x30, (ctrl & 0xFF) as u8);
        p.write_test(TIMER_BASE + 0x31, ((ctrl >> 8) & 0xFF) as u8);

        // Set mask for all overflow bits (2, 5, 8)
        let mask: u32 = 0x04 | 0x20 | 0x100;
        p.write_test(TIMER_BASE + 0x38, (mask & 0xFF) as u8);
        p.write_test(TIMER_BASE + 0x39, ((mask >> 8) & 0xFF) as u8);

        // Set counter to 0xFFFFFFFE for timer 0
        p.write_test(TIMER_BASE, 0xFE);
        p.write_test(TIMER_BASE + 1, 0xFF);
        p.write_test(TIMER_BASE + 2, 0xFF);
        p.write_test(TIMER_BASE + 3, 0xFF);

        // Set counter to 0xFFFFFFFD for timer 1
        p.write_test(TIMER_BASE + 0x10, 0xFD);
        p.write_test(TIMER_BASE + 0x11, 0xFF);
        p.write_test(TIMER_BASE + 0x12, 0xFF);
        p.write_test(TIMER_BASE + 0x13, 0xFF);

        // Set counter to 0xFFFFFFFC for timer 2
        p.write_test(TIMER_BASE + 0x20, 0xFC);
        p.write_test(TIMER_BASE + 0x21, 0xFF);
        p.write_test(TIMER_BASE + 0x22, 0xFF);
        p.write_test(TIMER_BASE + 0x23, 0xFF);

        // Enable all timer interrupts in interrupt controller
        let enabled = sources::TIMER1 | sources::TIMER2 | sources::TIMER3;
        p.write_test(INT_BASE + 0x04, enabled as u8);

        // Tick 2 cycles - should overflow timer 0 (deferred through delay pipeline)
        p.tick(2, 0);
        process_timer_delays(&mut p);

        // Tick 1 more - should overflow timer 1
        p.tick(1, 0);
        process_timer_delays(&mut p);

        // Tick 1 more - should overflow timer 2
        p.tick(1, 0);
        process_timer_delays(&mut p);

        // All 3 timers should have raised interrupts
        assert_ne!(p.interrupt.read(0x00) & sources::TIMER1 as u8, 0);
        assert_ne!(p.interrupt.read(0x00) & sources::TIMER2 as u8, 0);
        assert_ne!(p.interrupt.read(0x00) & sources::TIMER3 as u8, 0);
    }

    #[test]
    fn test_tick_no_interrupts_when_disabled() {
        let mut p = Peripherals::new();

        // Enable timer 0 with overflow in new control format
        p.write_test(TIMER_BASE + 0x30, 0x05); // enable + overflow
        p.write_test(TIMER_BASE + 0x38, 0x04); // mask overflow bit
        // Set counter to max
        p.write_test(TIMER_BASE, 0xFF);
        p.write_test(TIMER_BASE + 1, 0xFF);
        p.write_test(TIMER_BASE + 2, 0xFF);
        p.write_test(TIMER_BASE + 3, 0xFF);

        // But don't enable the interrupt in the interrupt controller
        // (enabled = 0 by default)

        // Tick should overflow timer — deferred through delay pipeline
        p.tick(2, 0);
        // Process delay to apply status bits (interrupt controller still won't fire because not enabled)
        process_timer_delays(&mut p);

        // Timer status is latched but interrupt controller shouldn't report pending
        assert!(!p.irq_pending());

        // Status should still be latched in interrupt controller raw register
        assert_ne!(p.interrupt.read(0x00), 0);
    }

    #[test]
    fn test_fallback_storage() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Write to unmapped address
        p.write_test(0x000100, 0xAB);
        assert_eq!(p.read_test(0x000100, &keys), 0xAB);
    }

    #[test]
    fn test_fallback_storage_wraps() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Write to address that wraps around fallback storage
        let addr = Peripherals::FALLBACK_SIZE as u32 + 0x100;
        p.write_test(addr, 0xCD);
        // Should read back at wrapped address
        assert_eq!(p.read_test(0x100, &keys), 0xCD);
    }

    #[test]
    fn test_flash_controller_routing() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Flash controller is at 0xE10000, offset 0x010000 from 0xE00000
        // CEmu default state: flash is enabled
        assert_eq!(p.read_test(FLASH_BASE + 0x00, &keys), 0x01); // enable
        assert_eq!(p.read_test(FLASH_BASE + 0x01, &keys), 0x00); // size config (CEmu memsets to 0)
        assert_eq!(p.read_test(FLASH_BASE + 0x02, &keys), 0x06); // CEmu defaults to 0x06
        assert_eq!(p.read_test(FLASH_BASE + 0x05, &keys), 0x04); // CEmu defaults to 0x04
        assert_eq!(p.read_test(FLASH_BASE + 0x08, &keys), 0x00); // control

        // Write to flash controller registers
        p.write_test(FLASH_BASE + 0x05, 0x08); // Set wait states to 8
        assert_eq!(p.read_test(FLASH_BASE + 0x05, &keys), 0x08);

        // Verify via direct access
        assert_eq!(p.flash.wait_states(), 0x08);
        assert_eq!(p.flash.total_wait_cycles(), 14); // 6 + 8
    }

    #[test]
    fn test_flash_controller_read_write() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Test enable register
        p.write_test(FLASH_BASE + 0x00, 0x00); // Disable flash
        assert_eq!(p.read_test(FLASH_BASE + 0x00, &keys), 0x00);
        assert!(!p.flash.is_enabled());

        p.write_test(FLASH_BASE + 0x00, 0x01); // Enable flash
        assert_eq!(p.read_test(FLASH_BASE + 0x00, &keys), 0x01);
        assert!(p.flash.is_enabled());

        // Test map select register
        p.write_test(FLASH_BASE + 0x02, 0x05);
        assert_eq!(p.read_test(FLASH_BASE + 0x02, &keys), 0x05);
        assert_eq!(p.flash.map_select(), 0x05);

        // Test that unmapped flash registers return 0xFF
        assert_eq!(p.read_test(FLASH_BASE + 0x03, &keys), 0xFF);
        assert_eq!(p.read_test(FLASH_BASE + 0x04, &keys), 0xFF);
    }

    #[test]
    fn test_flash_reset() {
        let mut p = Peripherals::new();

        // Modify flash state
        p.flash.write(0x00, 0x00); // Disable
        p.flash.write(0x05, 0x10); // Wait states
        assert!(!p.flash.is_enabled());

        // Reset should restore CEmu defaults
        p.reset();
        assert!(p.flash.is_enabled());
        assert_eq!(p.flash.wait_states(), 0x04); // CEmu default
        assert_eq!(p.flash.map_select(), 0x06); // CEmu default
    }
}
