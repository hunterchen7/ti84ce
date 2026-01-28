//! TI-84 Plus CE General Purpose Timers
//!
//! Memory-mapped at 0xF20000 (port offset 0x120000 from 0xE00000)
//! Three timers are available, each with 0x10 bytes of registers.
//!
//! Timer 1: 0xF20000-0xF2000F
//! Timer 2: 0xF20010-0xF2001F
//! Timer 3: 0xF20020-0xF2002F

/// Register offsets within each timer (relative to timer base)
mod regs {
    /// Counter value (32-bit, read/write)
    pub const COUNTER: u32 = 0x00;
    /// Reset/reload value (32-bit)
    pub const RESET: u32 = 0x04;
    /// Match value 1 (32-bit)
    pub const MATCH1: u32 = 0x08;
    /// Match value 2 (32-bit)
    pub const MATCH2: u32 = 0x0C;
}

/// Control register bits (at offset 0x30 for all timers)
mod ctrl {
    /// Timer enable
    pub const ENABLE: u8 = 1 << 0;
    /// Count direction: 1=up, 0=down
    pub const COUNT_UP: u8 = 1 << 1;
    /// Interrupt on zero/overflow
    pub const INT_ON_ZERO: u8 = 1 << 2;
    /// Use reset value on overflow
    pub const USE_RESET: u8 = 1 << 4;
    /// Clock divider bits 5-7 (0=1, 1=2, 2=4, 3=8, ...)
    pub const CLOCK_DIV_SHIFT: u8 = 5;
    pub const CLOCK_DIV_MASK: u8 = 0x07;
}

/// A single general-purpose timer
#[derive(Debug, Clone)]
pub struct Timer {
    /// Current counter value
    counter: u32,
    /// Reset/reload value
    reset_value: u32,
    /// Match value 1
    match1: u32,
    /// Match value 2
    match2: u32,
    /// Control register
    control: u8,
    /// Accumulated cycles (for clock division)
    accum_cycles: u32,
}

impl Timer {
    /// Create a new timer
    pub fn new() -> Self {
        Self {
            counter: 0,
            reset_value: 0,
            match1: 0,
            match2: 0,
            control: 0,
            accum_cycles: 0,
        }
    }

    /// Reset the timer to initial state
    pub fn reset(&mut self) {
        self.counter = 0;
        self.reset_value = 0;
        self.match1 = 0;
        self.match2 = 0;
        self.control = 0;
        self.accum_cycles = 0;
    }

    /// Check if timer is enabled
    pub fn is_enabled(&self) -> bool {
        self.control & ctrl::ENABLE != 0
    }

    /// Check if counting up
    fn count_up(&self) -> bool {
        self.control & ctrl::COUNT_UP != 0
    }

    /// Check if interrupt on zero/overflow is enabled
    fn int_on_zero(&self) -> bool {
        self.control & ctrl::INT_ON_ZERO != 0
    }

    /// Check if reset value should be used on overflow
    fn use_reset(&self) -> bool {
        self.control & ctrl::USE_RESET != 0
    }

    /// Get clock divider (1, 2, 4, 8, 16, 32, 64, 128)
    fn clock_divider(&self) -> u32 {
        let div_bits = (self.control >> ctrl::CLOCK_DIV_SHIFT) & ctrl::CLOCK_DIV_MASK;
        1 << div_bits
    }

    /// Tick the timer with given CPU cycles
    /// Returns true if an interrupt should be generated
    pub fn tick(&mut self, cycles: u32) -> bool {
        if !self.is_enabled() {
            return false;
        }

        // Accumulate cycles and apply divider
        self.accum_cycles += cycles;
        let divider = self.clock_divider();
        let ticks = self.accum_cycles / divider;
        self.accum_cycles %= divider;

        if ticks == 0 {
            return false;
        }

        let mut interrupt = false;

        if self.count_up() {
            // Count up
            let (new_val, overflow) = self.counter.overflowing_add(ticks);
            self.counter = new_val;

            if overflow {
                if self.use_reset() {
                    self.counter = self.reset_value;
                }
                if self.int_on_zero() {
                    interrupt = true;
                }
            }
        } else {
            // Count down
            if self.counter >= ticks {
                self.counter -= ticks;
            } else {
                // Underflow
                if self.use_reset() {
                    self.counter = self.reset_value.saturating_sub(ticks - self.counter - 1);
                } else {
                    self.counter = 0xFFFFFFFF_u32.saturating_sub(ticks - self.counter - 1);
                }
                if self.int_on_zero() {
                    interrupt = true;
                }
            }
        }

        interrupt
    }

