//! TI-84 Plus CE Real-Time Clock Stub
//!
//! Memory-mapped at 0xF80000 (port offset 0x180000 from 0xE00000)
//! Also accessible via I/O port range 0x8xxx
//!
//! This is a minimal stub that returns safe values to allow boot to progress.

/// RTC Controller Stub
#[derive(Debug, Clone)]
pub struct RtcController {
    /// Control register (bit 0 = enable, bit 7 = latch enable)
    control: u8,
    /// Interrupt status
    interrupt: u8,
    /// Latched seconds (0-59)
    latched_sec: u8,
    /// Latched minutes (0-59)
    latched_min: u8,
    /// Latched hours (0-23)
    latched_hour: u8,
    /// Latched day count
    latched_day: u16,
}

impl RtcController {
    /// Revision value returned at offset 0x3C-0x3F
    const REVISION: u32 = 0x00010500;

    /// Create a new RTC controller
    pub fn new() -> Self {
        Self {
            control: 0x81, // Enable + latch enable by default
            interrupt: 0,
            latched_sec: 30,
            latched_min: 15,
            latched_hour: 12,
            latched_day: 1,
        }
    }

    /// Reset the RTC controller
    pub fn reset(&mut self) {
        self.control = 0x81; // Enable + latch enable
        self.interrupt = 0;
        // Keep some reasonable time values
        self.latched_sec = 30;
        self.latched_min = 15;
        self.latched_hour = 12;
        self.latched_day = 1;
    }

    /// Read a register byte
    /// addr is offset from controller base (0-0xFF)
    pub fn read(&self, addr: u32) -> u8 {
        let index = addr & 0xFF;
        let bit_offset = ((index & 3) << 3) as u32;

        match index {
            // Latched time values
            0x00 => self.latched_sec,
            0x04 => self.latched_min,
            0x08 => self.latched_hour,
            0x0C => (self.latched_day & 0xFF) as u8,
            0x0D => ((self.latched_day >> 8) & 0xFF) as u8,

            // Alarm registers (return 0)
            0x10 => 0, // alarm sec
            0x14 => 0, // alarm min
            0x18 => 0, // alarm hour

            // Control register
            0x20 => self.control,

            // Load registers (return 0)
            0x24 => 0, // load sec
            0x28 => 0, // load min
            0x2C => 0, // load hour
            0x30 | 0x31 => 0, // load day

            // Interrupt status
            0x34 => self.interrupt,

            // Revision (0x00010500)
            0x3C..=0x3F => ((Self::REVISION >> bit_offset) & 0xFF) as u8,

            // Load status (0 = complete)
            0x40 => 0,

            // Combined latched value
            0x44..=0x47 => {
                let combined = (self.latched_sec as u32)
                    | ((self.latched_min as u32) << 6)
                    | ((self.latched_hour as u32) << 12)
                    | ((self.latched_day as u32) << 17);
                ((combined >> bit_offset) & 0xFF) as u8
            }

            _ => 0,
        }
    }

    /// Write a register byte
    /// addr is offset from controller base (0-0xFF)
    pub fn write(&mut self, addr: u32, value: u8) {
        let index = addr & 0xFF;

        match index {
            // Alarm registers
            0x10 => {} // alarm sec (ignored for stub)
            0x14 => {} // alarm min (ignored for stub)
            0x18 => {} // alarm hour (ignored for stub)

            // Control register
            0x20 => {
                // Bit 6 (load) should not be clearable by write
                self.control = value | (self.control & 0x40);
            }

            // Load registers (ignored for stub)
            0x24 | 0x28 | 0x2C | 0x30 | 0x31 => {}

            // Interrupt acknowledge (write to clear)
            0x34 => {
                self.interrupt &= !value;
            }

            _ => {}
        }
    }

    /// Tick the RTC (called periodically)
    /// For the stub, we don't actually advance time
    pub fn tick(&mut self, _cycles: u32) -> bool {
        // Return false - no interrupt pending from stub
        false
    }
}

impl Default for RtcController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let rtc = RtcController::new();
        assert_eq!(rtc.control, 0x81); // Enable + latch enable
        assert_eq!(rtc.latched_sec, 30);
    }

    #[test]
    fn test_read_time() {
        let rtc = RtcController::new();
        assert_eq!(rtc.read(0x00), 30); // sec
        assert_eq!(rtc.read(0x04), 15); // min
        assert_eq!(rtc.read(0x08), 12); // hour
    }

    #[test]
    fn test_read_revision() {
        let rtc = RtcController::new();
        assert_eq!(rtc.read(0x3C), 0x00);
        assert_eq!(rtc.read(0x3D), 0x05);
        assert_eq!(rtc.read(0x3E), 0x01);
        assert_eq!(rtc.read(0x3F), 0x00);
    }

    #[test]
    fn test_control() {
        let mut rtc = RtcController::new();
        rtc.write(0x20, 0x01); // Enable only
        assert_eq!(rtc.read(0x20), 0x01);
    }

    #[test]
    fn test_interrupt_ack() {
        let mut rtc = RtcController::new();
        rtc.interrupt = 0xFF;
        rtc.write(0x34, 0x0F); // Clear lower 4 bits
        assert_eq!(rtc.interrupt, 0xF0);
    }
}
