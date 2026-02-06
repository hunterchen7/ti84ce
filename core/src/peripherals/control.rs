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
    /// Battery configuration (starts battery probe)
    pub const BATTERY_CONFIG: u32 = 0x07;
    /// Fixed value register (returns 0x7F)
    pub const FIXED_7F: u32 = 0x08;
    /// Battery/panel control
    pub const PANEL_CONTROL: u32 = 0x09;
    /// Battery check (advances FSM)
    pub const BATTERY_CHECK: u32 = 0x0A;
    /// Battery charging status
    pub const BATTERY_CHARGING: u32 = 0x0B;
    /// Battery reset (resets FSM)
    pub const BATTERY_RESET: u32 = 0x0C;
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

/// Battery status levels (CEmu control.h)
#[allow(dead_code)]
mod battery {
    pub const DISCHARGED: u8 = 0;
    pub const LEVEL_0: u8 = 1;
    pub const LEVEL_1: u8 = 2;
    pub const LEVEL_2: u8 = 3;
    pub const LEVEL_3: u8 = 4;
    pub const LEVEL_4: u8 = 5; // Full battery
}

/// Control Port Controller
#[derive(Debug, Clone)]
pub struct ControlPorts {
    /// Power control register
    power: u8,
    /// CPU speed setting (0=6MHz, 1=12MHz, 2=24MHz, 3=48MHz)
    cpu_speed: u8,
    /// Previous CPU clock rate in MHz, for cycle conversion
    /// CEmu's scheduler starts at 48MHz regardless of cpu_speed register value
    prev_clock_mhz: u32,
    /// Flag set when port 0x01 (CPU_SPEED) is written
    /// CEmu resets cycle counter on ANY write to this port, not just when value changes
    cpu_speed_written: bool,
    /// Device type flags
    device_type: u8,
    /// Control flags
    control_flags: u8,
    /// Unlock status
    unlock_status: u8,
    /// Battery configuration (port 0x07)
    battery_config: u8,
    /// Panel control (port 0x09)
    panel_control: u8,
    /// Battery check port (port 0x0A)
    battery_check: u8,
    /// Battery charging port (port 0x0B)
    battery_charging_port: u8,
    /// Battery reset port (port 0x0C)
    battery_reset: u8,
    /// LCD enable
    lcd_enable: u8,
    /// USB control
    usb_control: u8,
    /// Flash unlock
    flash_unlock: u8,
    /// General control
    general: u8,
    /// Privileged boundary (3 bytes at 0x1D-0x1F)
    /// Code with PC > privileged is considered unprivileged
    privileged: u32,
    /// Protected port start (3 bytes)
    protected_start: u32,
    /// Protected port end (3 bytes)
    protected_end: u32,
    /// Stack limit (3 bytes)
    stack_limit: u32,
    /// Battery FSM: current read status (returned on port 0x02 reads)
    /// Values: 0, 1, 3, 5, 7, 9, 11 indicate different FSM states
    read_battery_status: u8,
    /// Battery FSM: actual battery level (DISCHARGED through LEVEL_4)
    set_battery_status: u8,
    /// Battery charging flag
    battery_charging: bool,
    /// Protection status (NMI cause bits)
    /// Bit 0: stack limit violation, Bit 1: protected memory violation
    protection_status: u8,
}

