//! TI-84 Plus CE Keypad Controller
//!
//! Memory-mapped at 0xF50000 (port offset 0x150000 from 0xE00000)
//!
//! The keypad is an 8x8 matrix. The controller scans rows and stores
//! the column data in data registers.

/// Number of keypad rows
pub const KEYPAD_ROWS: usize = 8;
/// Number of keypad columns
pub const KEYPAD_COLS: usize = 8;

/// Register offsets
mod regs {
    /// Control/mode register
    pub const CONTROL: u32 = 0x00;
    /// Matrix size configuration
    pub const SIZE: u32 = 0x04;
    /// Interrupt status
    pub const INT_STATUS: u32 = 0x08;
    /// Interrupt acknowledge/mask
    pub const INT_ACK: u32 = 0x0C;
    /// Row data registers (0x10-0x2F, 2 bytes per row)
    pub const DATA_BASE: u32 = 0x10;
}

/// Control register modes (kept for documentation/future use)
#[allow(dead_code)]
mod mode {
    /// Idle mode (no scanning)
    pub const IDLE: u8 = 0;
    /// Single scan mode
    pub const SINGLE: u8 = 1;
    /// Continuous scan with interrupt on any key
    pub const CONTINUOUS: u8 = 2;
    /// Multi-group scan mode
    pub const MULTI_GROUP: u8 = 3;
}

/// Keypad Controller
#[derive(Debug, Clone)]
pub struct KeypadController {
    /// Control/mode register
    control: u8,
    /// Matrix size configuration
    size: u8,
    /// Interrupt status
    int_status: u8,
    /// Interrupt mask
    int_mask: u8,
    /// Scan state: current row being scanned
    scan_row: usize,
    /// Whether any key was pressed during last scan
    any_key_pressed: bool,
}

impl KeypadController {
    /// Create a new keypad controller
    pub fn new() -> Self {
        Self {
            control: 0,
            size: 0x88, // 8 rows, 8 columns
            int_status: 0,
            int_mask: 0,
            scan_row: 0,
            any_key_pressed: false,
        }
    }

    /// Reset the keypad controller
    pub fn reset(&mut self) {
        self.control = 0;
        self.size = 0x88;
        self.int_status = 0;
        self.int_mask = 0;
        self.scan_row = 0;
        self.any_key_pressed = false;
    }

    /// Get the current scan mode
    fn mode(&self) -> u8 {
        self.control & 0x03
    }

