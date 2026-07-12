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
//!   0x10:      Status (read-only)
//!   0x14-0x17: Status clear (write-only)
//!   0x18:      Pulse load (8-bit)
//!   0x1C-0x1F: Revision (0x00010602, read-only)

pub const WATCHDOG_ACTION_RESET: u8 = 1 << 1;
pub const WATCHDOG_ACTION_NMI: u8 = 1 << 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchdogMode {
    Counter,
    Expired,
    Pulse,
    Reload,
}

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
    /// Current pulse countdown
    pulse_count: u8,
    /// Current watchdog phase
    mode: WatchdogMode,
    /// CPU cycles remaining in the output pulse phase
    pulse_cycles_remaining: u64,
    /// Fractional accumulator for the 32KHz watchdog clock
    clock_accumulator: u64,
    /// Actions raised by register writes during an active pulse
    pending_actions: u8,
    /// Restart requested on the last cycle of a pulse
    pending_reload: bool,
    /// Suppress the status latch for an expiry already in flight
    block_status: bool,
    /// Suppress pulse-count writes until a zero-length pulse completes
    block_pulse_reload: bool,
}

impl WatchdogController {
    pub const SNAPSHOT_SIZE: usize = 32;

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
            pulse_count: 0xFF,
            mode: WatchdogMode::Counter,
            pulse_cycles_remaining: 0,
            clock_accumulator: 0,
            pending_actions: 0,
            pending_reload: false,
            block_status: false,
            block_pulse_reload: false,
        }
    }

    /// Reset the Watchdog controller
    pub fn reset(&mut self) {
        self.count = Self::DEFAULT_LOAD;
        self.load = Self::DEFAULT_LOAD;
        self.control = 0x00;
        self.status = 0x00;
        self.pulse_load = 0xFF;
        self.pulse_count = 0xFF;
        self.mode = WatchdogMode::Counter;
        self.pulse_cycles_remaining = 0;
        self.clock_accumulator = 0;
        self.pending_actions = 0;
        self.pending_reload = false;
        self.block_status = false;
        self.block_pulse_reload = false;
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

            // Current pulse countdown
            0x18 => self.pulse_count,

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
                if self.mode == WatchdogMode::Pulse && self.count == 0 {
                    self.count = self.load;
                }
            }

            // Restart (write 0xB9 to reload counter from load value)
            0x08 => {
                if value == 0xB9 {
                    if self.mode == WatchdogMode::Counter && self.control & 1 != 0 {
                        self.mode = WatchdogMode::Reload;
                        self.pulse_cycles_remaining = 2;
                    } else if self.mode == WatchdogMode::Pulse && self.pulse_cycles_remaining == 1 {
                        self.pending_reload = true;
                    } else {
                        self.count = self.load;
                        if self.mode == WatchdogMode::Counter && self.count == 0 {
                            self.mode = WatchdogMode::Pulse;
                            if self.pulse_count == 0 {
                                self.block_pulse_reload = true;
                                self.pulse_cycles_remaining = 1;
                            }
                        }
                    }
                }
            }

            // Control register
            0x0C => {
                let old = self.control;
                self.control = value;
                if self.mode == WatchdogMode::Pulse && value & old & 1 != 0 {
                    self.pending_actions |= value & !old & 0x06;
                } else if self.mode == WatchdogMode::Pulse && value & 1 != 0 && old & 1 == 0 {
                    self.pending_actions |= value & 0x06;
                    self.pulse_cycles_remaining = u64::from(self.pulse_count) + 1;
                }
            }

            // Status clear (write-to-clear)
            0x14..=0x17 => {
                self.status = 0;
                if self.mode == WatchdogMode::Expired {
                    self.block_status = true;
                }
            }

            // Pulse load
            0x18..=0x1B => {
                if index == 0x18 {
                    self.pulse_load = value;
                }
                if !self.block_pulse_reload {
                    self.pulse_count = self.pulse_load;
                    if self.mode == WatchdogMode::Pulse {
                        self.pulse_cycles_remaining = u64::from(self.pulse_count) + 2;
                    }
                }
            }

            _ => {}
        }
    }

    /// Advance the watchdog and return any reset/NMI output actions.
    pub fn tick(&mut self, cycles: u32, cpu_speed: u8) -> u8 {
        let mut actions = std::mem::take(&mut self.pending_actions);
        let mut remaining_cycles = u64::from(cycles);

        if matches!(self.mode, WatchdogMode::Expired | WatchdogMode::Reload) {
            if remaining_cycles < self.pulse_cycles_remaining {
                self.pulse_cycles_remaining -= remaining_cycles;
                return actions;
            }
            remaining_cycles -= self.pulse_cycles_remaining;
            self.pulse_cycles_remaining = 0;

            if self.mode == WatchdogMode::Reload {
                self.count = self.load;
                self.mode = WatchdogMode::Counter;
            } else {
                self.count = self.load;
                if self.control & 1 != 0 {
                    actions |= self.control & 0x06;
                    if !self.block_status {
                        self.status = 1;
                    }
                }
                self.block_status = false;
                self.mode = WatchdogMode::Pulse;
                self.pulse_cycles_remaining = u64::from(self.pulse_count) + 1;
                if self.pulse_count == 0 {
                    self.block_pulse_reload = true;
                }
            }
        }

        if self.mode == WatchdogMode::Pulse {
            if self.control & 1 == 0 && self.pulse_count != 0 {
                return actions;
            }
            let consumed = remaining_cycles.min(self.pulse_cycles_remaining);
            self.pulse_cycles_remaining -= consumed;
            remaining_cycles -= consumed;
            self.pulse_count = self
                .pulse_cycles_remaining
                .saturating_sub(1)
                .min(u64::from(u8::MAX)) as u8;

            if self.pulse_cycles_remaining != 0 {
                return actions;
            }

            self.pulse_count = self.pulse_load;
            self.block_pulse_reload = false;
            self.mode = if self.pending_reload {
                self.pending_reload = false;
                self.pulse_cycles_remaining = 1;
                WatchdogMode::Reload
            } else if self.count == 0 {
                self.pulse_cycles_remaining = 1;
                WatchdogMode::Expired
            } else {
                WatchdogMode::Counter
            };
        }

        if self.control & 1 == 0 || remaining_cycles == 0 {
            return actions;
        }

        let ticks = if self.control & 0x10 == 0 {
            remaining_cycles
        } else {
            let cpu_rate = match cpu_speed & 3 {
                0 => 6_000_000u64,
                1 => 12_000_000u64,
                2 => 24_000_000u64,
                _ => 48_000_000u64,
            };
            self.clock_accumulator = self
                .clock_accumulator
                .saturating_add(remaining_cycles.saturating_mul(32_768));
            let ticks = self.clock_accumulator / cpu_rate;
            self.clock_accumulator %= cpu_rate;
            ticks
        };

        if ticks == 0 {
            return actions;
        }

        if ticks < u64::from(self.count) {
            self.count -= ticks as u32;
            return actions;
        }

        self.count = 0;
        self.mode = WatchdogMode::Expired;
        self.pulse_cycles_remaining = 1;
        actions
    }

    pub fn to_bytes(&self) -> [u8; Self::SNAPSHOT_SIZE] {
        let mut bytes = [0u8; Self::SNAPSHOT_SIZE];
        bytes[0..4].copy_from_slice(&self.count.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.load.to_le_bytes());
        bytes[8] = self.control;
        bytes[9] = self.status;
        bytes[10] = self.pulse_load;
        bytes[11] = self.pulse_count;
        bytes[12] = match self.mode {
            WatchdogMode::Counter => 0,
            WatchdogMode::Expired => 1,
            WatchdogMode::Pulse => 2,
            WatchdogMode::Reload => 3,
        };
        bytes[13] = u8::from(self.pending_reload);
        bytes[14] = u8::from(self.block_status);
        bytes[15] = u8::from(self.block_pulse_reload);
        bytes[16..24].copy_from_slice(&self.pulse_cycles_remaining.to_le_bytes());
        bytes[24..32].copy_from_slice(&self.clock_accumulator.to_le_bytes());
        bytes
    }

    pub fn from_bytes(&mut self, bytes: &[u8]) -> Result<(), i32> {
        if bytes.len() < Self::SNAPSHOT_SIZE {
            return Err(-105);
        }
        self.count = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        self.load = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        self.control = bytes[8];
        self.status = bytes[9];
        self.pulse_load = bytes[10];
        self.pulse_count = bytes[11];
        self.mode = match bytes[12] {
            0 => WatchdogMode::Counter,
            1 => WatchdogMode::Expired,
            2 => WatchdogMode::Pulse,
            3 => WatchdogMode::Reload,
            _ => return Err(-105),
        };
        self.pending_reload = bytes[13] != 0;
        self.block_status = bytes[14] != 0;
        self.block_pulse_reload = bytes[15] != 0;
        self.pulse_cycles_remaining = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
        self.clock_accumulator = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
        self.pending_actions = 0;
        Ok(())
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
        wdt.write(0x14, 0xFF); // Write-to-clear
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
    fn test_tick_disabled() {
        let mut wdt = WatchdogController::new();
        assert_eq!(wdt.tick(1000, 0), 0);
        assert_eq!(wdt.count, WatchdogController::DEFAULT_LOAD);
    }

    #[test]
    fn test_expiry_raises_configured_actions_after_expiry_cycle() {
        let mut wdt = WatchdogController::new();
        wdt.load = 2;
        wdt.count = 2;
        wdt.control = 0x07;

        assert_eq!(wdt.tick(2, 0), 0);
        assert_eq!(wdt.status, 0);
        assert_eq!(wdt.tick(1, 0), WATCHDOG_ACTION_RESET | WATCHDOG_ACTION_NMI);
        assert_eq!(wdt.status, 1);
    }

    #[test]
    fn test_snapshot_round_trip() {
        let mut original = WatchdogController::new();
        original.load = 42;
        original.count = 17;
        original.control = 0x11;
        original.clock_accumulator = 1234;

        let mut restored = WatchdogController::new();
        restored.from_bytes(&original.to_bytes()).unwrap();
        assert_eq!(restored.to_bytes(), original.to_bytes());
    }
}