impl ControlPorts {
    /// Create a new control port controller with default values
    /// Values match CEmu's control_reset() initialization
    pub fn new() -> Self {
        Self {
            power: 0x00,
            cpu_speed: speed::MHZ_6,  // CEmu defaults to 0, scheduler handles rate independently
            prev_clock_mhz: 48,  // CEmu scheduler starts at 48MHz
            cpu_speed_written: false,
            device_type: 0x00,        // Standard device
            control_flags: 0x00,
            unlock_status: 0x00,
            battery_config: 0x00,
            panel_control: 0x00,
            battery_check: 0x00,
            battery_charging_port: 0x00,
            battery_reset: 0x00,
            lcd_enable: 0x00,
            usb_control: 0x02,        // CEmu explicitly sets ports[0x0F] = 0x02
            flash_unlock: 0x00,       // Initially 0 (matches CEmu)
            general: 0x00,
            // CEmu: control.privileged = 0xFFFFFF (all code is privileged by default)
            privileged: 0xFFFFFF,
            // CEmu sets both protected boundaries to 0xD1887C (start=end means nothing protected)
            protected_start: 0xD1887C,
            protected_end: 0xD1887C,
            stack_limit: 0,
            // Battery FSM: CEmu sets setBatteryStatus = BATTERY_4, readBatteryStatus = 0
            read_battery_status: 0,
            set_battery_status: battery::LEVEL_4, // Full battery
            battery_charging: false,
            protection_status: 0,
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
            regs::POWER => {
                // CEmu returns the stored value directly (no forced bit)
                self.power
            }
            regs::CPU_SPEED => self.cpu_speed,
            regs::BATTERY_STATUS => {
                // For now, always return 0 which indicates battery probe complete
                // The FSM is complex and our implementation might not match CEmu exactly
                // TODO: Implement proper battery FSM if needed for specific ROM behavior
                0
            }
            regs::DEVICE_TYPE => self.device_type,
            regs::CONTROL_FLAGS => self.control_flags,
            regs::UNLOCK_STATUS => self.unlock_status,
            regs::BATTERY_CONFIG => self.battery_config,
            regs::FIXED_7F => 0x7F, // Always returns 0x7F
            regs::PANEL_CONTROL => self.panel_control,
            regs::BATTERY_CHECK => self.battery_check,
            regs::BATTERY_CHARGING => {
                // CEmu: control.ports[index] | control.batteryCharging << 1
                self.battery_charging_port | ((self.battery_charging as u8) << 1)
            }
            regs::BATTERY_RESET => self.battery_reset,
            regs::LCD_ENABLE => self.lcd_enable,
            regs::USB_CONTROL => {
                // CEmu ORs usb_status() into port 0x0F
                // CEmu's usb_status() returns 0x40 at reset (ROLE_D bit set in otgcsr = 0x00310E20)
                // Bit 7 (0x80) = VBUS/SESS valid (only set when USB cable connected)
                // Bit 6 (0x40) = DEV_B or ROLE_D (ROLE_D is set at reset)
                //
                // With 0x40: Boot fails at PC=0x13B3 (power-down HALT) - ROM thinks no power
                // With 0xC0: Boot succeeds - ROM thinks USB power is connected
                //
                // This suggests the TI-OS requires USB VBUS to be valid for boot without
                // battery power. CEmu may have USB cable "connected" by default in GUI,
                // or the actual hardware behavior differs.
                self.usb_control | 0xC0
            },
            regs::FIXED_80 => 0x80, // Always returns 0x80
            regs::FLASH_UNLOCK => {
                // CEmu: returns stored value directly
                // Bits: 0=unlock attempt, 2=unlocked, 3=flash ready
                self.flash_unlock
            }
            regs::GENERAL => self.general,
            // Privileged boundary (3 bytes at 0x1D-0x1F)
            0x1D => self.privileged as u8,
            0x1E => (self.privileged >> 8) as u8,
            0x1F => (self.privileged >> 16) as u8,
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
            // Protection status (read-only)
            0x3D => self.protection_status,
            _ => 0x00,
        }
    }