    /// Read a register byte
    /// addr is offset within this timer (0-0x0F)
    pub fn read(&self, addr: u32) -> u8 {
        let reg = addr & 0x0C;
        let byte_offset = (addr & 0x03) * 8;

        let value = match reg {
            regs::COUNTER => self.counter,
            regs::RESET => self.reset_value,
            regs::MATCH1 => self.match1,
            regs::MATCH2 => self.match2,
            _ => 0,
        };

        ((value >> byte_offset) & 0xFF) as u8
    }

    /// Write a register byte
    /// addr is offset within this timer (0-0x0F)
    pub fn write(&mut self, addr: u32, value: u8) {
        let reg = addr & 0x0C;
        let byte_offset = (addr & 0x03) * 8;
        let mask = 0xFF_u32 << byte_offset;
        let shifted_value = (value as u32) << byte_offset;

        match reg {
            regs::COUNTER => {
                self.counter = (self.counter & !mask) | (shifted_value & mask);
            }
            regs::RESET => {
                self.reset_value = (self.reset_value & !mask) | (shifted_value & mask);
            }
            regs::MATCH1 => {
                self.match1 = (self.match1 & !mask) | (shifted_value & mask);
            }
            regs::MATCH2 => {
                self.match2 = (self.match2 & !mask) | (shifted_value & mask);
            }
            _ => {}
        }
    }

    /// Read control register
    pub fn read_control(&self) -> u8 {
        self.control
    }

    /// Write control register
    pub fn write_control(&mut self, value: u8) {
        self.control = value;
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let timer = Timer::new();
        assert!(!timer.is_enabled());
        assert_eq!(timer.counter, 0);
    }

    #[test]
    fn test_count_up() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::COUNT_UP;
        timer.counter = 0;

        let irq = timer.tick(100);
        assert!(!irq);
        assert_eq!(timer.counter, 100);
    }

    #[test]
    fn test_count_down() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE;
        timer.counter = 100;

        let irq = timer.tick(50);
        assert!(!irq);
        assert_eq!(timer.counter, 50);
    }

    #[test]
    fn test_overflow_interrupt() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::COUNT_UP | ctrl::INT_ON_ZERO;
        timer.counter = 0xFFFFFFFF;

        let irq = timer.tick(2);
        assert!(irq);
    }

    #[test]
    fn test_clock_divider() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::COUNT_UP | (2 << ctrl::CLOCK_DIV_SHIFT);
        timer.counter = 0;

        // Divider is 4, so 8 cycles = 2 ticks
        let irq = timer.tick(8);
        assert!(!irq);
        assert_eq!(timer.counter, 2);
    }

    #[test]
    fn test_read_write_counter() {
        let mut timer = Timer::new();

        timer.write(0, 0x12);
        timer.write(1, 0x34);
        timer.write(2, 0x56);
        timer.write(3, 0x78);

        assert_eq!(timer.counter, 0x78563412);
        assert_eq!(timer.read(0), 0x12);
        assert_eq!(timer.read(1), 0x34);
        assert_eq!(timer.read(2), 0x56);
        assert_eq!(timer.read(3), 0x78);
    }

    #[test]
    fn test_reset_on_overflow() {
        let mut timer = Timer::new();
        timer.control = ctrl::ENABLE | ctrl::COUNT_UP | ctrl::USE_RESET;
        timer.counter = 0xFFFFFFFE;
        timer.reset_value = 0x1000;

        timer.tick(3);
        // Should have overflowed and reset to 0x1000
        assert_eq!(timer.counter, 0x1000);
    }
}
