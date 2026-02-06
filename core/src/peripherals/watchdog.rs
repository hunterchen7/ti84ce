//! TI-84 Plus CE Watchdog Timer
//!
//! Memory-mapped at 0xF60000 (port offset 0x160000 from 0xE00000)
//! Also accessible via I/O port range 0x6xxx
//!
//! Register layout (from CEmu misc.c):
//!   0x00-0x03: Current counter (32-bit, read-only)
//!   0x04-0x07: Load value (32-bit, read/write)
//!   0x08:      Restart (write 0xB9 to reload)
//!   0x0C:      Control register
//!   0x10-0x13: Status (read, write-to-clear)
//!   0x18:      Pulse load (8-bit)
//!   0x1C-0x1F: Revision (0x00010602, read-only)

/// Watchdog Controller
#[derive(Debug, Clone)]
pub struct WatchdogController {
    /// Current countdown counter
    count: u32,
    /// Load/reload value
    load: u32,
    /// Control register
    control: u8,
    /// Status register (bit 0 = expired)
    status: u8,
    /// Pulse load value
    pulse_load: u8,
}

impl WatchdogController {
    /// Revision value returned at offset 0x1C-0x1F
    const REVISION: u32 = 0x00010602;

    /// Default load value on reset (from CEmu)
    const DEFAULT_LOAD: u32 = 0x03EF1480;

    /// Create a new Watchdog controller
    pub fn new() -> Self {
        Self {
            count: Self::DEFAULT_LOAD,
            load: Self::DEFAULT_LOAD,
            control: 0x00,
            status: 0x00,
            pulse_load: 0xFF,
        }
    }

    /// Reset the Watchdog controller
    pub fn reset(&mut self) {
        self.count = Self::DEFAULT_LOAD;
        self.load = Self::DEFAULT_LOAD;
        self.control = 0x00;
        self.status = 0x00;
        self.pulse_load = 0xFF;
    }

    /// Read a register byte
    /// addr is offset from controller base (0-0xFF)
    pub fn read(&self, addr: u32) -> u8 {
        let index = addr & 0xFF;
        let bit_offset = ((index & 3) << 3) as u32;

        match index {
            // Current counter (32-bit, read-only)
            0x00..=0x03 => ((self.count >> bit_offset) & 0xFF) as u8,

            // Load value (32-bit)
            0x04..=0x07 => ((self.load >> bit_offset) & 0xFF) as u8,

            // Restart register (write-only, read returns 0)
            0x08 => 0,

            // Control register
            0x0C => self.control,

            // Status register
            0x10 => self.status,

            // Pulse load
            0x18 => self.pulse_load,

            // Revision (0x00010602)
            0x1C..=0x1F => ((Self::REVISION >> bit_offset) & 0xFF) as u8,

            _ => 0,
        }
    }

    /// Write a register byte
    /// addr is offset from controller base (0-0xFF)
    pub fn write(&mut self, addr: u32, value: u8) {
        let index = addr & 0xFF;
        let bit_offset = ((index & 3) << 3) as u32;

        match index {
            // Load value (32-bit, writable)
            0x04..=0x07 => {
                let mask = !(0xFF_u32 << bit_offset);
                self.load = (self.load & mask) | ((value as u32) << bit_offset);
            }

            // Restart (write 0xB9 to reload counter from load value)
            0x08 => {
                if value == 0xB9 {
                    self.count = self.load;
                }
            }

            // Control register
            0x0C => {
                self.control = value;
            }

            // Status clear (write-to-clear)
            0x10..=0x13 => {
                self.status = 0;
            }

            // Pulse load
            0x18..=0x1B => {
                self.pulse_load = value;
            }

            _ => {}
        }
    }

    /// Tick the watchdog (called periodically)
    /// TODO: Implement proper countdown and state machine (Phase 4+)
    pub fn tick(&mut self, _cycles: u32) -> bool {
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
        assert_eq!(wdt.load, 0x03EF1480);
        assert_eq!(wdt.count, 0x03EF1480);
        assert_eq!(wdt.pulse_load, 0xFF);
    }

    #[test]
    fn test_reset() {
        let mut wdt = WatchdogController::new();
        wdt.control = 0x03;
        wdt.load = 0x12345678;
        wdt.count = 0x00000001;
        wdt.status = 0x01;
        wdt.pulse_load = 0x42;
        wdt.reset();
        assert_eq!(wdt.control, 0x00);
        assert_eq!(wdt.load, 0x03EF1480);
        assert_eq!(wdt.count, 0x03EF1480);
        assert_eq!(wdt.status, 0x00);
        assert_eq!(wdt.pulse_load, 0xFF);
    }

    #[test]
    fn test_read_count() {
        let mut wdt = WatchdogController::new();
        wdt.count = 0x12345678;
        assert_eq!(wdt.read(0x00), 0x78);
        assert_eq!(wdt.read(0x01), 0x56);
        assert_eq!(wdt.read(0x02), 0x34);
        assert_eq!(wdt.read(0x03), 0x12);
    }

    #[test]
    fn test_read_write_load() {
        let mut wdt = WatchdogController::new();
        wdt.write(0x04, 0x12);
        wdt.write(0x05, 0x34);
        wdt.write(0x06, 0x56);
        wdt.write(0x07, 0x78);
        assert_eq!(wdt.load, 0x78563412);
        assert_eq!(wdt.read(0x04), 0x12);
        assert_eq!(wdt.read(0x05), 0x34);
        assert_eq!(wdt.read(0x06), 0x56);
        assert_eq!(wdt.read(0x07), 0x78);
    }

    #[test]
    fn test_restart_magic() {
        let mut wdt = WatchdogController::new();
        wdt.load = 0x00001000;
        wdt.count = 0x00000001; // Almost expired

        // Non-magic value should NOT reload
        wdt.write(0x08, 0x42);
        assert_eq!(wdt.count, 0x00000001);

        // Magic value 0xB9 should reload from load
        wdt.write(0x08, 0xB9);
        assert_eq!(wdt.count, 0x00001000);
    }

    #[test]
    fn test_control() {
        let mut wdt = WatchdogController::new();
        wdt.write(0x0C, 0x07);
        assert_eq!(wdt.read(0x0C), 0x07);
    }

    #[test]
    fn test_status_write_to_clear() {
        let mut wdt = WatchdogController::new();
        wdt.status = 0x01; // Expired flag
        assert_eq!(wdt.read(0x10), 0x01);
        wdt.write(0x10, 0xFF); // Write-to-clear
        assert_eq!(wdt.read(0x10), 0x00);
    }

    #[test]
    fn test_pulse_load() {
        let mut wdt = WatchdogController::new();
        assert_eq!(wdt.read(0x18), 0xFF); // Default
        wdt.write(0x18, 0x42);
        assert_eq!(wdt.read(0x18), 0x42);
    }

    #[test]
    fn test_read_revision() {
        let wdt = WatchdogController::new();
        assert_eq!(wdt.read(0x1C), 0x02);
        assert_eq!(wdt.read(0x1D), 0x06);
        assert_eq!(wdt.read(0x1E), 0x01);
        assert_eq!(wdt.read(0x1F), 0x00);
    }

    #[test]
    fn test_tick_no_interrupt() {
        let mut wdt = WatchdogController::new();
        assert!(!wdt.tick(1000));
    }
}
