//! TI-84 Plus CE Watchdog Timer Stub
//!
//! Memory-mapped at 0xF60000 (port offset 0x160000 from 0xE00000)
//! Also accessible via I/O port range 0x6xxx
//!
//! This is a minimal stub that ignores writes and returns safe values.
//! The watchdog timer is not critical for boot progression.

/// Watchdog Controller Stub
#[derive(Debug, Clone)]
pub struct WatchdogController {
    /// Control register
    control: u8,
    /// Load value (32-bit)
    load: u32,
    /// Interrupt status
    interrupt: u8,
    /// Lock register (when locked, control register cannot be modified)
    lock: u8,
}

impl WatchdogController {
    /// Revision value returned at offset 0xFC-0xFF
    const REVISION: u32 = 0x00000500;

    /// Create a new Watchdog controller
    pub fn new() -> Self {
        Self {
            control: 0x00,
            load: 0xFFFFFFFF,
            interrupt: 0,
            lock: 0,
        }
    }

    /// Reset the Watchdog controller
    pub fn reset(&mut self) {
        self.control = 0x00;
        self.load = 0xFFFFFFFF;
        self.interrupt = 0;
        self.lock = 0;
    }

    /// Read a register byte
    /// addr is offset from controller base (0-0xFF)
    pub fn read(&self, addr: u32) -> u8 {
        let index = addr & 0xFF;
        let bit_offset = ((index & 3) << 3) as u32;

        match index {
            // Load register (32-bit)
            0x00..=0x03 => ((self.load >> bit_offset) & 0xFF) as u8,

            // Current value (return load value for stub - not counting down)
            0x04..=0x07 => ((self.load >> bit_offset) & 0xFF) as u8,

            // Control register
            0x08 => self.control,

            // Interrupt clear (write-only, read returns 0)
            0x0C => 0,

            // Raw interrupt status
            0x10 => self.interrupt,

            // Masked interrupt status
            0x14 => self.interrupt & (self.control & 0x01),

            // Lock register (at offset 0xC0)
            0xC0 => self.lock,

            // Revision (0x00000500)
            0xFC..=0xFF => ((Self::REVISION >> bit_offset) & 0xFF) as u8,

            _ => 0,
        }
    }

    /// Write a register byte
    /// addr is offset from controller base (0-0xFF)
    pub fn write(&mut self, addr: u32, value: u8) {
        let index = addr & 0xFF;
        let bit_offset = ((index & 3) << 3) as u32;

        match index {
            // Load register (32-bit)
            0x00..=0x03 => {
                let mask = !(0xFF_u32 << bit_offset);
                self.load = (self.load & mask) | ((value as u32) << bit_offset);
            }

            // Control register (ignored if locked)
            0x08 => {
                if self.lock == 0 {
                    self.control = value;
                }
            }

            // Interrupt clear (write any value to clear)
            0x0C => {
                self.interrupt = 0;
            }

            // Lock register
            // Writing 0x1ACCE551 unlocks, any other value locks
            // For byte access, we just track the last byte written
            0xC0 => {
                // Simplified: any write to lock register updates lock state
                // In reality this is a 32-bit access check
                self.lock = value;
            }

            _ => {}
        }
    }

    /// Tick the watchdog (called periodically)
    /// For the stub, we don't actually count down or trigger reset
    pub fn tick(&mut self, _cycles: u32) -> bool {
        // Return false - no interrupt/reset pending from stub
        false
    }
}

impl Default for WatchdogController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let wdt = WatchdogController::new();
        assert_eq!(wdt.control, 0x00);
        assert_eq!(wdt.load, 0xFFFFFFFF);
    }

    #[test]
    fn test_reset() {
        let mut wdt = WatchdogController::new();
        wdt.control = 0x03;
        wdt.load = 0x12345678;
        wdt.reset();
        assert_eq!(wdt.control, 0x00);
        assert_eq!(wdt.load, 0xFFFFFFFF);
    }

    #[test]
    fn test_read_load() {
        let mut wdt = WatchdogController::new();
        wdt.load = 0x12345678;
        assert_eq!(wdt.read(0x00), 0x78);
        assert_eq!(wdt.read(0x01), 0x56);
        assert_eq!(wdt.read(0x02), 0x34);
        assert_eq!(wdt.read(0x03), 0x12);
    }

    #[test]
    fn test_read_current_value() {
        let mut wdt = WatchdogController::new();
        wdt.load = 0xAABBCCDD;
        // Current value should equal load (not counting down in stub)
        assert_eq!(wdt.read(0x04), 0xDD);
        assert_eq!(wdt.read(0x05), 0xCC);
        assert_eq!(wdt.read(0x06), 0xBB);
        assert_eq!(wdt.read(0x07), 0xAA);
    }

    #[test]
    fn test_write_load() {
        let mut wdt = WatchdogController::new();
        wdt.write(0x00, 0x12);
        wdt.write(0x01, 0x34);
        wdt.write(0x02, 0x56);
        wdt.write(0x03, 0x78);
        assert_eq!(wdt.load, 0x78563412);
    }

    #[test]
    fn test_control() {
        let mut wdt = WatchdogController::new();
        wdt.write(0x08, 0x03); // Enable + interrupt enable
        assert_eq!(wdt.read(0x08), 0x03);
    }

    #[test]
    fn test_interrupt_clear() {
        let mut wdt = WatchdogController::new();
        wdt.interrupt = 0x01;
        assert_eq!(wdt.read(0x10), 0x01); // Raw status
        wdt.write(0x0C, 0x01); // Clear
        assert_eq!(wdt.read(0x10), 0x00); // Cleared
    }

    #[test]
    fn test_read_revision() {
        let wdt = WatchdogController::new();
        assert_eq!(wdt.read(0xFC), 0x00);
        assert_eq!(wdt.read(0xFD), 0x05);
        assert_eq!(wdt.read(0xFE), 0x00);
        assert_eq!(wdt.read(0xFF), 0x00);
    }

    #[test]
    fn test_tick_no_interrupt() {
        let mut wdt = WatchdogController::new();
        assert!(!wdt.tick(1000));
    }
}
