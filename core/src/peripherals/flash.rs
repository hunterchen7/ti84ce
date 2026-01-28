//! TI-84 Plus CE Flash Controller
//!
//! Memory-mapped at 0xE10000 (port offset 0x010000 from 0xE00000)
//!
//! The flash controller manages flash memory access, wait states, and
//! memory mapping configuration.
//!
//! Reference: CEmu flash.c

/// Register offsets
mod regs {
    /// Flash enable (bit 0 writable)
    pub const ENABLE: u32 = 0x00;
    /// Flash size configuration
    pub const SIZE_CONFIG: u32 = 0x01;
    /// Flash map selection (bits 0-3)
    pub const MAP_SELECT: u32 = 0x02;
    /// Wait states (added to base 6 cycles)
    pub const WAIT_STATES: u32 = 0x05;
    /// General control flag (bit 0 writable)
    pub const CONTROL: u32 = 0x08;
}

/// Flash Controller
///
/// Emulates the TI-84 Plus CE flash controller at 0xE10000.
/// The flash controller handles:
/// - Flash enable/disable
/// - Flash size configuration
/// - Memory mapping selection
/// - Wait state configuration
#[derive(Debug, Clone)]
pub struct FlashController {
    /// Flash enable (bit 0)
    enable: u8,
    /// Size configuration
    size_config: u8,
    /// Map selection (bits 0-3)
    map_select: u8,
    /// Wait states offset
    wait_states: u8,
    /// Control flag (bit 0)
    control: u8,
}

impl FlashController {
    /// Create a new flash controller with default "ready" values
    pub fn new() -> Self {
        Self {
            // Flash enabled by default - ROM expects flash to be accessible
            enable: 0x01,
            // Default size configuration (4MB flash)
            size_config: 0x07,
            // Default map selection
            map_select: 0x00,
            // Default wait states (0 extra wait states)
            wait_states: 0x00,
            // Control flag
            control: 0x00,
        }
    }

    /// Reset the flash controller
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Check if flash is enabled
    pub fn is_enabled(&self) -> bool {
        self.enable & 0x01 != 0
    }

    /// Get the configured wait states (added to base 6)
    pub fn wait_states(&self) -> u8 {
        self.wait_states
    }

    /// Get the total wait cycles (base 6 + configured wait states)
    pub fn total_wait_cycles(&self) -> u8 {
        6 + self.wait_states
    }

    /// Get the map selection value
    pub fn map_select(&self) -> u8 {
        self.map_select
    }

    /// Calculate the mapped flash size in bytes
    /// Formula: 0x10000 << map_select (capped at map < 8)
    pub fn mapped_bytes(&self) -> u32 {
        let map = self.map_select & 0x0F;
        if self.enable == 0 || self.size_config > 0x3F {
            0
        } else {
            0x10000 << (if map < 8 { map } else { 0 })
        }
    }

    /// Read a register byte
    /// addr is offset from controller base (0x00-0xFF)
    pub fn read(&self, addr: u32) -> u8 {
        match addr {
            regs::ENABLE => self.enable,
            regs::SIZE_CONFIG => self.size_config,
            regs::MAP_SELECT => self.map_select,
            regs::WAIT_STATES => self.wait_states,
            regs::CONTROL => self.control,
            // Other registers return 0xFF (unprogrammed flash default)
            _ => 0xFF,
        }
    }

    /// Write a register byte
    /// addr is offset from controller base (0x00-0xFF)
    pub fn write(&mut self, addr: u32, value: u8) {
        match addr {
            regs::ENABLE => {
                // Only bit 0 is writable
                self.enable = value & 0x01;
            }
            regs::SIZE_CONFIG => {
                self.size_config = value;
            }
            regs::MAP_SELECT => {
                // Only bits 0-3 are writable
                self.map_select = value & 0x0F;
            }
            regs::WAIT_STATES => {
                self.wait_states = value;
            }
            regs::CONTROL => {
                // Only bit 0 is writable
                self.control = value & 0x01;
            }
            _ => {
                // Other addresses are ignored
            }
        }
    }
}