    /// Write a control port byte
    /// addr is offset from 0xE00000 (0x00-0xFF)
    pub fn write(&mut self, addr: u32, value: u8) {
        match addr {
            regs::POWER => {
                // Bit 4 is read-only (power stable indicator)
                // Only bits 0, 1, 7 are writable (0x93 mask per CEmu)
                let old = self.power;
                self.power = value & 0x93;
                // Log power register changes to detect APO (Auto Power Off)
                if old != self.power {
                    crate::emu::log_event(&format!(
                        "POWER register: 0x{:02X} -> 0x{:02X} (bit0={} bit1={} bit7={})",
                        old, self.power,
                        self.power & 1,
                        (self.power >> 1) & 1,
                        (self.power >> 7) & 1
                    ));
                }
                // Battery FSM transitions would go here if implemented
                // For now we skip the FSM to ensure boot completes
            }
            regs::CPU_SPEED => {
                // CEmu: control.ports[index] = byte & 19 (0x13)
                // Bits [1:0] = CPU speed, bit 4 = additional flag
                // Store full masked value; cpu_speed() extracts bits [1:0]
                self.cpu_speed = value & 0x13;
                // CEmu calls set_cpu_clock() on EVERY write to port 0x01,
                // which triggers cycle conversion
                self.cpu_speed_written = true;
            }
            regs::BATTERY_STATUS => {} // Read-only
            regs::DEVICE_TYPE => {}    // Read-only
            regs::CONTROL_FLAGS => {
                // CEmu masks control flags to 0x1F on write
                self.control_flags = value & 0x1F;
            }
            regs::UNLOCK_STATUS => {
                // Only low 3 bits are writable
                self.unlock_status = value & 0x07;
                // If protected ports become locked, clear bit 3 of flash_unlock
                if !self.protected_ports_unlocked() {
                    self.flash_unlock &= !(1 << 3);
                }
            }
            regs::BATTERY_CONFIG => {
                // CEmu: writing bit 4 or 7 starts battery probe
                // Skip FSM for now - just store value
                self.battery_config = value;
            }
            regs::FIXED_7F => {} // Read-only
            regs::PANEL_CONTROL => {
                // Skip battery FSM - just store value
                self.panel_control = value;
            }
            regs::BATTERY_CHECK => {
                // Skip battery FSM - just store value
                self.battery_check = value;
            }
            regs::BATTERY_CHARGING => {
                self.battery_charging_port = value;
            }
            regs::BATTERY_RESET => {
                // Skip battery FSM - just store value
                self.battery_reset = value;
            }
            regs::LCD_ENABLE => {
                // CEmu: control.ports[index] = (byte & 0xF) << 4 | (byte & 0xF)
                // Duplicates the low nibble into both nibbles
                let old = self.lcd_enable;
                self.lcd_enable = (value & 0x0F) << 4 | (value & 0x0F);
                // Log LCD enable/disable (bit 3 controls LCD on/off)
                if old != self.lcd_enable {
                    let lcd_enabled = (self.lcd_enable & (1 << 3)) != 0;
                    crate::emu::log_event(&format!(
                        "LCD_ENABLE: 0x{:02X} -> 0x{:02X} (LCD {})",
                        old, self.lcd_enable,
                        if lcd_enabled { "ON" } else { "OFF" }
                    ));
                }
            }
            regs::USB_CONTROL => {
                // CEmu: control.ports[index] = byte & 3
                self.usb_control = value & 0x03;
            }
            regs::FIXED_80 => {} // Read-only
            regs::FLASH_UNLOCK => {
                // CEmu behavior: (current | 5) & value
                // This ORs in bits 0 and 2, then ANDs with written value
                // Bit 3 (flash ready) can only be cleared by this, never set
                self.flash_unlock = (self.flash_unlock | 5) & value;
            }
            regs::GENERAL => self.general = value & 0x01, // CEmu: byte & 1
            // Privileged boundary (3 bytes at 0x1D-0x1F)
            0x1D => self.privileged = (self.privileged & 0xFFFF00) | (value as u32),
            0x1E => self.privileged = (self.privileged & 0xFF00FF) | ((value as u32) << 8),
            0x1F => self.privileged = (self.privileged & 0x00FFFF) | ((value as u32) << 16),
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
            // Clear protection status (write-1-to-clear)
            0x3E => self.protection_status &= !value,
            _ => {}
        }
    }

    /// Get current CPU speed setting (0=6MHz, 1=12MHz, 2=24MHz, 3=48MHz)
    /// Returns only bits [1:0] of the port value
    pub fn cpu_speed(&self) -> u8 {
        self.cpu_speed & 0x03
    }

    /// Check if CPU_SPEED port was written since last check, and reset the flag.
    /// Returns (was_written, new_rate, old_rate) for cycle conversion.
    /// CEmu's sched_set_clock converts: new_cycles = old_cycles * new_rate / old_rate
    /// - 48MHz -> 6MHz: new_cycles = old_cycles * 6 / 48 = old_cycles / 8
    /// - 6MHz -> 48MHz: new_cycles = old_cycles * 48 / 6 = old_cycles * 8
    pub fn cpu_speed_changed(&mut self) -> (bool, u32, u32) {
        if self.cpu_speed_written {
            self.cpu_speed_written = false;
            let new_mhz = self.clock_rate_mhz();
            let old_mhz = self.prev_clock_mhz;
            // Update prev_clock_mhz to new rate for next conversion
            // This ensures the scheduler tracks its actual rate, not the register value
            self.prev_clock_mhz = new_mhz;
            // Return (was_written, new_rate, old_rate) for conversion
            (true, new_mhz, old_mhz)
        } else {
            (false, 1, 1)
        }
    }

