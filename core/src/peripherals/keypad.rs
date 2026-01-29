//! TI-84 Plus CE Keypad Controller
//!
//! Memory-mapped at 0xF50000 (port offset 0x150000 from 0xE00000)
//!
//! The keypad is an 8x8 matrix. The controller scans rows and stores
//! the column data in data registers.
//!
//! ## Scan Timing
//!
//! When scanning is initiated (modes 2 or 3), the controller scans one row
//! at a time with a configurable delay between rows. After all rows are
//! scanned, status bits are updated and the scan either repeats (continuous)
//! or stops (single scan).
//!
//! ## Status Bits (INT_STATUS register, offset 0x08)
//!
//! - Bit 0 (0x01): Scan complete - set when a full scan finishes
//! - Bit 1 (0x02): Data changed - set when key state differs from previous scan
//! - Bit 2 (0x04): Any key pressed - set when any key is detected during scan

/// Number of keypad rows
pub const KEYPAD_ROWS: usize = 8;
/// Number of keypad columns
pub const KEYPAD_COLS: usize = 8;

/// Default cycles between scanning each row (based on CEmu timing)
const DEFAULT_ROW_WAIT: u32 = 256;
/// Default cycles between complete scan cycles
const DEFAULT_SCAN_WAIT: u32 = 1024;

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
    /// Row wait (cycles between row scans)
    pub const ROW_WAIT: u32 = 0x30;
    /// Scan wait (cycles between complete scans)
    pub const SCAN_WAIT: u32 = 0x34;
}

