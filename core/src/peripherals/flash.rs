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

    // Cached mapping state (updated by recalculate_mapping)
    /// Cached: whether flash mapping is currently active
    mapping_enabled: bool,
    /// Cached: the mapped flash size in bytes
    cached_mapped_bytes: u32,
    /// Cached: total wait cycles (base 6 + wait_states)
    cached_total_wait_cycles: u8,
}

impl FlashController {
    /// Create a new flash controller with default "ready" values
    /// Values match CEmu's flash_reset() initialization
    pub fn new() -> Self {
        let mut controller = Self {
            // Flash enabled by default - ROM expects flash to be accessible
            enable: 0x01,
            // CEmu flash_reset() memsets to 0; ROM writes correct value during init
            size_config: 0x00,
            // CEmu defaults map_select to 0x06
            map_select: 0x06,
            // CEmu defaults wait_states to 0x04 (total 10 wait cycles = 6 base + 4)
            wait_states: 0x04,
            // Control flag
            control: 0x00,

            // Cached values will be set by recalculate_mapping
            mapping_enabled: false,
            cached_mapped_bytes: 0,
            // Initialize to match wait_states (base 6 + 4 = 10)
            cached_total_wait_cycles: 10,
        };
        controller.recalculate_mapping();
        controller
    }

    /// Reset the flash controller
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Recalculate cached mapping state based on current register values.
    ///
    /// This method is called automatically when ENABLE (0x00), SIZE (0x01),
    /// or MAP (0x02) registers are written, matching CEmu's flash_set_map()
    /// behavior.
    ///
    /// Updates:
    /// - `mapping_enabled`: Whether flash mapping is active
    /// - `cached_mapped_bytes`: The calculated mapped flash size
    /// - `cached_total_wait_cycles`: Total wait cycles (base 6 + wait_states)
    fn recalculate_mapping(&mut self) {
        // Check if mapping should be enabled
        // Mapping is disabled if flash is disabled or size_config is invalid
        self.mapping_enabled = (self.enable & 0x01) != 0 && self.size_config <= 0x3F;

        // Calculate mapped bytes
        if self.mapping_enabled {
            let map = self.map_select & 0x0F;
            // Values >= 8 fall back to map=0
            let effective_map = if map < 8 { map } else { 0 };
            self.cached_mapped_bytes = 0x10000u32 << effective_map;
        } else {
            self.cached_mapped_bytes = 0;
        }

        // Update total wait cycles
        self.cached_total_wait_cycles = 6u8.saturating_add(self.wait_states);
    }

    /// Check if flash mapping is currently enabled (cached value)
    pub fn is_mapping_enabled(&self) -> bool {
        self.mapping_enabled
    }

    /// Get the cached mapped flash size in bytes
    pub fn cached_mapped_bytes(&self) -> u32 {
        self.cached_mapped_bytes
    }

    /// Get the cached total wait cycles
    pub fn cached_total_wait_cycles(&self) -> u8 {
        self.cached_total_wait_cycles
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
    /// Uses saturating addition to prevent overflow with large wait_states values
    pub fn total_wait_cycles(&self) -> u8 {
        6u8.saturating_add(self.wait_states)
    }

    /// Get the map selection value
    pub fn map_select(&self) -> u8 {
        self.map_select
    }

    /// Calculate the mapped flash size in bytes
    /// Formula: 0x10000 << map_select for map_select 0-7
    /// Values >= 8 fall back to map_select=0 (returns 0x10000)
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
                // Recalculate mapping when enable changes (CEmu flash_set_map)
                self.recalculate_mapping();
            }
            regs::SIZE_CONFIG => {
                self.size_config = value;
                // Recalculate mapping when size config changes (CEmu flash_set_map)
                self.recalculate_mapping();
            }
            regs::MAP_SELECT => {
                // Only bits 0-3 are writable
                self.map_select = value & 0x0F;
                // Recalculate mapping when map selection changes (CEmu flash_set_map)
                self.recalculate_mapping();
            }
            regs::WAIT_STATES => {
                self.wait_states = value;
                // Update cached wait cycles when wait states change
                self.recalculate_mapping();
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
        // CEmu defaults to wait_states=0x04 (total 10 cycles)
        assert_eq!(flash.wait_states(), 0x04);
        assert_eq!(flash.total_wait_cycles(), 10);
        // CEmu defaults to map_select=0x06
        assert_eq!(flash.map_select(), 0x06);
    }

