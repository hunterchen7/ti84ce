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
pub use timer::{Timer, TimerSystem};
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
const LCD_END: u32 = 0x030100;
const INT_BASE: u32 = 0x100000; // 0xF00000
const INT_END: u32 = 0x100020;
const TIMER_BASE: u32 = 0x120000; // 0xF20000
const TIMER_END: u32 = 0x120040;
const KEYPAD_BASE: u32 = 0x150000; // 0xF50000
const KEYPAD_END: u32 = 0x150040;
const WATCHDOG_BASE: u32 = 0x160000; // 0xF60000
const WATCHDOG_END: u32 = 0x160100;
const RTC_BASE: u32 = 0x180000; // 0xF80000
const RTC_END: u32 = 0x180100;
#[allow(dead_code)]
const BACKLIGHT_BASE: u32 = 0x1B0000; // 0xFB0000
#[allow(dead_code)]
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
    /// Timer system (3 GPTs with global registers at 0xF20000-0xF2003F)
    pub timers: TimerSystem,
    /// Legacy timer 1 (kept for backward compatibility with existing code)
    pub timer1: Timer,
    /// Legacy timer 2
    pub timer2: Timer,
    /// Legacy timer 3
    pub timer3: Timer,
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
            timers: TimerSystem::new(),
            timer1: Timer::new(),
            timer2: Timer::new(),
            timer3: Timer::new(),
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
        self.timer1.reset();
        self.timer2.reset();
        self.timer3.reset();
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
            // Uses TimerSystem which has proper CEmu-style global registers at 0x30-0x3F
            a if a >= TIMER_BASE && a < TIMER_END => {
                let offset = a - TIMER_BASE;
                self.timers.read(offset)
            }

            // Keypad Controller (0xF50000 - 0xF5003F)
            a if a >= KEYPAD_BASE && a < KEYPAD_END => self.keypad.read(a - KEYPAD_BASE, key_state),

            // Watchdog Controller (0xF60000 - 0xF600FF)
            a if a >= WATCHDOG_BASE && a < WATCHDOG_END => self.watchdog.read(a - WATCHDOG_BASE),

            // RTC Controller (0xF80000 - 0xF800FF)
            a if a >= RTC_BASE && a < RTC_END => self.rtc.read(a - RTC_BASE, current_cycles, cpu_speed),

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
            // Uses TimerSystem which has proper CEmu-style global registers at 0x30-0x3F
            a if a >= TIMER_BASE && a < TIMER_END => {
                let offset = a - TIMER_BASE;
                self.timers.write(offset, value);
            }

            // Keypad Controller (0xF50000 - 0xF5003F)
            a if a >= KEYPAD_BASE && a < KEYPAD_END => {
                let offset = a - KEYPAD_BASE;

                // DIAGNOSTIC: Unconditional log to see if writes go through here
                static mut WRITE_COUNT: u32 = 0;
                #[allow(static_mut_refs)]
                unsafe {
                    WRITE_COUNT += 1;
                    if WRITE_COUNT % 10000 == 1 {
                        crate::emu::log_event(&format!(
                            "PERIPHERALS_KEYPAD_WRITE: offset=0x{:02X} value=0x{:02X} count={}",
                            offset, value, WRITE_COUNT
                        ));
                    }
                }

                let flag_before = self.keypad.needs_any_key_check;
                self.keypad.write(offset, value);
                let flag_after = self.keypad.needs_any_key_check;

                // Debug: log flag state changes
                if flag_after && !flag_before {
                    crate::emu::log_event(&format!(
                        "KEYPAD: offset=0x{:02X} set needs_any_key_check flag",
                        offset
                    ));
                }

                // CEmu calls keypad_any_check() after certain writes (STATUS, SIZE, CONTROL mode 0/1)
                // This updates data registers with current key state
                if self.keypad.needs_any_key_check {
                    self.keypad.needs_any_key_check = false;

                    // Log which register triggered the check
                    let reg_name = match offset {
                        0x00 => "CONTROL",
                        0x04..=0x07 => "SIZE",
                        0x08 => "INT_STATUS",
                        _ => "OTHER",
                    };
                    crate::emu::log_event(&format!("KEYPAD: {} write triggered any_key_check", reg_name));

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

            // Unmapped - store in fallback
            _ => {
                let offset = (addr as usize) % Self::FALLBACK_SIZE;
                self.fallback[offset] = value;
            }
        }
    }

    /// Tick all peripherals
    /// Returns true if any interrupt is pending
    pub fn tick(&mut self, cycles: u32) -> bool {
        // Tick timers (using TimerSystem with global registers)
        // Get CPU clock rate for 32K timer calculations
        let speed = (self.control.read(0x01) & 0x03) as usize;
        let cpu_clock: u64 = match speed {
            0 => 6_000_000,
            1 => 12_000_000,
            2 => 24_000_000,
            _ => 48_000_000,
        };
        let timer_irqs = self.timers.tick(cycles, cpu_clock);
        if timer_irqs & 0x01 != 0 {
            self.interrupt.raise(sources::TIMER1);
        }
        if timer_irqs & 0x02 != 0 {
            self.interrupt.raise(sources::TIMER2);
        }
        if timer_irqs & 0x04 != 0 {
            self.interrupt.raise(sources::TIMER3);
        }

        // Also tick legacy timers for backward compatibility with tests
        if self.timer1.tick(cycles) != 0 {
            self.interrupt.raise(sources::TIMER1);
        }
        if self.timer2.tick(cycles) != 0 {
            self.interrupt.raise(sources::TIMER2);
        }
        if self.timer3.tick(cycles) != 0 {
            self.interrupt.raise(sources::TIMER3);
        }

        // Tick LCD
        if self.lcd.tick(cycles) {
            self.interrupt.raise(sources::LCD);
        }

        // Tick keypad scan timing and raise interrupt if scan/status indicates it
        if self.keypad.tick(cycles, &self.key_state) {
            self.interrupt.raise(sources::KEYPAD);
        }

        // Check keypad using internal key_state (any-key interrupt in continuous mode)
        if self.keypad.check_interrupt(&self.key_state) {
            self.interrupt.raise(sources::KEYPAD);
        }

        // Tick OS Timer (32KHz crystal-based timer)
        self.tick_os_timer(cycles);

        self.interrupt.irq_pending()
    }

    /// Tick the OS Timer (32KHz crystal timer, generates bit 4 interrupt)
    /// Based on CEmu's ost_event in timers.c
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

        // Debug: OS Timer state tracking
        static mut OS_TIMER_DEBUG_COUNT: u64 = 0;
        #[allow(static_mut_refs)]
        unsafe {
            OS_TIMER_DEBUG_COUNT += 1;
            if OS_TIMER_DEBUG_COUNT % 5000000 == 1 {
                eprintln!("OS_TIMER_TICK: count={}, os_timer_cycles={}, state={}, cycles_per_32k_tick={}",
                         OS_TIMER_DEBUG_COUNT, self.os_timer_cycles, self.os_timer_state, cycles_per_32k_tick);
            }
        }

        // Check if enough cycles have passed to toggle state
        // cycles_needed must be recalculated each iteration since it depends on os_timer_state
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

            // CEmu order: toggle FIRST, then set interrupt to match NEW state
            // From timers.c ost_event():
            //   gpt.osTimerState = !gpt.osTimerState;
            //   intrpt_set(INT_OSTIMER, gpt.osTimerState);
            self.os_timer_state = !self.os_timer_state;

            // Set interrupt based on NEW state
            // IMPORTANT: Only RAISE the interrupt on the rising edge.
            // Do NOT clear_raw when state becomes false - the status should
            // persist until software acknowledges it via the interrupt controller.
            //
            // The issue with calling clear_raw: For non-latched interrupts,
            // clear_raw also clears the status bit. The OS Timer only stays
            // in the "true" state for ~1464 cycles at 48MHz, which is too short
            // for the CPU to reliably process the interrupt before it's cleared.
            //
            // By only raising and letting software acknowledge, the interrupt
            // remains pending until the ISR processes it.
            if self.os_timer_state {
                self.interrupt.raise(sources::OSTIMER);
            }
            // Note: We intentionally do NOT call clear_raw(OSTIMER) when state
            // becomes false. The raw state tracks the physical timer state,
            // but the interrupt status should remain until acknowledged.

            // IMPORTANT: Only process one state change per tick() call!
            // This matches CEmu's scheduler-based approach where each timer event
            // is processed separately, giving the CPU a chance to handle interrupts
            // before the next state change clears them.
            break;
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

        // Timer 1 (24 bytes)
        buf[pos..pos+4].copy_from_slice(&self.timer1.counter().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.timer1.reset_value().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.timer1.match1().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.timer1.match2().to_le_bytes()); pos += 4;
        buf[pos] = self.timer1.read_control(); pos += 1;
        buf[pos..pos+4].copy_from_slice(&self.timer1.accum_cycles().to_le_bytes()); pos += 4;
        pos += 3; // Padding to 24 bytes

        // Timer 2 (24 bytes)
        buf[pos..pos+4].copy_from_slice(&self.timer2.counter().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.timer2.reset_value().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.timer2.match1().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.timer2.match2().to_le_bytes()); pos += 4;
        buf[pos] = self.timer2.read_control(); pos += 1;
        buf[pos..pos+4].copy_from_slice(&self.timer2.accum_cycles().to_le_bytes()); pos += 4;
        pos += 3; // Padding to 24 bytes

        // Timer 3 (24 bytes)
        buf[pos..pos+4].copy_from_slice(&self.timer3.counter().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.timer3.reset_value().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.timer3.match1().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.timer3.match2().to_le_bytes()); pos += 4;
        buf[pos] = self.timer3.read_control(); pos += 1;
        buf[pos..pos+4].copy_from_slice(&self.timer3.accum_cycles().to_le_bytes()); pos += 4;
        pos += 3; // Padding to 24 bytes

        // LCD controller (24 bytes)
        buf[pos..pos+4].copy_from_slice(&self.lcd.control().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.lcd.upbase().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.lcd.int_mask().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.lcd.int_status().to_le_bytes()); pos += 4;
        buf[pos..pos+4].copy_from_slice(&self.lcd.frame_cycles().to_le_bytes()); pos += 4;
        pos += 4; // Padding

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

        // Timer 1
        self.timer1.set_counter(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timer1.set_reset_value(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timer1.set_match1(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timer1.set_match2(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timer1.write_control(buf[pos]); pos += 1;
        self.timer1.set_accum_cycles(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        pos += 3;

        // Timer 2
        self.timer2.set_counter(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timer2.set_reset_value(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timer2.set_match1(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timer2.set_match2(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timer2.write_control(buf[pos]); pos += 1;
        self.timer2.set_accum_cycles(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        pos += 3;

        // Timer 3
        self.timer3.set_counter(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timer3.set_reset_value(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timer3.set_match1(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timer3.set_match2(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.timer3.write_control(buf[pos]); pos += 1;
        self.timer3.set_accum_cycles(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        pos += 3;

        // LCD controller
        self.lcd.set_control(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.lcd.set_upbase(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.lcd.set_int_mask(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.lcd.set_int_status(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        self.lcd.set_frame_cycles(u32::from_le_bytes(buf[pos..pos+4].try_into().unwrap())); pos += 4;
        pos += 4;

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
        p.timer1.write_control(0x01); // Enable timer 1
        p.set_key(0, 0, true);

        assert!(p.irq_pending());
        assert!(p.timer1.is_enabled());
        assert!(p.key_state()[0][0]);

        // Reset
        p.reset();

        assert!(!p.irq_pending());
        assert!(!p.timer1.is_enabled());
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

        // Write global timer control (CEmu-style)
        p.write_test(TIMER_BASE + 0x30, 0x01); // Enable timer 1 (bit 0)

        // Verify control register was written
        assert_eq!(p.timers.control() & 0x01, 0x01);

        // Test reading global registers
        assert_eq!(p.read_test(TIMER_BASE + 0x30, &keys), 0x01); // Control
        assert_eq!(p.read_test(TIMER_BASE + 0x3C, &keys), 0x01); // Revision LSB (0x00010801)
        assert_eq!(p.read_test(TIMER_BASE + 0x3D, &keys), 0x08);
        assert_eq!(p.read_test(TIMER_BASE + 0x3E, &keys), 0x01);
        assert_eq!(p.read_test(TIMER_BASE + 0x3F, &keys), 0x00);
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

        // Enable timer 1 with overflow interrupt via global control register
        // Bit 0 = enable, bit 2 = overflow interrupt enable
        p.write_test(TIMER_BASE + 0x30, 0x05);

        // Set counter to max via write API
        p.write_test(TIMER_BASE, 0xFF);
        p.write_test(TIMER_BASE + 1, 0xFF);
        p.write_test(TIMER_BASE + 2, 0xFF);
        p.write_test(TIMER_BASE + 3, 0xFF);

        // Enable timer 1 interrupt in interrupt controller
        p.write_test(INT_BASE + 0x04, sources::TIMER1 as u8);

        // Tick should overflow timer and raise interrupt
        let pending = p.tick(2);
        assert!(pending);
        assert!(p.irq_pending());
    }

    #[test]
    fn test_tick_lcd_interrupt() {
        let mut p = Peripherals::new();

        // Enable LCD with VBLANK interrupt via write API
        // Control is at offset 0x18, INT_MASK is at offset 0x1C
        p.write_test(LCD_BASE + 0x18, 0x01); // ENABLE (control bit 0)
        p.write_test(LCD_BASE + 0x1C, 0x01); // Enable VBLANK interrupt mask

        // Enable LCD interrupt in interrupt controller (bit 11 - in byte 1)
        p.write_test(INT_BASE + 0x04, 0x00); // Low byte
        p.write_test(INT_BASE + 0x05, (sources::LCD >> 8) as u8); // High byte (bit 11)

        // Tick for a full frame (800_000 cycles at 48MHz/60Hz)
        let pending = p.tick(800_000);
        assert!(pending);
        assert!(p.irq_pending());
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
        let pending = p.tick(1);
        assert!(pending);
    }

    #[test]
    fn test_tick_multiple_timers() {
        let mut p = Peripherals::new();

        // Enable all 3 timers with overflow interrupt via global control
        // Timer 1: bit 0 (enable) + bit 2 (overflow int) = 0x05
        // Timer 2: bit 3 (enable) + bit 5 (overflow int) = 0x28
        // Timer 3: bit 6 (enable) + bit 8 (overflow int) = 0x140
        // Combined: 0x05 | 0x28 | 0x140 = 0x16D
        // But control is accessed byte-by-byte, so:
        // Byte 0: 0x6D (bits 0-7)
        // Byte 1: 0x01 (bits 8-15)
        p.write_test(TIMER_BASE + 0x30, 0x6D);
        p.write_test(TIMER_BASE + 0x31, 0x01);

        // Set counter to 0xFFFFFFFE via write API for timer 1
        p.write_test(TIMER_BASE, 0xFE);
        p.write_test(TIMER_BASE + 1, 0xFF);
        p.write_test(TIMER_BASE + 2, 0xFF);
        p.write_test(TIMER_BASE + 3, 0xFF);

        // Set counter to 0xFFFFFFFD via write API for timer 2
        p.write_test(TIMER_BASE + 0x10, 0xFD);
        p.write_test(TIMER_BASE + 0x11, 0xFF);
        p.write_test(TIMER_BASE + 0x12, 0xFF);
        p.write_test(TIMER_BASE + 0x13, 0xFF);

        // Set counter to 0xFFFFFFFC via write API for timer 3
        p.write_test(TIMER_BASE + 0x20, 0xFC);
        p.write_test(TIMER_BASE + 0x21, 0xFF);
        p.write_test(TIMER_BASE + 0x22, 0xFF);
        p.write_test(TIMER_BASE + 0x23, 0xFF);

        // Enable all timer interrupts
        let enabled = sources::TIMER1 | sources::TIMER2 | sources::TIMER3;
        p.write_test(INT_BASE + 0x04, enabled as u8);

        // Tick 2 cycles - should overflow timer 1
        let pending = p.tick(2);
        assert!(pending);

        // Tick 1 more - should overflow timer 2
        p.tick(1);

        // Tick 1 more - should overflow timer 3
        p.tick(1);

        // All 3 timers should have raised interrupts
        assert_ne!(p.interrupt.read(0x00) & sources::TIMER1 as u8, 0);
        assert_ne!(p.interrupt.read(0x00) & sources::TIMER2 as u8, 0);
        assert_ne!(p.interrupt.read(0x00) & sources::TIMER3 as u8, 0);
    }

    #[test]
    fn test_tick_no_interrupts_when_disabled() {
        let mut p = Peripherals::new();

        // Enable timer 1 with overflow interrupt via global control
        // But don't enable the interrupt in the interrupt controller
        p.write_test(TIMER_BASE + 0x30, 0x05); // Enable + overflow int

        // Set counter to max
        p.write_test(TIMER_BASE, 0xFF);
        p.write_test(TIMER_BASE + 1, 0xFF);
        p.write_test(TIMER_BASE + 2, 0xFF);
        p.write_test(TIMER_BASE + 3, 0xFF);

        // Don't enable the interrupt in the interrupt controller
        // (enabled = 0 by default)

        // Tick should overflow timer but not report pending (interrupt not enabled)
        let pending = p.tick(2);
        assert!(!pending);
        assert!(!p.irq_pending());

        // Status should still be latched in interrupt controller
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
        assert_eq!(p.read_test(FLASH_BASE + 0x01, &keys), 0x07); // size config
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

    #[test]
    fn test_timer_system_global_registers() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Test writing global control register (0xF20030)
        // Enable timer 1 (bit 0), overflow interrupt (bit 2)
        p.write_test(TIMER_BASE + 0x30, 0x05);
        assert_eq!(p.timers.control(), 0x05);
        assert_eq!(p.read_test(TIMER_BASE + 0x30, &keys), 0x05);

        // Test writing timer 1 counter to near overflow
        p.write_test(TIMER_BASE + 0x00, 0xFE);
        p.write_test(TIMER_BASE + 0x01, 0xFF);
        p.write_test(TIMER_BASE + 0x02, 0xFF);
        p.write_test(TIMER_BASE + 0x03, 0xFF);
        assert_eq!(p.timers.timer1().counter(), 0xFFFFFFFE);

        // Enable timer 1 interrupt
        p.write_test(INT_BASE + 0x04, sources::TIMER1 as u8);

        // Tick to overflow
        let pending = p.tick(3);
        assert!(pending);

        // Status register should have overflow bit set (bit 2)
        let status = p.timers.status();
        assert_ne!(status & 0x04, 0, "Overflow status bit should be set");

        // Write 1 to clear status (write-1-to-clear behavior)
        p.write_test(TIMER_BASE + 0x34, 0x04);
        assert_eq!(p.timers.status() & 0x04, 0, "Status should be cleared");
    }

    #[test]
    fn test_timer_system_revision() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Revision register (0xF2003C) should be 0x00010801
        assert_eq!(p.read_test(TIMER_BASE + 0x3C, &keys), 0x01);
        assert_eq!(p.read_test(TIMER_BASE + 0x3D, &keys), 0x08);
        assert_eq!(p.read_test(TIMER_BASE + 0x3E, &keys), 0x01);
        assert_eq!(p.read_test(TIMER_BASE + 0x3F, &keys), 0x00);

        // Revision should be read-only
        p.write_test(TIMER_BASE + 0x3C, 0xFF);
        assert_eq!(p.read_test(TIMER_BASE + 0x3C, &keys), 0x01);
    }

    #[test]
    fn test_timer_system_multiple_timers() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Enable all 3 timers via global control
        // Timer 1: bit 0, Timer 2: bit 3, Timer 3: bit 6
        p.write_test(TIMER_BASE + 0x30, 0x49); // 0b01001001

        // Set different counters
        p.write_test(TIMER_BASE + 0x00, 10); // Timer 1
        p.write_test(TIMER_BASE + 0x10, 20); // Timer 2
        p.write_test(TIMER_BASE + 0x20, 30); // Timer 3

        // Tick 5 cycles
        p.tick(5);

        // All timers should have incremented
        assert_eq!(p.read_test(TIMER_BASE + 0x00, &keys), 15);
        assert_eq!(p.read_test(TIMER_BASE + 0x10, &keys), 25);
        assert_eq!(p.read_test(TIMER_BASE + 0x20, &keys), 35);
    }
}
