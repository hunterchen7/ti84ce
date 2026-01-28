//! TI-84 Plus CE Control Ports
//!
//! Memory-mapped at 0xE00000 (also accessible via OUT0/IN0 at 0xFF00xx)
//!
//! These ports control system-level functions like CPU speed, battery status,
//! and memory protection.

/// Register offsets
mod regs {
    /// Power control
    pub const POWER: u32 = 0x00;
    /// CPU speed selection (6/12/24/48 MHz)
    pub const CPU_SPEED: u32 = 0x01;
    /// Battery status
    pub const BATTERY_STATUS: u32 = 0x02;
    /// Device type / serial flash indicator
    pub const DEVICE_TYPE: u32 = 0x03;
    /// Control flags
    pub const CONTROL_FLAGS: u32 = 0x05;
    /// Protected ports unlock
    pub const UNLOCK_STATUS: u32 = 0x06;
    /// Battery configuration
    pub const BATTERY_CONFIG: u32 = 0x07;
    /// Fixed value register (returns 0x7F)
    pub const FIXED_7F: u32 = 0x08;
    /// Battery/panel control
    pub const PANEL_CONTROL: u32 = 0x09;
    /// LCD enable
    pub const LCD_ENABLE: u32 = 0x0D;
    /// USB/general control
    pub const USB_CONTROL: u32 = 0x0F;
    /// Fixed value register (returns 0x80)
    pub const FIXED_80: u32 = 0x1C;
    /// Flash unlock status
    pub const FLASH_UNLOCK: u32 = 0x28;
    /// General control
    pub const GENERAL: u32 = 0x29;
}

/// CPU speed values
#[allow(dead_code)]
mod speed {
    pub const MHZ_6: u8 = 0x00;
    pub const MHZ_12: u8 = 0x01;
    pub const MHZ_24: u8 = 0x02;
    pub const MHZ_48: u8 = 0x03;
}

/// Control Port Controller
#[derive(Debug, Clone)]
pub struct ControlPorts {
    /// Power control register
    power: u8,
    /// CPU speed setting
    cpu_speed: u8,
    /// Battery status
    battery_status: u8,
    /// Device type flags
    device_type: u8,
    /// Control flags
    control_flags: u8,
    /// Unlock status
    unlock_status: u8,
    /// Battery configuration
    battery_config: u8,
    /// Panel control
    panel_control: u8,
    /// LCD enable
    lcd_enable: u8,
    /// USB control
    usb_control: u8,
    /// Flash unlock
    flash_unlock: u8,
    /// General control
    general: u8,
    /// Protected port start (3 bytes)
    protected_start: u32,
    /// Protected port end (3 bytes)
    protected_end: u32,
    /// Stack limit (3 bytes)
    stack_limit: u32,
}

impl ControlPorts {
    /// Create a new control port controller with default values
    pub fn new() -> Self {
        Self {
            power: 0x00,
            cpu_speed: speed::MHZ_48, // TI-84 CE runs at 48 MHz
            battery_status: 0x00,     // Battery charged/OK
            device_type: 0x00,        // Standard device
            control_flags: 0x00,
            unlock_status: 0x00,
            battery_config: 0x00,
            panel_control: 0x00,
            lcd_enable: 0x00,
            usb_control: 0x00,
            flash_unlock: 0x08,       // Bit 3 set = flash ready/unlocked
            general: 0x00,
            protected_start: 0,
            protected_end: 0,
            stack_limit: 0,
        }
    }