    /// Get current CPU clock rate in MHz based on speed setting
    pub fn clock_rate_mhz(&self) -> u32 {
        match self.cpu_speed() {
            0 => 6,
            1 => 12,
            2 => 24,
            _ => 48,
        }
    }

    /// Check if LCD is enabled via control port 0x0D
    pub fn lcd_enabled(&self) -> bool {
        self.lcd_enable != 0
    }

    /// Check if LCD enable bit is set in control flags (port 0x05 bit 4)
    /// This is one of two conditions CEmu checks for "LCD OFF"
    pub fn lcd_flag_enabled(&self) -> bool {
        self.control_flags & (1 << 4) != 0
    }

    /// Check if protected ports are unlocked (bit 2 of unlock_status)
    pub fn protected_ports_unlocked(&self) -> bool {
        (self.unlock_status & (1 << 2)) != 0
    }

    /// Check if flash is fully unlocked (bits 2 and 3 of flash_unlock)
    pub fn flash_unlocked(&self) -> bool {
        (self.flash_unlock & 0x0C) == 0x0C
    }

    /// Set the flash ready bit (bit 3 of flash_unlock)
    /// Called when the flash unlock sequence is detected during instruction fetch
    /// CEmu: control.flashUnlocked |= 1 << 3 - ONLY sets bit 3, not bit 2
    /// Bit 2 comes from the OUT0 (0x28), A instruction in the sequence
    pub fn set_flash_ready(&mut self) {
        // Only set bit 3 (flash ready), not bit 2
        // Bit 2 is set by the OUT0 (0x28), A in the unlock sequence itself
        self.flash_unlock |= 0x08;  // only bit 3
    }

    /// Clear the flash ready bit (bit 3 of flash_unlock)
    /// Called when unprivileged code fetches after flash unlock sequence
    pub fn clear_flash_ready(&mut self) {
        self.flash_unlock &= !(1 << 3);
    }

    /// Check if flash ready bit is set (bit 3)
    pub fn flash_ready(&self) -> bool {
        (self.flash_unlock & (1 << 3)) != 0
    }

    /// Read raw flash_unlock value (for debugging)
    pub fn read_flash_unlock(&self) -> u8 {
        self.flash_unlock
    }

    /// Get the privileged boundary address
    /// Code with PC > this value is considered unprivileged
    pub fn privileged_boundary(&self) -> u32 {
        self.privileged
    }

    /// Get the protected start address
    pub fn protected_start(&self) -> u32 {
        self.protected_start
    }

    /// Get the protected end address
    pub fn protected_end(&self) -> u32 {
        self.protected_end
    }

    /// Check if a given PC is running unprivileged code
    /// CEmu: unprivileged_code() in control.c
    /// Unprivileged means: PC > privileged AND (PC < protectedStart OR PC > protectedEnd)
    pub fn is_unprivileged(&self, pc: u32) -> bool {
        pc > self.privileged && (pc < self.protected_start || pc > self.protected_end)
    }

    /// Get the stack limit address
    pub fn stack_limit(&self) -> u32 {
        self.stack_limit
    }

    /// Set stack limit violation bit and return true if NMI should fire
    pub fn set_stack_violation(&mut self) -> bool {
        self.protection_status |= 1;
        true
    }

    /// Set protected memory violation bit and return true if NMI should fire
    pub fn set_protected_violation(&mut self) -> bool {
        self.protection_status |= 2;
        true
    }

