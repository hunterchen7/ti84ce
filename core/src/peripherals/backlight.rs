/// Backlight controller emulation for TI-84 Plus CE
///
/// Controls LCD backlight brightness via PWM. When brightness is 0,
/// the screen appears black even though LCD controller and VRAM remain powered.

#[derive(Debug, Clone)]
pub struct Backlight {
    /// Backlight brightness level (0x00 = off, 0xFF = full brightness)
    /// Register at offset 0x30
    brightness: u8,
}

impl Backlight {
    pub fn new() -> Self {
        Self {
            brightness: 0xFF, // Full brightness at power-on
        }
    }

    pub fn reset(&mut self) {
        self.brightness = 0xFF;
    }

    /// Read from backlight register
    pub fn read(&self, offset: u32) -> u8 {
        match offset {
            0x24 => self.brightness,
            _ => 0x00,
        }
    }

    /// Write to backlight register
    pub fn write(&mut self, offset: u32, value: u8) {
        match offset {
            0x21 | 0x22 | 0x25 | 0x26 => {
                // These registers turn off backlight when written
                if value != 0 {
                    let old = self.brightness;
                    self.brightness = 0;
                    if old != 0 {
                        crate::emu::log_event("BACKLIGHT: brightness OFF (via control register)");
                    }
                }
            }
            0x24 => {
                // Main brightness register
                let old = self.brightness;
                self.brightness = value;
                if old != value {
                    crate::emu::log_event(&format!(
                        "BACKLIGHT: brightness 0x{:02X} -> 0x{:02X} ({}%)",
                        old,
                        value,
                        (value as u32 * 100) / 255
                    ));
                }
            }
            _ => {}
        }
    }

    /// Get current brightness level (0-255)
    pub fn brightness(&self) -> u8 {
        self.brightness
    }

    /// Check if backlight is effectively off (brightness < 5%)
    pub fn is_off(&self) -> bool {
        self.brightness < 13 // < 5% brightness
    }
}