    /// Reset the control ports
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Read a control port byte
    /// addr is offset from 0xE00000 (0x00-0xFF)
    pub fn read(&self, addr: u32) -> u8 {
        match addr {
            regs::POWER => self.power,
            regs::CPU_SPEED => self.cpu_speed,
            regs::BATTERY_STATUS => self.battery_status,
            regs::DEVICE_TYPE => self.device_type,
            regs::CONTROL_FLAGS => self.control_flags,
            regs::UNLOCK_STATUS => self.unlock_status,
            regs::BATTERY_CONFIG => self.battery_config,
            regs::FIXED_7F => 0x7F, // Always returns 0x7F
            regs::PANEL_CONTROL => self.panel_control,
            regs::LCD_ENABLE => self.lcd_enable,
            regs::USB_CONTROL => self.usb_control,
            regs::FIXED_80 => 0x80, // Always returns 0x80
            regs::FLASH_UNLOCK => {
                // Bits 2 and 3 indicate flash unlocked/ready status
                // Bit 2 = unlocked, Bit 3 = hardware ready
                // On power-up, flash is unlocked and ready (0x0C)
                self.flash_unlock | 0x0C
            }
            regs::GENERAL => self.general,
            // Protected start address (3 bytes at 0x20-0x22)
            0x20 => self.protected_start as u8,
            0x21 => (self.protected_start >> 8) as u8,
            0x22 => (self.protected_start >> 16) as u8,
            // Protected end address (3 bytes at 0x23-0x25)
            0x23 => self.protected_end as u8,
            0x24 => (self.protected_end >> 8) as u8,
            0x25 => (self.protected_end >> 16) as u8,
            // Stack limit (3 bytes at 0x3A-0x3C)
            0x3A => self.stack_limit as u8,
            0x3B => (self.stack_limit >> 8) as u8,
            0x3C => (self.stack_limit >> 16) as u8,
            _ => 0x00,
        }
    }

    /// Write a control port byte
    /// addr is offset from 0xE00000 (0x00-0xFF)
    pub fn write(&mut self, addr: u32, value: u8) {
        match addr {
            regs::POWER => self.power = value,
            regs::CPU_SPEED => self.cpu_speed = value & 0x03,
            regs::BATTERY_STATUS => {} // Read-only
            regs::DEVICE_TYPE => {}    // Read-only
            regs::CONTROL_FLAGS => self.control_flags = value,
            regs::UNLOCK_STATUS => {
                // Only low 3 bits are writable
                self.unlock_status = value & 0x07;
                // If protected ports become locked, clear bit 3 of flash_unlock
                if !self.protected_ports_unlocked() {
                    self.flash_unlock &= !(1 << 3);
                }
            }
            regs::BATTERY_CONFIG => self.battery_config = value,
            regs::FIXED_7F => {} // Read-only
            regs::PANEL_CONTROL => self.panel_control = value,
            regs::LCD_ENABLE => self.lcd_enable = value,
            regs::USB_CONTROL => self.usb_control = value,
            regs::FIXED_80 => {} // Read-only
            regs::FLASH_UNLOCK => {
                // CEmu behavior: (current | 5) & value
                // This ORs in bits 0 and 2, then ANDs with written value
                // Bit 3 (flash ready) can only be cleared by this, never set
                self.flash_unlock = (self.flash_unlock | 5) & value;
            }
            regs::GENERAL => self.general = value,
            // Protected start address (3 bytes at 0x20-0x22)
            0x20 => self.protected_start = (self.protected_start & 0xFFFF00) | (value as u32),
            0x21 => {
                self.protected_start = (self.protected_start & 0xFF00FF) | ((value as u32) << 8)
            }
            0x22 => {
                self.protected_start = (self.protected_start & 0x00FFFF) | ((value as u32) << 16)
            }
            // Protected end address (3 bytes at 0x23-0x25)
            0x23 => self.protected_end = (self.protected_end & 0xFFFF00) | (value as u32),
            0x24 => self.protected_end = (self.protected_end & 0xFF00FF) | ((value as u32) << 8),
            0x25 => self.protected_end = (self.protected_end & 0x00FFFF) | ((value as u32) << 16),
            // Stack limit (3 bytes at 0x3A-0x3C)
            0x3A => self.stack_limit = (self.stack_limit & 0xFFFF00) | (value as u32),
            0x3B => self.stack_limit = (self.stack_limit & 0xFF00FF) | ((value as u32) << 8),
            0x3C => self.stack_limit = (self.stack_limit & 0x00FFFF) | ((value as u32) << 16),
            _ => {}
        }
    }