    /// Dump all control port values for debugging/comparison with CEmu
    /// Returns a formatted string showing all port values
    pub fn dump(&self) -> String {
        let mut s = String::new();
        s.push_str("=== Control Ports (0xE000xx / 0xFF00xx) ===\n");
        s.push_str(&format!("0x00 POWER:          0x{:02X}\n", self.power));
        s.push_str(&format!("0x01 CPU_SPEED:      0x{:02X} ({}MHz)\n",
            self.cpu_speed,
            match self.cpu_speed { 0 => 6, 1 => 12, 2 => 24, 3 => 48, _ => 0 }));
        s.push_str(&format!("0x02 BATTERY_STATUS: 0x{:02X} (FSM state, setBattery={})\n",
            self.read_battery_status, self.set_battery_status));
        s.push_str(&format!("0x03 DEVICE_TYPE:    0x{:02X}\n", self.device_type));
        s.push_str(&format!("0x05 CONTROL_FLAGS:  0x{:02X}\n", self.control_flags));
        s.push_str(&format!("0x06 UNLOCK_STATUS:  0x{:02X} (protected_unlocked={})\n",
            self.unlock_status, self.protected_ports_unlocked()));
        s.push_str(&format!("0x07 BATTERY_CONFIG: 0x{:02X}\n", self.battery_config));
        s.push_str(&format!("0x08 FIXED_7F:       0x7F (always)\n"));
        s.push_str(&format!("0x09 PANEL_CONTROL:  0x{:02X}\n", self.panel_control));
        s.push_str(&format!("0x0A BATTERY_CHECK:  0x{:02X}\n", self.battery_check));
        s.push_str(&format!("0x0B BATTERY_CHARG:  0x{:02X} (charging={})\n",
            self.battery_charging_port, self.battery_charging));
        s.push_str(&format!("0x0C BATTERY_RESET:  0x{:02X}\n", self.battery_reset));
        s.push_str(&format!("0x0D LCD_ENABLE:     0x{:02X} (lcd_enabled={})\n",
            self.lcd_enable, self.lcd_enabled()));
        s.push_str(&format!("0x0F USB_CONTROL:    0x{:02X} (stored) -> 0x{:02X} (read with USB status)\n",
            self.usb_control, self.usb_control | 0xC0));
        s.push_str(&format!("0x1C FIXED_80:       0x80 (always)\n"));
        s.push_str(&format!("0x1D-1F PRIVILEGED:  0x{:06X}\n", self.privileged));
        s.push_str(&format!("0x20-22 PROT_START:  0x{:06X}\n", self.protected_start));
        s.push_str(&format!("0x23-25 PROT_END:    0x{:06X}\n", self.protected_end));
        s.push_str(&format!("0x28 FLASH_UNLOCK:   0x{:02X} (flash_unlocked={})\n",
            self.flash_unlock, self.flash_unlocked()));
        s.push_str(&format!("0x29 GENERAL:        0x{:02X}\n", self.general));
        s.push_str(&format!("0x3A-3C STACK_LIMIT: 0x{:06X}\n", self.stack_limit));
        s
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
        // CEmu's control.cpuSpeed defaults to 0 (6MHz), scheduler runs independently at 48MHz
        assert_eq!(ctrl.cpu_speed(), speed::MHZ_6);
        assert!(!ctrl.lcd_enabled());
        assert!(!ctrl.protected_ports_unlocked());
        // Battery FSM read status defaults to 0 (probe complete)
        assert_eq!(ctrl.read_battery_status, 0);
        // Privileged boundary defaults to 0xFFFFFF (all code is privileged)
        assert_eq!(ctrl.privileged, 0xFFFFFF);
        // Protected memory defaults to 0xD1887C (start=end)
        assert_eq!(ctrl.protected_start, 0xD1887C);
        assert_eq!(ctrl.protected_end, 0xD1887C);
    }