/// Status register bits
mod status {
    /// Scan complete - set when a full scan finishes
    pub const SCAN_DONE: u8 = 0x01;
    /// Data changed - set when key state differs from previous scan
    pub const DATA_CHANGED: u8 = 0x02;
    /// Any key pressed - set when any key is detected during scan
    pub const ANY_KEY: u8 = 0x04;
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
    /// Whether a scan is currently in progress
    scanning: bool,
    /// Cycles until next row scan or scan completion
    scan_cycles_remaining: u32,
    /// Cycles between row scans
    row_wait: u32,
    /// Cycles between complete scan cycles
    scan_wait: u32,
    /// Previous scan results for detecting data changes
    prev_scan_data: [u16; KEYPAD_ROWS],
    /// Current scan results
    current_scan_data: [u16; KEYPAD_ROWS],
    /// Whether any key was detected during current scan
    any_key_in_scan: bool,
    /// Whether data changed during current scan
    data_changed_in_scan: bool,
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
            scanning: false,
            scan_cycles_remaining: 0,
            row_wait: DEFAULT_ROW_WAIT,
            scan_wait: DEFAULT_SCAN_WAIT,
            prev_scan_data: [0xFFFF; KEYPAD_ROWS],
            current_scan_data: [0xFFFF; KEYPAD_ROWS],
            any_key_in_scan: false,
            data_changed_in_scan: false,
        }
    }

    /// Reset the keypad controller
    pub fn reset(&mut self) {
        self.control = 0;
        self.size = 0x88;
        self.int_status = 0;
        self.int_mask = 0;
        self.scan_row = 0;
        self.scanning = false;
        self.scan_cycles_remaining = 0;
        self.row_wait = DEFAULT_ROW_WAIT;
        self.scan_wait = DEFAULT_SCAN_WAIT;
        self.prev_scan_data = [0xFFFF; KEYPAD_ROWS];
        self.current_scan_data = [0xFFFF; KEYPAD_ROWS];
        self.any_key_in_scan = false;
        self.data_changed_in_scan = false;
    }

    /// Get the current scan mode
    fn mode(&self) -> u8 {
        self.control & 0x03
    }

    /// Start a new scan cycle
    fn start_scan(&mut self) {
        self.scan_row = 0;
        self.scanning = true;
        self.scan_cycles_remaining = self.row_wait;
        self.any_key_in_scan = false;
        self.data_changed_in_scan = false;
    }

    /// Complete the current scan cycle
    fn finish_scan(&mut self) {
        // Set status bits based on scan results
        self.int_status |= status::SCAN_DONE;

        if self.data_changed_in_scan {
            self.int_status |= status::DATA_CHANGED;
        }

        if self.any_key_in_scan {
            self.int_status |= status::ANY_KEY;
        }

        // Save current scan data as previous for next comparison
        self.prev_scan_data = self.current_scan_data;

        // In continuous mode (2), restart the scan after scan_wait delay
        // In single scan mode (1) or multi-group (3), stop scanning
        if self.mode() == mode::CONTINUOUS {
            self.scan_row = 0;
            self.scan_cycles_remaining = self.scan_wait + self.row_wait;
            self.any_key_in_scan = false;
            self.data_changed_in_scan = false;
        } else {
            self.scanning = false;
        }
    }

    /// Advance the keypad controller by the given number of CPU cycles.
    /// This handles scan timing and status bit updates.
    /// Returns true if an interrupt should be raised.
    pub fn tick(&mut self, cycles: u32, key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> bool {
        if !self.scanning {
            return false;
        }

        let mut cycles_left = cycles;
        let mut interrupt_pending = false;

        while cycles_left > 0 && self.scanning {
            if cycles_left >= self.scan_cycles_remaining {
                cycles_left -= self.scan_cycles_remaining;
                self.scan_cycles_remaining = 0;

                // Scan the current row
                if self.scan_row < KEYPAD_ROWS {
                    let row_data = self.compute_row_data(self.scan_row, key_state);
                    self.current_scan_data[self.scan_row] = row_data;

                    // Check if any key is pressed in this row
                    if row_data != 0xFFFF {
                        self.any_key_in_scan = true;
                    }

                    // Check if data changed from previous scan
                    if row_data != self.prev_scan_data[self.scan_row] {
                        self.data_changed_in_scan = true;
                    }

                    self.scan_row += 1;

                    if self.scan_row >= KEYPAD_ROWS {
                        // Scan complete
                        self.finish_scan();
                        // Check if we should raise an interrupt
                        if (self.int_status & self.int_mask) != 0 {
                            interrupt_pending = true;
                        }
                    } else {
                        // Schedule next row
                        self.scan_cycles_remaining = self.row_wait;
                    }
                }
            } else {
                self.scan_cycles_remaining -= cycles_left;
                cycles_left = 0;
            }
        }

        interrupt_pending
    }

    /// Compute row data from key matrix
    /// Returns a bitmask where 0 = pressed, 1 = not pressed (active low)
    fn compute_row_data(&self, row: usize, key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> u16 {
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
    pub fn read(&mut self, addr: u32, _key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> u8 {
        match addr {
            regs::CONTROL => self.control,
            regs::SIZE => self.size,
            regs::INT_STATUS => {
                let status = self.int_status;
                // Reading status clears the status bits (auto-clear behavior)
                // This matches CEmu behavior where reading acknowledges the status
                self.int_status = 0;
                status
            }
            regs::INT_ACK => self.int_mask,
            a if a >= regs::DATA_BASE && a < regs::DATA_BASE + 0x20 => {
                // Row data registers
                // Each row has 2 bytes (16 bits, though only 8 columns used)
                let row_offset = (a - regs::DATA_BASE) as usize;
                let row = row_offset / 2;
                let byte = row_offset % 2;

                if row < KEYPAD_ROWS {
                    // Return the last scanned data, not live key state
                    let row_data = self.current_scan_data[row];
                    if byte == 0 {
                        row_data as u8
                    } else {
                        (row_data >> 8) as u8
                    }
                } else {
                    0xFF
                }
            }
            // Row wait register (32-bit, little-endian)
            a if a >= regs::ROW_WAIT && a < regs::ROW_WAIT + 4 => {
                let byte_offset = (a - regs::ROW_WAIT) as usize;
                (self.row_wait >> (byte_offset * 8)) as u8
            }
            // Scan wait register (32-bit, little-endian)
            a if a >= regs::SCAN_WAIT && a < regs::SCAN_WAIT + 4 => {
                let byte_offset = (a - regs::SCAN_WAIT) as usize;
                (self.scan_wait >> (byte_offset * 8)) as u8
            }
            _ => 0xFF,
        }
    }

    /// Read a register byte without side effects (for debugging/testing)
    /// addr is offset from controller base (0-0x3F)
    pub fn peek(&self, addr: u32, key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> u8 {
        match addr {
            regs::CONTROL => self.control,
            regs::SIZE => self.size,
            regs::INT_STATUS => self.int_status,
            regs::INT_ACK => self.int_mask,
            a if a >= regs::DATA_BASE && a < regs::DATA_BASE + 0x20 => {
                let row_offset = (a - regs::DATA_BASE) as usize;
                let row = row_offset / 2;
                let byte = row_offset % 2;

                if row < KEYPAD_ROWS {
                    // For peek, return live key state for compatibility
                    let row_data = self.compute_row_data(row, key_state);
                    if byte == 0 {
                        row_data as u8
                    } else {
                        (row_data >> 8) as u8
                    }
                } else {
                    0xFF
                }
            }
            a if a >= regs::ROW_WAIT && a < regs::ROW_WAIT + 4 => {
                let byte_offset = (a - regs::ROW_WAIT) as usize;
                (self.row_wait >> (byte_offset * 8)) as u8
            }
            a if a >= regs::SCAN_WAIT && a < regs::SCAN_WAIT + 4 => {
                let byte_offset = (a - regs::SCAN_WAIT) as usize;
                (self.scan_wait >> (byte_offset * 8)) as u8
            }
            _ => 0xFF,
        }
    }

    /// Write a register byte
    /// addr is offset from controller base (0-0x3F)
    pub fn write(&mut self, addr: u32, value: u8) {
        match addr {
            regs::CONTROL => {
                let old_mode = self.mode();
                self.control = value;
                let new_mode = self.mode();

                // If scanning mode is being enabled (modes 2 or 3), start a scan
                if (new_mode & 0x02) != 0 && (old_mode & 0x02) == 0 {
                    self.start_scan();
                } else if (new_mode & 0x02) == 0 {
                    // Scanning disabled
                    self.scanning = false;
                }
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
            // Row wait register (32-bit, little-endian)
            a if a >= regs::ROW_WAIT && a < regs::ROW_WAIT + 4 => {
                let byte_offset = (a - regs::ROW_WAIT) as usize;
                let mask = !(0xFF_u32 << (byte_offset * 8));
                self.row_wait = (self.row_wait & mask) | ((value as u32) << (byte_offset * 8));
            }
            // Scan wait register (32-bit, little-endian)
            a if a >= regs::SCAN_WAIT && a < regs::SCAN_WAIT + 4 => {
                let byte_offset = (a - regs::SCAN_WAIT) as usize;
                let mask = !(0xFF_u32 << (byte_offset * 8));
                self.scan_wait = (self.scan_wait & mask) | ((value as u32) << (byte_offset * 8));
            }
            // Data registers are read-only
            _ => {}
        }
    }

    /// Check if a scan is currently in progress
    pub fn is_scanning(&self) -> bool {
        self.scanning
    }

    /// Get the current status register value (without clearing)
    pub fn status(&self) -> u8 {
        self.int_status
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

    fn scan_keys(kp: &mut KeypadController, keys: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) {
        // Enable scanning and run enough cycles to capture a full scan.
        kp.write(regs::CONTROL, mode::CONTINUOUS);
        kp.tick(5000, keys);
    }

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
        let mut kp = KeypadController::new();
        let keys = empty_key_state();

        // All rows should return 0xFF (no keys pressed)
        scan_keys(&mut kp, &keys);
        for row in 0..KEYPAD_ROWS {
            let data = kp.read(regs::DATA_BASE + (row as u32) * 2, &keys);
            assert_eq!(data, 0xFF, "Row {} should be 0xFF", row);
        }
    }

    #[test]
    fn test_read_key_pressed() {
        let mut kp = KeypadController::new();
        let mut keys = empty_key_state();

        // Press key at row 2, column 3
        keys[2][3] = true;
        scan_keys(&mut kp, &keys);

        // Row 2 should have bit 3 clear
        let data = kp.read(regs::DATA_BASE + 4, &keys);
        assert_eq!(data, 0xFF ^ (1 << 3), "Row 2 should have bit 3 clear");

        // Other rows should still be 0xFF
        let data = kp.read(regs::DATA_BASE, &keys);
        assert_eq!(data, 0xFF, "Row 0 should be 0xFF");
    }

    #[test]
    fn test_read_high_byte() {
        let mut kp = KeypadController::new();
        let mut keys = empty_key_state();

        // Press key - should only affect low byte
        keys[0][3] = true;
        scan_keys(&mut kp, &keys);

        // Low byte should have bit 3 clear
        let lo = kp.read(regs::DATA_BASE, &keys);
        assert_eq!(lo, 0xFF ^ (1 << 3));

        // High byte should be 0xFF (no keys in columns 8-15)
        let hi = kp.read(regs::DATA_BASE + 1, &keys);
        assert_eq!(hi, 0xFF);
    }

    #[test]
    fn test_multiple_keys() {
        let mut kp = KeypadController::new();
        let mut keys = empty_key_state();

        // Press multiple keys in row 0
        keys[0][0] = true;
        keys[0][2] = true;
        keys[0][5] = true;
        scan_keys(&mut kp, &keys);

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
        let mut kp = KeypadController::new();
        let keys = empty_key_state();

        // Rows beyond 7 should return 0xFF
        scan_keys(&mut kp, &keys);
        let data = kp.read(regs::DATA_BASE + 0x10, &keys); // Row 8
        assert_eq!(data, 0xFF);
    }

    #[test]
    fn test_read_unknown_register() {
        let mut kp = KeypadController::new();
        let keys = empty_key_state();

        // Unknown register should return 0xFF
        let data = kp.read(0x3F, &keys);
        assert_eq!(data, 0xFF);
    }
}