    /// Get current CPU speed setting
    pub fn cpu_speed(&self) -> u8 {
        self.cpu_speed
    }

    /// Check if LCD is enabled via control port
    pub fn lcd_enabled(&self) -> bool {
        self.lcd_enable != 0
    }

    /// Check if protected ports are unlocked (bit 2 of unlock_status)
    pub fn protected_ports_unlocked(&self) -> bool {
        (self.unlock_status & (1 << 2)) != 0
    }

    /// Check if flash is fully unlocked (bits 2 and 3 of flash_unlock)
    pub fn flash_unlocked(&self) -> bool {
        (self.flash_unlock & 0x0C) == 0x0C
    }
}

impl Default for ControlPorts {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let ctrl = ControlPorts::new();
        assert_eq!(ctrl.cpu_speed(), speed::MHZ_48);
    }

    #[test]
    fn test_fixed_values() {
        let ctrl = ControlPorts::new();
        assert_eq!(ctrl.read(regs::FIXED_7F), 0x7F);
        assert_eq!(ctrl.read(regs::FIXED_80), 0x80);
    }

    #[test]
    fn test_cpu_speed() {
        let mut ctrl = ControlPorts::new();
        ctrl.write(regs::CPU_SPEED, speed::MHZ_24);
        assert_eq!(ctrl.read(regs::CPU_SPEED), speed::MHZ_24);
        assert_eq!(ctrl.cpu_speed(), speed::MHZ_24);
    }

    #[test]
    fn test_protected_address() {
        let mut ctrl = ControlPorts::new();
        ctrl.write(0x20, 0x12);
        ctrl.write(0x21, 0x34);
        ctrl.write(0x22, 0x56);
        assert_eq!(ctrl.protected_start, 0x563412);
    }

    #[test]
    fn test_flash_unlock_bits23_always_set_on_read() {
        let mut ctrl = ControlPorts::new();
        // Initial state: bits 2 and 3 always set on read (flash unlocked and ready)
        assert_eq!(ctrl.read(regs::FLASH_UNLOCK), 0x0C);

        // Write 0x0C (bits 2 and 3) - stored value becomes 0x0C
        ctrl.write(regs::FLASH_UNLOCK, 0x0C);
        assert_eq!(ctrl.read(regs::FLASH_UNLOCK), 0x0C);
    }

    #[test]
    fn test_flash_unlock_hw_bits_override_stored() {
        let mut ctrl = ControlPorts::new();
        // Initial state: bits 2 and 3 always set on read
        assert_eq!(ctrl.read(regs::FLASH_UNLOCK), 0x0C);

        // Write 0x00 - stored value cleared but read still shows hw bits
        ctrl.write(regs::FLASH_UNLOCK, 0x00);
        // Stored: (0x08 | 5) & 0x00 = 0x00
        // Read: 0x00 | 0x0C = 0x0C (hw bits always present)
        assert_eq!(ctrl.read(regs::FLASH_UNLOCK), 0x0C);
    }

    #[test]
    fn test_port06_affects_flash_unlock() {
        let mut ctrl = ControlPorts::new();
        // First unlock protected ports (set bit 2)
        ctrl.write(regs::UNLOCK_STATUS, 0x04);
        assert!(ctrl.protected_ports_unlocked());
        // Read shows bits 2 and 3 (hardware ready)
        assert_eq!(ctrl.read(regs::FLASH_UNLOCK), 0x0C);

        // Lock protected ports (clear bit 2) - this clears stored bit 3
        ctrl.write(regs::UNLOCK_STATUS, 0x00);
        assert!(!ctrl.protected_ports_unlocked());
        // But read still shows hw bits 2 and 3
        assert_eq!(ctrl.read(regs::FLASH_UNLOCK), 0x0C);
    }
}