impl Default for FlashController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let flash = FlashController::new();
        assert!(flash.is_enabled());
        assert_eq!(flash.wait_states(), 0);
        assert_eq!(flash.total_wait_cycles(), 6);
    }

    #[test]
    fn test_reset() {
        let mut flash = FlashController::new();
        flash.enable = 0x00;
        flash.wait_states = 0x10;
        flash.map_select = 0x05;

        flash.reset();
        assert!(flash.is_enabled());
        assert_eq!(flash.wait_states(), 0);
        assert_eq!(flash.map_select(), 0);
    }

    #[test]
    fn test_read_enable() {
        let flash = FlashController::new();
        assert_eq!(flash.read(regs::ENABLE), 0x01);
    }

    #[test]
    fn test_write_enable() {
        let mut flash = FlashController::new();

        // Disable flash
        flash.write(regs::ENABLE, 0x00);
        assert!(!flash.is_enabled());
        assert_eq!(flash.read(regs::ENABLE), 0x00);

        // Enable flash
        flash.write(regs::ENABLE, 0x01);
        assert!(flash.is_enabled());
        assert_eq!(flash.read(regs::ENABLE), 0x01);
    }

    #[test]
    fn test_enable_masked() {
        let mut flash = FlashController::new();

        // Writing value > 1 should be masked to bit 0
        flash.write(regs::ENABLE, 0xFF);
        assert_eq!(flash.read(regs::ENABLE), 0x01);
    }

    #[test]
    fn test_size_config() {
        let mut flash = FlashController::new();

        flash.write(regs::SIZE_CONFIG, 0x3F);
        assert_eq!(flash.read(regs::SIZE_CONFIG), 0x3F);

        flash.write(regs::SIZE_CONFIG, 0x07);
        assert_eq!(flash.read(regs::SIZE_CONFIG), 0x07);
    }

    #[test]
    fn test_map_select() {
        let mut flash = FlashController::new();

        flash.write(regs::MAP_SELECT, 0x05);
        assert_eq!(flash.read(regs::MAP_SELECT), 0x05);
        assert_eq!(flash.map_select(), 0x05);
    }

    #[test]
    fn test_map_select_masked() {
        let mut flash = FlashController::new();

        // Only bits 0-3 should be writable
        flash.write(regs::MAP_SELECT, 0xFF);
        assert_eq!(flash.read(regs::MAP_SELECT), 0x0F);
    }

    #[test]
    fn test_wait_states() {
        let mut flash = FlashController::new();

        flash.write(regs::WAIT_STATES, 0x04);
        assert_eq!(flash.read(regs::WAIT_STATES), 0x04);
        assert_eq!(flash.wait_states(), 0x04);
        assert_eq!(flash.total_wait_cycles(), 10); // 6 + 4
    }

    #[test]
    fn test_control() {
        let mut flash = FlashController::new();

        flash.write(regs::CONTROL, 0x01);
        assert_eq!(flash.read(regs::CONTROL), 0x01);
    }

    #[test]
    fn test_control_masked() {
        let mut flash = FlashController::new();

        // Only bit 0 should be writable
        flash.write(regs::CONTROL, 0xFF);
        assert_eq!(flash.read(regs::CONTROL), 0x01);
    }

    #[test]
    fn test_unmapped_returns_ff() {
        let flash = FlashController::new();

        // Unmapped registers should return 0xFF
        assert_eq!(flash.read(0x03), 0xFF);
        assert_eq!(flash.read(0x04), 0xFF);
        assert_eq!(flash.read(0x06), 0xFF);
        assert_eq!(flash.read(0x07), 0xFF);
        assert_eq!(flash.read(0x10), 0xFF);
    }

    #[test]
    fn test_unmapped_writes_ignored() {
        let mut flash = FlashController::new();

        // Writing to unmapped addresses should be ignored
        flash.write(0x03, 0x55);
        flash.write(0x04, 0x55);
        flash.write(0x10, 0x55);

        // Should still return 0xFF
        assert_eq!(flash.read(0x03), 0xFF);
        assert_eq!(flash.read(0x04), 0xFF);
        assert_eq!(flash.read(0x10), 0xFF);
    }

    #[test]
    fn test_mapped_bytes() {
        let mut flash = FlashController::new();

        // map_select = 0 -> 0x10000 (64KB)
        flash.write(regs::MAP_SELECT, 0x00);
        assert_eq!(flash.mapped_bytes(), 0x10000);

        // map_select = 1 -> 0x20000 (128KB)
        flash.write(regs::MAP_SELECT, 0x01);
        assert_eq!(flash.mapped_bytes(), 0x20000);

        // map_select = 7 -> 0x800000 (8MB)
        flash.write(regs::MAP_SELECT, 0x07);
        assert_eq!(flash.mapped_bytes(), 0x800000);

        // map_select >= 8 should clamp to 0 (use map=0)
        flash.write(regs::MAP_SELECT, 0x08);
        assert_eq!(flash.mapped_bytes(), 0x10000);
    }

    #[test]
    fn test_mapped_bytes_disabled() {
        let mut flash = FlashController::new();

        // When flash is disabled, mapped_bytes should be 0
        flash.write(regs::ENABLE, 0x00);
        assert_eq!(flash.mapped_bytes(), 0);
    }

    #[test]
    fn test_mapped_bytes_invalid_size_config() {
        let mut flash = FlashController::new();

        // When size_config > 0x3F, mapping should be disabled
        flash.write(regs::SIZE_CONFIG, 0x40);
        assert_eq!(flash.mapped_bytes(), 0);
    }

    #[test]
    fn test_default_values_for_boot() {
        // The flash controller should return sensible defaults that indicate "flash ready"
        let flash = FlashController::new();

        // Flash should be enabled
        assert!(flash.is_enabled());
        assert_eq!(flash.read(regs::ENABLE), 0x01);

        // Size config should be set for 4MB flash (0x07)
        assert_eq!(flash.read(regs::SIZE_CONFIG), 0x07);

        // Wait states should be 0 (6 base cycles)
        assert_eq!(flash.read(regs::WAIT_STATES), 0x00);
    }
}