    #[test]
    fn test_reset() {
        let mut flash = FlashController::new();
        flash.enable = 0x00;
        flash.wait_states = 0x10;
        flash.map_select = 0x05;

        flash.reset();
        assert!(flash.is_enabled());
        // CEmu defaults
        assert_eq!(flash.wait_states(), 0x04);
        assert_eq!(flash.map_select(), 0x06);
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
    fn test_wait_states_saturates() {
        let mut flash = FlashController::new();

        // Writing 0xFF should saturate total_wait_cycles to 255
        flash.write(regs::WAIT_STATES, 0xFF);
        assert_eq!(flash.wait_states(), 0xFF);
        assert_eq!(flash.total_wait_cycles(), 255); // saturates at u8::MAX
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
        // The flash controller should return CEmu-matching defaults
        let flash = FlashController::new();

        // Flash should be enabled
        assert!(flash.is_enabled());
        assert_eq!(flash.read(regs::ENABLE), 0x01);

        // CEmu memsets flash to 0; size_config starts at 0x00
        assert_eq!(flash.read(regs::SIZE_CONFIG), 0x00);

        // CEmu defaults wait states to 0x04 (10 total cycles)
        assert_eq!(flash.read(regs::WAIT_STATES), 0x04);

        // CEmu defaults map_select to 0x06
        assert_eq!(flash.read(regs::MAP_SELECT), 0x06);
    }

    // Tests for recalculate_mapping() and cached values

    #[test]
    fn test_cached_values_initialized() {
        let flash = FlashController::new();

        // Cached values should match computed values after initialization
        assert!(flash.is_mapping_enabled());
        // map_select=0x06 -> 0x10000 << 6 = 0x400000 (4MB)
        assert_eq!(flash.cached_mapped_bytes(), 0x400000);
        // wait_states=0x04 -> 6 + 4 = 10
        assert_eq!(flash.cached_total_wait_cycles(), 10);
    }

    #[test]
    fn test_cached_values_match_computed() {
        let flash = FlashController::new();

        // Cached values should match the computed versions
        assert_eq!(flash.cached_mapped_bytes(), flash.mapped_bytes());
        assert_eq!(flash.cached_total_wait_cycles(), flash.total_wait_cycles());
        assert_eq!(flash.is_mapping_enabled(), flash.is_enabled());
    }

    #[test]
    fn test_recalculate_on_enable_write() {
        let mut flash = FlashController::new();

        // Initially enabled
        assert!(flash.is_mapping_enabled());
        assert_eq!(flash.cached_mapped_bytes(), 0x400000);

        // Disable flash - should recalculate
        flash.write(regs::ENABLE, 0x00);
        assert!(!flash.is_mapping_enabled());
        assert_eq!(flash.cached_mapped_bytes(), 0);

        // Re-enable flash - should recalculate
        flash.write(regs::ENABLE, 0x01);
        assert!(flash.is_mapping_enabled());
        assert_eq!(flash.cached_mapped_bytes(), 0x400000);
    }

    #[test]
    fn test_recalculate_on_size_config_write() {
        let mut flash = FlashController::new();

        // Initially valid size config
        assert!(flash.is_mapping_enabled());
        assert_eq!(flash.cached_mapped_bytes(), 0x400000);

        // Set invalid size config (> 0x3F) - should disable mapping
        flash.write(regs::SIZE_CONFIG, 0x40);
        assert!(!flash.is_mapping_enabled());
        assert_eq!(flash.cached_mapped_bytes(), 0);

        // Set valid size config - should re-enable mapping
        flash.write(regs::SIZE_CONFIG, 0x07);
        assert!(flash.is_mapping_enabled());
        assert_eq!(flash.cached_mapped_bytes(), 0x400000);
    }

    #[test]
    fn test_recalculate_on_map_select_write() {
        let mut flash = FlashController::new();

        // Change map_select from default 0x06 to 0x00
        flash.write(regs::MAP_SELECT, 0x00);
        assert_eq!(flash.cached_mapped_bytes(), 0x10000); // 64KB

        // Change to map_select 0x07
        flash.write(regs::MAP_SELECT, 0x07);
        assert_eq!(flash.cached_mapped_bytes(), 0x800000); // 8MB

        // Change to map_select >= 8 (should clamp to 0)
        flash.write(regs::MAP_SELECT, 0x0F);
        assert_eq!(flash.cached_mapped_bytes(), 0x10000); // Falls back to 64KB
    }

    #[test]
    fn test_recalculate_on_wait_states_write() {
        let mut flash = FlashController::new();

        // Default wait_states is 0x04
        assert_eq!(flash.cached_total_wait_cycles(), 10);

        // Change wait_states to 0x00
        flash.write(regs::WAIT_STATES, 0x00);
        assert_eq!(flash.cached_total_wait_cycles(), 6); // Base only

        // Change wait_states to 0xFF
        flash.write(regs::WAIT_STATES, 0xFF);
        assert_eq!(flash.cached_total_wait_cycles(), 255); // Saturates
    }

    #[test]
    fn test_cached_matches_computed_after_writes() {
        let mut flash = FlashController::new();

        // Perform various writes and verify cached matches computed
        flash.write(regs::MAP_SELECT, 0x03);
        assert_eq!(flash.cached_mapped_bytes(), flash.mapped_bytes());
        assert_eq!(flash.cached_total_wait_cycles(), flash.total_wait_cycles());

        flash.write(regs::WAIT_STATES, 0x10);
        assert_eq!(flash.cached_mapped_bytes(), flash.mapped_bytes());
        assert_eq!(flash.cached_total_wait_cycles(), flash.total_wait_cycles());

        flash.write(regs::ENABLE, 0x00);
        assert_eq!(flash.cached_mapped_bytes(), flash.mapped_bytes());

        flash.write(regs::SIZE_CONFIG, 0x20);
        flash.write(regs::ENABLE, 0x01);
        assert_eq!(flash.cached_mapped_bytes(), flash.mapped_bytes());
    }

    #[test]
    fn test_control_write_does_not_recalculate() {
        let mut flash = FlashController::new();
        let initial_cached_bytes = flash.cached_mapped_bytes();
        let initial_cached_wait = flash.cached_total_wait_cycles();

        // Writing to CONTROL should not affect cached mapping values
        flash.write(regs::CONTROL, 0x01);
        assert_eq!(flash.cached_mapped_bytes(), initial_cached_bytes);
        assert_eq!(flash.cached_total_wait_cycles(), initial_cached_wait);
    }

    #[test]
    fn test_reset_recalculates() {
        let mut flash = FlashController::new();

        // Modify state
        flash.write(regs::ENABLE, 0x00);
        flash.write(regs::MAP_SELECT, 0x02);
        flash.write(regs::WAIT_STATES, 0x20);

        assert!(!flash.is_mapping_enabled());
        assert_eq!(flash.cached_mapped_bytes(), 0);

        // Reset should restore defaults and recalculate
        flash.reset();
        assert!(flash.is_mapping_enabled());
        assert_eq!(flash.cached_mapped_bytes(), 0x400000); // Default map_select=0x06
        assert_eq!(flash.cached_total_wait_cycles(), 10); // Default wait_states=0x04
    }
}
