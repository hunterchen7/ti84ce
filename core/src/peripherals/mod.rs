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
pub use timer::Timer;
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
    /// Timer 1
    pub timer1: Timer,
    /// Timer 2
    pub timer2: Timer,
    /// Timer 3
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
            a if a >= TIMER_BASE && a < TIMER_END => {
                let offset = a - TIMER_BASE;
                if offset >= 0x30 {
                    // Timer control registers
                    match offset {
                        0x30 => self.timer1.read_control(),
                        0x34 => self.timer2.read_control(),
                        0x38 => self.timer3.read_control(),
                        _ => 0x00,
                    }
                } else {
                    // Timer data registers (0x10 bytes per timer)
                    let timer_idx = offset / 0x10;
                    let reg_offset = offset % 0x10;
                    match timer_idx {
                        0 => self.timer1.read(reg_offset),
                        1 => self.timer2.read(reg_offset),
                        2 => self.timer3.read(reg_offset),
                        _ => 0x00,
                    }
                }
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
            a if a >= TIMER_BASE && a < TIMER_END => {
                let offset = a - TIMER_BASE;
                if offset >= 0x30 {
                    // Timer control registers
                    match offset {
                        0x30 => self.timer1.write_control(value),
                        0x34 => self.timer2.write_control(value),
                        0x38 => self.timer3.write_control(value),
                        _ => {}
                    }
                } else {
                    // Timer data registers (0x10 bytes per timer)
                    let timer_idx = offset / 0x10;
                    let reg_offset = offset % 0x10;
                    match timer_idx {
                        0 => self.timer1.write(reg_offset, value),
                        1 => self.timer2.write(reg_offset, value),
                        2 => self.timer3.write(reg_offset, value),
                        _ => {}
                    }
                }
            }

            // Keypad Controller (0xF50000 - 0xF5003F)
            a if a >= KEYPAD_BASE && a < KEYPAD_END => {
                let offset = a - KEYPAD_BASE;

                // DIAGNOSTIC: Unconditional log to see if writes go through here
                static mut WRITE_COUNT: u32 = 0;
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
        // Tick timers
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

        // Write timer control
        p.write_test(TIMER_BASE + 0x30, 0x01); // Enable timer 1
        assert!(p.timer1.is_enabled());
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

        // Enable timer 1 with interrupt on overflow
        p.timer1.write_control(0x01 | 0x02 | 0x04); // ENABLE | COUNT_UP | INT_ON_ZERO
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

        // Enable all 3 timers counting up with interrupt
        let ctrl = 0x01 | 0x02 | 0x04; // ENABLE | COUNT_UP | INT_ON_ZERO

        p.timer1.write_control(ctrl);
        // Set counter to 0xFFFFFFFE via write API
        p.write_test(TIMER_BASE, 0xFE);
        p.write_test(TIMER_BASE + 1, 0xFF);
        p.write_test(TIMER_BASE + 2, 0xFF);
        p.write_test(TIMER_BASE + 3, 0xFF);

        p.timer2.write_control(ctrl);
        // Set counter to 0xFFFFFFFD via write API
        p.write_test(TIMER_BASE + 0x10, 0xFD);
        p.write_test(TIMER_BASE + 0x11, 0xFF);
        p.write_test(TIMER_BASE + 0x12, 0xFF);
        p.write_test(TIMER_BASE + 0x13, 0xFF);

        p.timer3.write_control(ctrl);
        // Set counter to 0xFFFFFFFC via write API
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

        // Enable timer 1 with interrupt on overflow
        p.timer1.write_control(0x01 | 0x02 | 0x04);
        // Set counter to max
        p.write_test(TIMER_BASE, 0xFF);
        p.write_test(TIMER_BASE + 1, 0xFF);
        p.write_test(TIMER_BASE + 2, 0xFF);
        p.write_test(TIMER_BASE + 3, 0xFF);

        // But don't enable the interrupt in the interrupt controller
        // (enabled = 0 by default)

        // Tick should overflow timer but not report pending
        let pending = p.tick(2);
        assert!(!pending);
        assert!(!p.irq_pending());

        // Status should still be latched
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
}