    #[test]
    fn test_reset() {
        let mut ctrl = ControlPorts::new();
        ctrl.write(regs::CPU_SPEED, speed::MHZ_48);
        ctrl.write(regs::POWER, 0x83); // Write allowed bits
        ctrl.write(0x20, 0x12);
        ctrl.write(0x1D, 0x00); // Change privileged boundary

        ctrl.reset();
        // CEmu resets cpu_speed to 0 (6MHz), scheduler handles rate independently
        assert_eq!(ctrl.cpu_speed(), speed::MHZ_6);
        // Power port resets to 0
        assert_eq!(ctrl.read(regs::POWER), 0x00);
        // CEmu sets privileged to 0xFFFFFF
        assert_eq!(ctrl.privileged, 0xFFFFFF);
        // CEmu sets protected_start to 0xD1887C
        assert_eq!(ctrl.protected_start, 0xD1887C);
        // Battery FSM read status is 0
        assert_eq!(ctrl.read_battery_status, 0);
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
    fn test_cpu_speed_masked() {
        let mut ctrl = ControlPorts::new();
        // Writing value > 3 should be masked
        ctrl.write(regs::CPU_SPEED, 0xFF);
        assert_eq!(ctrl.cpu_speed(), 0x03); // Only low 2 bits
    }

    #[test]
    fn test_battery_status_readonly() {
        let mut ctrl = ControlPorts::new();
        let initial = ctrl.read(regs::BATTERY_STATUS);
        ctrl.write(regs::BATTERY_STATUS, 0xFF);
        assert_eq!(ctrl.read(regs::BATTERY_STATUS), initial);
    }

    #[test]
    fn test_device_type_readonly() {
        let mut ctrl = ControlPorts::new();
        let initial = ctrl.read(regs::DEVICE_TYPE);
        ctrl.write(regs::DEVICE_TYPE, 0xFF);
        assert_eq!(ctrl.read(regs::DEVICE_TYPE), initial);
    }

    #[test]
    fn test_lcd_enable() {
        let mut ctrl = ControlPorts::new();
        assert!(!ctrl.lcd_enabled());

        ctrl.write(regs::LCD_ENABLE, 0x01);
        assert!(ctrl.lcd_enabled());
        // CEmu duplicates nibble: (0x01 & 0xF) << 4 | (0x01 & 0xF) = 0x11
        assert_eq!(ctrl.read(regs::LCD_ENABLE), 0x11);

        ctrl.write(regs::LCD_ENABLE, 0x08);
        // (0x08 & 0xF) << 4 | (0x08 & 0xF) = 0x88
        assert_eq!(ctrl.read(regs::LCD_ENABLE), 0x88);

        ctrl.write(regs::LCD_ENABLE, 0x00);
        assert!(!ctrl.lcd_enabled());
        assert_eq!(ctrl.read(regs::LCD_ENABLE), 0x00);
    }

    #[test]
    fn test_privileged_register() {
        let mut ctrl = ControlPorts::new();
        // Default is 0xFFFFFF (all code privileged)
        assert_eq!(ctrl.privileged_boundary(), 0xFFFFFF);
        assert_eq!(ctrl.read(0x1D), 0xFF);
        assert_eq!(ctrl.read(0x1E), 0xFF);
        assert_eq!(ctrl.read(0x1F), 0xFF);

        // Write to privileged register
        ctrl.write(0x1D, 0x00);
        ctrl.write(0x1E, 0x40);
        ctrl.write(0x1F, 0x00);
        assert_eq!(ctrl.privileged_boundary(), 0x004000);
        assert_eq!(ctrl.read(0x1D), 0x00);
        assert_eq!(ctrl.read(0x1E), 0x40);
        assert_eq!(ctrl.read(0x1F), 0x00);
    }

    #[test]
    fn test_is_unprivileged() {
        let mut ctrl = ControlPorts::new();
        // With default privileged=0xFFFFFF, no code is unprivileged
        assert!(!ctrl.is_unprivileged(0x000000));
        assert!(!ctrl.is_unprivileged(0xD00000));
        assert!(!ctrl.is_unprivileged(0xFFFFFF));

        // Set privileged boundary to 0x400000 (end of flash)
        ctrl.write(0x1D, 0x00);
        ctrl.write(0x1E, 0x00);
        ctrl.write(0x1F, 0x40);
        assert_eq!(ctrl.privileged_boundary(), 0x400000);

        // Code in flash (< 0x400000) is privileged
        assert!(!ctrl.is_unprivileged(0x001000));
        assert!(!ctrl.is_unprivileged(0x3FFFFF));

        // Code > privileged is unprivileged (unless in protected range)
        assert!(ctrl.is_unprivileged(0x400001));
        assert!(ctrl.is_unprivileged(0xD00000));

        // Code in protected range (0xD1887C-0xD1887C) is privileged
        // Since start=end, this is a single address
        assert!(!ctrl.is_unprivileged(0xD1887C));
    }

    #[test]
    fn test_usb_control_masked() {
        let mut ctrl = ControlPorts::new();
        // Initial value is 0x02, read ORs in 0xC0 (usb_status)
        // Note: CEmu's usb_status() returns 0x40 at reset, but we use 0xC0 for boot compatibility
        assert_eq!(ctrl.read(regs::USB_CONTROL), 0xC2);

        // Writing 0xFF should be masked to 0x03, read shows 0xC3 (with USB status)
        ctrl.write(regs::USB_CONTROL, 0xFF);
        assert_eq!(ctrl.read(regs::USB_CONTROL), 0xC3);

        // Writing 0x01, read shows 0xC1 (with USB status)
        ctrl.write(regs::USB_CONTROL, 0x01);
        assert_eq!(ctrl.read(regs::USB_CONTROL), 0xC1);
    }

    #[test]
    fn test_protected_address() {
        let mut ctrl = ControlPorts::new();
        ctrl.write(0x20, 0x12);
        ctrl.write(0x21, 0x34);
        ctrl.write(0x22, 0x56);
        assert_eq!(ctrl.protected_start, 0x563412);

        // Read back
        assert_eq!(ctrl.read(0x20), 0x12);
        assert_eq!(ctrl.read(0x21), 0x34);
        assert_eq!(ctrl.read(0x22), 0x56);
    }

    #[test]
    fn test_unlock_status() {
        let mut ctrl = ControlPorts::new();

        // Write and verify masking (only low 3 bits writable)
        ctrl.write(regs::UNLOCK_STATUS, 0xFF);
        assert_eq!(ctrl.read(regs::UNLOCK_STATUS), 0x07);

        // Test protected ports unlock
        ctrl.write(regs::UNLOCK_STATUS, 0x04);
        assert!(ctrl.protected_ports_unlocked());

        ctrl.write(regs::UNLOCK_STATUS, 0x00);
        assert!(!ctrl.protected_ports_unlocked());
    }

    #[test]
    fn test_flash_unlock_initial_zero() {
        let ctrl = ControlPorts::new();
        // Initial state: flash_unlock is 0 (matches CEmu)
        assert_eq!(ctrl.read(regs::FLASH_UNLOCK), 0x00);
    }

    #[test]
    fn test_flash_unlock_write_behavior() {
        let mut ctrl = ControlPorts::new();
        // CEmu write behavior: (current | 5) & value
        // This forces bits 0 and 2 on, then ANDs with written value

        // Write 0x0C: (0 | 5) & 0x0C = 5 & 0x0C = 4
        ctrl.write(regs::FLASH_UNLOCK, 0x0C);
        assert_eq!(ctrl.read(regs::FLASH_UNLOCK), 0x04);

        // Write 0x0F: (4 | 5) & 0x0F = 5 & 0x0F = 5
        ctrl.write(regs::FLASH_UNLOCK, 0x0F);
        assert_eq!(ctrl.read(regs::FLASH_UNLOCK), 0x05);

        // Write 0x00: (5 | 5) & 0x00 = 5 & 0 = 0
        ctrl.write(regs::FLASH_UNLOCK, 0x00);
        assert_eq!(ctrl.read(regs::FLASH_UNLOCK), 0x00);
    }

    #[test]
    fn test_port06_affects_flash_unlock() {
        let mut ctrl = ControlPorts::new();

        // Set up flash_unlock with bit 3
        ctrl.flash_unlock = 0x0C; // bits 2 and 3

        // Unlock protected ports (set bit 2 of port 0x06)
        ctrl.write(regs::UNLOCK_STATUS, 0x04);
        assert!(ctrl.protected_ports_unlocked());
        assert_eq!(ctrl.read(regs::FLASH_UNLOCK), 0x0C);

        // Lock protected ports (clear bit 2) - this clears bit 3 of flash_unlock
        ctrl.write(regs::UNLOCK_STATUS, 0x00);
        assert!(!ctrl.protected_ports_unlocked());
        // Bit 3 should now be cleared
        assert_eq!(ctrl.read(regs::FLASH_UNLOCK), 0x04);
    }

    #[test]
    fn test_unmapped_returns_zero() {
        let ctrl = ControlPorts::new();
        // Unmapped register should return 0
        assert_eq!(ctrl.read(0x10), 0x00);
        assert_eq!(ctrl.read(0x30), 0x00);
    }
}