    /// Check if keypad interrupt should fire
    /// Returns true if any key is pressed and interrupts are enabled
    pub fn check_interrupt(&self, key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> bool {
        if self.mode() != mode::CONTINUOUS {
            return false;
        }

        // Check if any key is pressed
        for row in key_state {
            for &pressed in row {
                if pressed {
                    return (self.int_mask & 0x04) != 0; // Bit 2 = any key interrupt
                }
            }
        }

        false
    }

    /// Read a register byte
    /// addr is offset from controller base (0-0x3F)
    /// key_state is the current keyboard matrix state
    pub fn read(&self, addr: u32, key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> u8 {
        match addr {
            regs::CONTROL => self.control,
            regs::SIZE => self.size,
            regs::INT_STATUS => self.int_status,
            regs::INT_ACK => self.int_mask,
            a if a >= regs::DATA_BASE && a < regs::DATA_BASE + 0x20 => {
                // Row data registers
                // Each row has 2 bytes (16 bits, though only 8 columns used)
                let row_offset = (a - regs::DATA_BASE) as usize;
                let row = row_offset / 2;
                let byte = row_offset % 2;

                if row < KEYPAD_ROWS {
                    let row_data = self.read_row(row, key_state);
                    if byte == 0 {
                        row_data as u8
                    } else {
                        (row_data >> 8) as u8
                    }
                } else {
                    0xFF
                }
            }
            _ => 0xFF,
        }
    }

    /// Read row data from key matrix
    /// Returns a bitmask where 0 = pressed, 1 = not pressed (active low)
    fn read_row(&self, row: usize, key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> u16 {
        let mut result = 0xFFFF_u16; // All keys released (active low)

        if row < KEYPAD_ROWS {
            for col in 0..KEYPAD_COLS {
                if key_state[row][col] {
                    result &= !(1 << col); // Key pressed = bit clear
                }
            }
        }

        result
    }

    /// Write a register byte
    /// addr is offset from controller base (0-0x3F)
    pub fn write(&mut self, addr: u32, value: u8) {
        match addr {
            regs::CONTROL => {
                self.control = value;
            }
            regs::SIZE => {
                self.size = value;
            }
            regs::INT_STATUS => {
                // Writing clears status bits
                self.int_status &= !value;
            }
            regs::INT_ACK => {
                self.int_mask = value;
            }
            // Data registers are read-only
            _ => {}
        }
    }
}

impl Default for KeypadController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_key_state() -> [[bool; KEYPAD_COLS]; KEYPAD_ROWS] {
        [[false; KEYPAD_COLS]; KEYPAD_ROWS]
    }

    #[test]
    fn test_new() {
        let kp = KeypadController::new();
        assert_eq!(kp.mode(), mode::IDLE);
        assert_eq!(kp.size, 0x88); // 8x8 matrix
    }

    #[test]
    fn test_reset() {
        let mut kp = KeypadController::new();
        kp.control = mode::CONTINUOUS;
        kp.int_mask = 0x04;
        kp.int_status = 0x01;

        kp.reset();
        assert_eq!(kp.mode(), mode::IDLE);
        assert_eq!(kp.int_mask, 0);
        assert_eq!(kp.int_status, 0);
        assert_eq!(kp.size, 0x88);
    }

    #[test]
    fn test_read_no_keys() {
        let kp = KeypadController::new();
        let keys = empty_key_state();

        // All rows should return 0xFF (no keys pressed)
        for row in 0..KEYPAD_ROWS {
            let data = kp.read(regs::DATA_BASE + (row as u32) * 2, &keys);
            assert_eq!(data, 0xFF, "Row {} should be 0xFF", row);
        }
    }

    #[test]
    fn test_read_key_pressed() {
        let kp = KeypadController::new();
        let mut keys = empty_key_state();

        // Press key at row 2, column 3
        keys[2][3] = true;

        // Row 2 should have bit 3 clear
        let data = kp.read(regs::DATA_BASE + 4, &keys);
        assert_eq!(data, 0xFF ^ (1 << 3), "Row 2 should have bit 3 clear");

        // Other rows should still be 0xFF
        let data = kp.read(regs::DATA_BASE, &keys);
        assert_eq!(data, 0xFF, "Row 0 should be 0xFF");
    }

    #[test]
    fn test_read_high_byte() {
        let kp = KeypadController::new();
        let mut keys = empty_key_state();

        // Press key - should only affect low byte
        keys[0][3] = true;

        // Low byte should have bit 3 clear
        let lo = kp.read(regs::DATA_BASE, &keys);
        assert_eq!(lo, 0xFF ^ (1 << 3));

        // High byte should be 0xFF (no keys in columns 8-15)
        let hi = kp.read(regs::DATA_BASE + 1, &keys);
        assert_eq!(hi, 0xFF);
    }

    #[test]
    fn test_multiple_keys() {
        let kp = KeypadController::new();
        let mut keys = empty_key_state();

        // Press multiple keys in row 0
        keys[0][0] = true;
        keys[0][2] = true;
        keys[0][5] = true;

        let data = kp.read(regs::DATA_BASE, &keys);
        let expected = 0xFF ^ (1 << 0) ^ (1 << 2) ^ (1 << 5);
        assert_eq!(data, expected as u8);
    }

    #[test]
    fn test_clear_int_status() {
        let mut kp = KeypadController::new();
        let keys = empty_key_state();

        kp.int_status = 0xFF;

        // Writing to INT_STATUS should clear those bits
        kp.write(regs::INT_STATUS, 0x05);
        assert_eq!(kp.read(regs::INT_STATUS, &keys), 0xFF & !0x05);
    }

    #[test]
    fn test_interrupt_check() {
        let mut kp = KeypadController::new();
        let mut keys = empty_key_state();

        // No keys, no interrupt
        assert!(!kp.check_interrupt(&keys));

        // Enable continuous mode and interrupt mask
        kp.control = mode::CONTINUOUS;
        kp.int_mask = 0x04;

        // Still no keys
        assert!(!kp.check_interrupt(&keys));

        // Press a key
        keys[0][0] = true;
        assert!(kp.check_interrupt(&keys));
    }

    #[test]
    fn test_interrupt_different_modes() {
        let mut kp = KeypadController::new();
        let mut keys = empty_key_state();
        keys[0][0] = true;
        kp.int_mask = 0x04; // Enable any key interrupt

        // IDLE mode - no interrupt
        kp.control = mode::IDLE;
        assert!(!kp.check_interrupt(&keys));

        // SINGLE mode - no interrupt
        kp.control = mode::SINGLE;
        assert!(!kp.check_interrupt(&keys));

        // CONTINUOUS mode - interrupt!
        kp.control = mode::CONTINUOUS;
        assert!(kp.check_interrupt(&keys));

        // MULTI_GROUP mode - no interrupt (only continuous triggers)
        kp.control = mode::MULTI_GROUP;
        assert!(!kp.check_interrupt(&keys));
    }

    #[test]
    fn test_read_out_of_range_row() {
        let kp = KeypadController::new();
        let keys = empty_key_state();

        // Rows beyond 7 should return 0xFF
        let data = kp.read(regs::DATA_BASE + 0x10, &keys); // Row 8
        assert_eq!(data, 0xFF);
    }

    #[test]
    fn test_read_unknown_register() {
        let kp = KeypadController::new();
        let keys = empty_key_state();

        // Unknown register should return 0xFF
        let data = kp.read(0x30, &keys);
        assert_eq!(data, 0xFF);
    }
}
