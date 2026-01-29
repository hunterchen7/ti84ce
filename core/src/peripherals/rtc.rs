//! TI-84 Plus CE Real-Time Clock
//!
//! Memory-mapped at 0xF80000 (port offset 0x180000 from 0xE00000)
//! Also accessible via I/O port range 0x8xxx
//!
//! Based on CEmu's realclock.c implementation.

/// Load status gets set 1 tick after each load completes (from CEmu)
const LOAD_SEC_FINISHED: u8 = 1 + 8;
const LOAD_MIN_FINISHED: u8 = LOAD_SEC_FINISHED + 8;
const LOAD_HOUR_FINISHED: u8 = LOAD_MIN_FINISHED + 8;
const LOAD_DAY_FINISHED: u8 = LOAD_HOUR_FINISHED + 16;
const LOAD_TOTAL_TICKS: u8 = LOAD_DAY_FINISHED + 10;
/// LOAD_PENDING = 255 (UINT8_MAX in CEmu)
const LOAD_PENDING: u8 = 255;

/// RTC Controller
#[derive(Debug, Clone)]
pub struct RtcController {
    /// Control register (bit 0 = enable, bit 6 = load, bit 7 = latch enable)
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
    /// Load ticks processed (255 = LOAD_PENDING, >= LOAD_TOTAL_TICKS = complete)
    load_ticks_processed: u8,
    /// Access counter for simulating load timing without scheduler
    access_count: u32,
}

impl RtcController {
    /// Revision value returned at offset 0x3C-0x3F
    const REVISION: u32 = 0x00010500;

    /// Create a new RTC controller
    /// Values match CEmu's rtc_reset() which uses memset(0)
    pub fn new() -> Self {
        Self {
            control: 0, // CEmu memsets to 0
            interrupt: 0,
            latched_sec: 0,
            latched_min: 0,
            latched_hour: 0,
            latched_day: 0,
            load_ticks_processed: LOAD_TOTAL_TICKS, // Load complete initially
            access_count: 0,
        }
    }

    /// Reset the RTC controller
    pub fn reset(&mut self) {
        self.control = 0; // CEmu memsets to 0
        self.interrupt = 0;
        self.latched_sec = 0;
        self.latched_min = 0;
        self.latched_hour = 0;
        self.latched_day = 0;
        self.load_ticks_processed = LOAD_TOTAL_TICKS; // Load complete
        self.access_count = 0;
    }

    /// Read a register byte
    /// addr is offset from controller base (0-0xFF)
    pub fn read(&mut self, addr: u32) -> u8 {
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
            0x20 => {
                // Note: CEmu calls rtc_update_load() here but for parity
                // we need to be careful - don't advance ticks on control reads
                self.control
            }

            // Load registers (return 0)
            0x24 => 0, // load sec
            0x28 => 0, // load min
            0x2C => 0, // load hour
            0x30 | 0x31 => 0, // load day

            // Interrupt status
            0x34 => self.interrupt,

            // Revision (0x00010500)
            0x3C..=0x3F => ((Self::REVISION >> bit_offset) & 0xFF) as u8,

            // Load status
            0x40 => {
                self.update_load();
                // Convert to i8 to treat LOAD_PENDING (255) as -1, matching CEmu
                let ticks = self.load_ticks_processed as i8;
                if ticks >= LOAD_TOTAL_TICKS as i8 {
                    0
                } else {
                    // Bits set indicate load is still in progress for each field
                    8 | ((ticks < LOAD_SEC_FINISHED as i8) as u8) << 4
                      | ((ticks < LOAD_MIN_FINISHED as i8) as u8) << 5
                      | ((ticks < LOAD_HOUR_FINISHED as i8) as u8) << 6
                      | ((ticks < LOAD_DAY_FINISHED as i8) as u8) << 7
                }
            }

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

    /// Simulate load progress (without proper scheduler, use access-based timing)
    fn update_load(&mut self) {
        // If load is pending (255), keep it pending indefinitely
        // CEmu's scheduler advances this at 32kHz based on real time
        // Without proper scheduler integration, we just keep it pending
        // The ROM code handles the pending state correctly (polls and waits)
        // This matches CEmu behavior during early boot
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
                if value & 0x40 != 0 {
                    // Writing bit 6 starts a load operation
                    self.update_load();
                    if self.control & 0x40 == 0 {
                        // Load can be pended once previous load is finished
                        // Previous load is finished when load_ticks_processed >= RTC_DATETIME_BITS (40)
                        if self.load_ticks_processed >= 40 {
                            self.load_ticks_processed = LOAD_PENDING;
                        }
                    }
                    self.control = value;
                } else {
                    // Don't allow resetting the load bit via write
                    self.control = value | (self.control & 0x40);
                }
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
        assert_eq!(rtc.control, 0); // CEmu memsets to 0
        assert_eq!(rtc.latched_sec, 0);
    }

    #[test]
    fn test_read_time() {
        let mut rtc = RtcController::new();
        // CEmu initializes all time values to 0
        assert_eq!(rtc.read(0x00), 0); // sec
        assert_eq!(rtc.read(0x04), 0); // min
        assert_eq!(rtc.read(0x08), 0); // hour
    }

    #[test]
    fn test_read_revision() {
        let mut rtc = RtcController::new();
        assert_eq!(rtc.read(0x3C), 0x00);
        assert_eq!(rtc.read(0x3D), 0x05);
        assert_eq!(rtc.read(0x3E), 0x01);
        assert_eq!(rtc.read(0x3F), 0x00);
    }

    #[test]
    fn test_load_status_complete() {
        let mut rtc = RtcController::new();
        // After new(), load should be complete
        assert_eq!(rtc.read(0x40), 0); // 0 means load complete
    }

    #[test]
    fn test_load_status_pending() {
        let mut rtc = RtcController::new();
        // Trigger a load by writing bit 6 to control
        rtc.write(0x20, 0xC1); // Enable + load + latch enable
        // Should now show load pending (0xF8)
        assert_eq!(rtc.read(0x40), 0xF8);
        // Without scheduler integration, load stays pending indefinitely
        // (matches CEmu behavior during early boot)
        for _ in 0..10 {
            rtc.read(0x40);
        }
        assert_eq!(rtc.read(0x40), 0xF8); // Still pending
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
