//! TI-84 Plus CE Peripheral Emulation
//!
//! This module contains the memory-mapped peripheral controllers:
//! - Control Ports (0xE00000, 0xFF0000)
//! - Interrupt Controller (0xF00000)
//! - Timers (0xF20000)
//! - LCD Controller (0xE30000)
//! - Keypad Controller (0xF50000)

pub mod control;
pub mod interrupt;
pub mod keypad;
pub mod lcd;
pub mod timer;

pub use control::ControlPorts;
pub use interrupt::InterruptController;
pub use keypad::{KeypadController, KEYPAD_COLS, KEYPAD_ROWS};
pub use lcd::{LcdController, LCD_HEIGHT, LCD_WIDTH};
pub use timer::Timer;

use interrupt::sources;

/// Port address regions (offsets from 0xE00000)
const CONTROL_BASE: u32 = 0x000000; // 0xE00000
const CONTROL_END: u32 = 0x000100;
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

/// Peripheral subsystem containing all hardware controllers
#[derive(Debug, Clone)]
pub struct Peripherals {
    /// Control ports (0xE00000, 0xFF0000)
    pub control: ControlPorts,
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
    /// Fallback register storage for unmapped ports
    fallback: Vec<u8>,
    /// Keypad state (updated by Emu)
    key_state: [[bool; KEYPAD_COLS]; KEYPAD_ROWS],
}

impl Peripherals {
    /// Size of fallback register storage
    const FALLBACK_SIZE: usize = 0x200000;

    /// Create new peripheral subsystem
    pub fn new() -> Self {
        Self {
            control: ControlPorts::new(),
            interrupt: InterruptController::new(),
            timer1: Timer::new(),
            timer2: Timer::new(),
            timer3: Timer::new(),
            lcd: LcdController::new(),
            keypad: KeypadController::new(),
            fallback: vec![0x00; Self::FALLBACK_SIZE],
            key_state: [[false; KEYPAD_COLS]; KEYPAD_ROWS],
        }
    }

    /// Update keypad state from emulator
    pub fn set_key(&mut self, row: usize, col: usize, pressed: bool) {
        if row < KEYPAD_ROWS && col < KEYPAD_COLS {
            self.key_state[row][col] = pressed;
        }
    }

    /// Get current key state
    pub fn key_state(&self) -> &[[bool; KEYPAD_COLS]; KEYPAD_ROWS] {
        &self.key_state
    }

    /// Reset all peripherals
    pub fn reset(&mut self) {
        self.control.reset();
        self.interrupt.reset();
        self.timer1.reset();
        self.timer2.reset();
        self.timer3.reset();
        self.lcd.reset();
        self.keypad.reset();
        self.fallback.fill(0x00);
        self.key_state = [[false; KEYPAD_COLS]; KEYPAD_ROWS];
    }

    /// Read from a port address
    /// addr is offset from 0xE00000
    /// key_state is the current keyboard matrix
    pub fn read(&self, addr: u32, key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> u8 {
        match addr {
            // Control Ports (0xE00000 - 0xE000FF)
            a if a >= CONTROL_BASE && a < CONTROL_END => self.control.read(a - CONTROL_BASE),

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

            // Unmapped - return from fallback storage
            _ => {
                let offset = (addr as usize) % Self::FALLBACK_SIZE;
                self.fallback[offset]
            }
        }
    }

    /// Write to a port address
    /// addr is offset from 0xE00000
    pub fn write(&mut self, addr: u32, value: u8) {
        match addr {
            // Control Ports (0xE00000 - 0xE000FF)
            a if a >= CONTROL_BASE && a < CONTROL_END => self.control.write(a - CONTROL_BASE, value),

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
            a if a >= KEYPAD_BASE && a < KEYPAD_END => self.keypad.write(a - KEYPAD_BASE, value),

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
        if self.timer1.tick(cycles) {
            self.interrupt.raise(sources::TIMER1);
        }
        if self.timer2.tick(cycles) {
            self.interrupt.raise(sources::TIMER2);
        }
        if self.timer3.tick(cycles) {
            self.interrupt.raise(sources::TIMER3);
        }

        // Tick LCD
        if self.lcd.tick(cycles) {
            self.interrupt.raise(sources::LCD);
        }

        // Check keypad using internal key_state
        if self.keypad.check_interrupt(&self.key_state) {
            self.interrupt.raise(sources::KEYPAD);
        }

        self.interrupt.irq_pending()
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

    #[test]
    fn test_new() {
        let p = Peripherals::new();
        assert!(!p.irq_pending());
    }

    #[test]
    fn test_lcd_routing() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Write to LCD upbase register
        p.write(LCD_BASE + 0x1C, 0x00);
        p.write(LCD_BASE + 0x1D, 0x00);
        p.write(LCD_BASE + 0x1E, 0xD5);

        assert_eq!(p.lcd.upbase(), 0xD50000);

        // Read back
        assert_eq!(p.read(LCD_BASE + 0x1C, &keys), 0x00);
        assert_eq!(p.read(LCD_BASE + 0x1E, &keys), 0xD5);
    }

    #[test]
    fn test_timer_routing() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Write to timer 1 counter
        p.write(TIMER_BASE, 0x12);
        p.write(TIMER_BASE + 1, 0x34);

        // Read back
        assert_eq!(p.read(TIMER_BASE, &keys), 0x12);
        assert_eq!(p.read(TIMER_BASE + 1, &keys), 0x34);

        // Write timer control
        p.write(TIMER_BASE + 0x30, 0x01); // Enable timer 1
        assert!(p.timer1.is_enabled());
    }

    #[test]
    fn test_keypad_routing() {
        let p = Peripherals::new();
        let mut keys = empty_keys();

        // All keys released
        assert_eq!(p.read(KEYPAD_BASE + 0x10, &keys), 0xFF);

        // Press a key
        keys[0][3] = true;
        assert_eq!(p.read(KEYPAD_BASE + 0x10, &keys), 0xFF ^ (1 << 3));
    }

    #[test]
    fn test_interrupt_routing() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Enable timer 1 interrupt
        p.write(INT_BASE + 0x04, sources::TIMER1 as u8);

        // Raise timer 1 interrupt
        p.interrupt.raise(sources::TIMER1);

        assert!(p.irq_pending());

        // Read interrupt status
        assert_eq!(p.read(INT_BASE, &keys), sources::TIMER1 as u8);
    }

    #[test]
    fn test_fallback_storage() {
        let mut p = Peripherals::new();
        let keys = empty_keys();

        // Write to unmapped address
        p.write(0x000100, 0xAB);
        assert_eq!(p.read(0x000100, &keys), 0xAB);
    }
}
