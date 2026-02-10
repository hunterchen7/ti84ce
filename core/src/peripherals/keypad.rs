//! TI-84 Plus CE Keypad Controller
//!
//! Memory-mapped at 0xF50000 (port offset 0x150000 from 0xE00000)
//!
//! The keypad is an 8x8 matrix. The controller scans rows and stores
//! the column data in data registers.
//!
//! ## Register Layout (CEmu parity)
//!
//! Uses packed 32-bit registers with byte-within-register addressing:
//! - Index 0x00 (offset 0x00-0x03): control — mode[1:0] | rowWait[15:2] | scanWait[31:16]
//! - Index 0x01 (offset 0x04-0x07): size — rows[7:0] | cols[15:8] | mask[31:16]
//! - Index 0x02 (offset 0x08-0x0B): status (read: status & enable; write: write-1-to-clear)
//! - Index 0x03 (offset 0x0C-0x0F): enable (writes masked to & 0x07)
//! - Index 0x04-0x0B (offset 0x10-0x2F): data[0..15] (16 rows x 2 bytes)
//! - Index 0x10 (offset 0x40-0x43): gpioEnable
//!
//! ## Scan Timing
//!
//! When scanning is initiated (modes 2 or 3), the controller scans one row
//! at a time with a configurable delay between rows. After all rows are
//! scanned, status bits are updated and the scan either repeats (modes 1/3)
//! or stops (mode 2 single scan).
//!
//! ## Status Bits (status register, index 0x02)
//!
//! - Bit 0 (0x01): Scan complete - set when a full scan finishes
//! - Bit 1 (0x02): Data changed - set when key state differs from previous scan
//! - Bit 2 (0x04): Any key pressed - set when any key is detected during scan

/// Number of physical keypad rows
pub const KEYPAD_ROWS: usize = 8;
/// Number of physical keypad columns
pub const KEYPAD_COLS: usize = 8;
/// Maximum rows supported by register layout
const KEYPAD_MAX_ROWS: usize = 16;

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
    /// Continuous scan mode (single scan, goes idle after)
    pub const CONTINUOUS: u8 = 2;
    /// Multi-group scan mode (repeating scan)
    pub const MULTI_GROUP: u8 = 3;
}

/// Register offsets (for documentation; actual addressing uses index-based scheme)
#[allow(dead_code)]
mod regs {
    /// Control/mode register (packed 32-bit)
    pub const CONTROL: u32 = 0x00;
    /// Matrix size configuration (packed 32-bit)
    pub const SIZE: u32 = 0x04;
    /// Interrupt status
    pub const INT_STATUS: u32 = 0x08;
    /// Interrupt enable
    pub const INT_ACK: u32 = 0x0C;
    /// Row data registers (0x10-0x2F, 2 bytes per row)
    pub const DATA_BASE: u32 = 0x10;
    /// GPIO enable register
    pub const GPIO_ENABLE: u32 = 0x40;
}

/// Keypad Controller
#[derive(Debug, Clone)]
pub struct KeypadController {
    /// Packed control register: mode[1:0] | rowWait[15:2] | scanWait[31:16]
    control: u32,
    /// Packed size register: rows[7:0] | cols[15:8] | mask[31:16]
    size: u32,
    /// Interrupt status
    status: u8,
    /// Interrupt enable (masked to 0x07 on writes)
    enable: u8,
    /// Current scan row (CEmu: keypad.row)
    scan_row: u8,
    /// Data registers (16 rows x 16-bit)
    data: [u16; KEYPAD_MAX_ROWS],
    /// GPIO enable register
    gpio_enable: u32,
    /// Whether a scan is currently in progress
    scanning: bool,
    /// Cycles until next row scan or scan completion
    scan_cycles_remaining: u32,
    /// Previous scan results for detecting data changes
    prev_scan_data: [u16; KEYPAD_MAX_ROWS],
    /// Whether any key was detected during current scan
    any_key_in_scan: bool,
    /// Whether data changed during current scan
    data_changed_in_scan: bool,
    /// Flag: any_key_check needs to be called (set by write, cleared by caller)
    pub needs_any_key_check: bool,
    /// Edge flags for key presses (CEmu's "edge" bit mechanism)
    /// Set when key is pressed, cleared when queried by any_key_check
    /// This allows detecting quick press/release even if released before query
    key_edge_flags: [[bool; KEYPAD_COLS]; KEYPAD_ROWS],
}

impl KeypadController {
    /// Create a new keypad controller
    pub fn new() -> Self {
        Self {
            control: 0,
            // rows=8 (byte 0), cols=8 (byte 1), mask=0xFFFF (bytes 2-3)
            size: Self::pack_size(8, 8, 0xFFFF),
            status: 0,
            enable: 0,
            scan_row: 0,
            data: [0x0000; KEYPAD_MAX_ROWS],
            gpio_enable: 0,
            scanning: false,
            scan_cycles_remaining: 0,
            prev_scan_data: [0x0000; KEYPAD_MAX_ROWS],
            any_key_in_scan: false,
            data_changed_in_scan: false,
            needs_any_key_check: false,
            key_edge_flags: [[false; KEYPAD_COLS]; KEYPAD_ROWS],
        }
    }

    /// Reset the keypad controller
    pub fn reset(&mut self) {
        self.control = 0;
        // CEmu reset: keypad.mask = 0xFFFF, row = 0
        // rows and cols are memset to 0 in CEmu init (not explicitly set in reset)
        // But we keep rows=8, cols=8 as reasonable defaults that match init
        self.size = Self::pack_size(8, 8, 0xFFFF);
        self.status = 0;
        self.enable = 0;
        self.scan_row = 0;
        self.data = [0x0000; KEYPAD_MAX_ROWS];
        self.gpio_enable = 0;
        self.scanning = false;
        self.scan_cycles_remaining = 0;
        self.prev_scan_data = [0x0000; KEYPAD_MAX_ROWS];
        self.any_key_in_scan = false;
        self.data_changed_in_scan = false;
        self.needs_any_key_check = false;
        self.key_edge_flags = [[false; KEYPAD_COLS]; KEYPAD_ROWS];
    }

    // ========== Packed field accessors ==========

    /// Get current scan mode (bits 1:0 of control)
    pub fn mode(&self) -> u8 {
        (self.control & 0x03) as u8
    }

    /// Set scan mode (bits 1:0 of control)
    fn set_mode(&mut self, m: u8) {
        self.control = (self.control & !0x03) | (m as u32 & 0x03);
    }

    /// Get row wait cycles (bits 15:2 of control)
    fn row_wait(&self) -> u32 {
        (self.control >> 2) & 0x3FFF
    }

    /// Get scan wait cycles (bits 31:16 of control)
    fn scan_wait(&self) -> u32 {
        (self.control >> 16) & 0xFFFF
    }

    /// Get number of rows (bits 7:0 of size)
    fn rows(&self) -> u8 {
        (self.size & 0xFF) as u8
    }

    /// Get number of columns (bits 15:8 of size)
    fn cols(&self) -> u8 {
        ((self.size >> 8) & 0xFF) as u8
    }

    /// Get row mask (bits 31:16 of size)
    fn mask(&self) -> u16 {
        ((self.size >> 16) & 0xFFFF) as u16
    }

    /// Pack size register from components
    fn pack_size(rows: u8, cols: u8, mask: u16) -> u32 {
        (rows as u32) | ((cols as u32) << 8) | ((mask as u32) << 16)
    }

    // ========== Key edge handling ==========

    /// Update key edge flag (called when key state changes)
    /// When pressed=true, sets edge flag (will be seen by next query)
    /// When pressed=false, does NOT clear edge (CEmu behavior)
    /// Edge flags are cleared only by query_row_data()
    ///
    /// Also immediately updates data so the key is visible
    /// in data register reads regardless of keypad mode. This is critical
    /// for TI-OS key detection which may not switch to mode 1 before reading.
    ///
    /// IMPORTANT: On key release, we do NOT clear data!
    /// The data should persist until the OS reads and processes it.
    /// CEmu's edge detection works the same way - keymap_edge preserves
    /// the key press until it's queried.
    pub fn set_key_edge(&mut self, row: usize, col: usize, pressed: bool) {
        if row < KEYPAD_ROWS && col < KEYPAD_COLS {
            // Skip ON key (row 2, col 0) - CEmu stores it separately from keyMap
            // The ON key has its own interrupt handling (INT_ON) and doesn't
            // participate in normal keypad scanning/any_key_check
            if row == 2 && col == 0 {
                return;
            }
            if pressed {
                // Set edge on press, not on release (CEmu behavior)
                self.key_edge_flags[row][col] = true;

                // Immediately update data so the key is visible
                // This is needed because any_key_check only runs in mode 1,
                // but TI-OS might read data registers in mode 0.
                self.data[row] |= 1 << col;

                // Set status flags
                self.status |= status::DATA_CHANGED | status::ANY_KEY;
            } else {
                // CEmu clears the keyMap bit on release:
                // keyMap[row] &= ~(1 << col)
                // The edge flag persists for detection, but the data should reflect
                // current key state.
                self.data[row] &= !(1 << col);
            }
        }
    }

    // ========== Scan logic ==========

    /// Start a new scan cycle
    fn start_scan(&mut self) {
        self.scan_row = 0;
        self.scanning = true;
        self.scan_cycles_remaining = self.row_wait();
        self.any_key_in_scan = false;
        self.data_changed_in_scan = false;
    }

    /// Complete the current scan cycle
    /// Matches CEmu's keypad_scan_event completion logic:
    /// - Sets status bit 0 (scan done) always
    /// - If mode & 1 (modes 1 or 3): restart scanning with scanWait + rowWait + 2
    /// - If mode & 1 == 0 (mode 2): go to idle (set mode = 0)
    fn finish_scan(&mut self) {
        // Set scan complete status
        self.status |= status::SCAN_DONE;

        if self.data_changed_in_scan {
            self.status |= status::DATA_CHANGED;
        }

        if self.any_key_in_scan {
            self.status |= status::ANY_KEY;
        }

        // Scan done logging removed to reduce log noise

        // Save current data as previous for next comparison
        self.prev_scan_data = self.data;

        // CEmu: if (keypad.mode & 1) — modes 1 and 3 keep scanning
        if self.mode() & 1 != 0 {
            self.scan_row = 0;
            self.scan_cycles_remaining = 2 + self.scan_wait() + self.row_wait();
            self.any_key_in_scan = false;
            self.data_changed_in_scan = false;
        } else {
            // Mode 2 (continuous but bit 0 clear): go to idle after single scan
            self.set_mode(mode::IDLE);
            self.scanning = false;
        }
    }

    /// Row limit for register iteration (capped at KEYPAD_MAX_ROWS)
    fn row_limit(&self) -> usize {
        let r = self.rows() as usize;
        if r >= KEYPAD_MAX_ROWS { KEYPAD_MAX_ROWS } else { r }
    }

    /// Row limit for physical row access (capped at KEYPAD_ROWS)
    fn actual_row_limit(&self) -> usize {
        let r = self.rows() as usize;
        if r >= KEYPAD_ROWS { KEYPAD_ROWS } else { r }
    }

    /// Data mask based on column count
    fn data_mask(&self) -> u16 {
        let col_limit = std::cmp::min(self.cols() as usize, KEYPAD_COLS);
        (1u16 << col_limit) - 1
    }

    /// Advance the keypad controller by the given number of CPU cycles.
    /// This handles scan timing and status bit updates.
    /// Returns true if an interrupt should be raised.
    pub fn tick(&mut self, cycles: u32, key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> bool {
        if !self.scanning {
            return false;
        }

        // Scan activity logging removed — was generating 9000+ messages per session

        let mut cycles_left = cycles;
        let mut interrupt_pending = false;

        while cycles_left > 0 && self.scanning {
            if cycles_left >= self.scan_cycles_remaining {
                cycles_left -= self.scan_cycles_remaining;
                self.scan_cycles_remaining = 0;

                let row = self.scan_row as usize;
                let row_limit = self.row_limit();

                // Scan the current row
                if row < row_limit {
                    let mut row_data: u16 = 0;
                    if row < KEYPAD_ROWS {
                        // Use query_row_data for edge detection (CEmu: keypad_query_keymap)
                        row_data = self.query_row_data(row, key_state) & self.data_mask();
                    }

                    // Check if data changed from previous scan
                    if self.data[row] != row_data {
                        self.status |= status::DATA_CHANGED;
                        self.data[row] = row_data;
                    }

                    // Check if any key is pressed in this row
                    if row_data != 0 {
                        self.any_key_in_scan = true;
                        // Key scan logging removed to reduce log noise
                    }

                    // Check if data changed from previous scan cycle
                    if row_data != self.prev_scan_data[row] {
                        self.data_changed_in_scan = true;
                    }
                }

                self.scan_row += 1;

                if (self.scan_row as usize) < self.rows() as usize {
                    // Schedule next row
                    self.scan_cycles_remaining = self.row_wait();
                } else {
                    // Scan complete
                    self.finish_scan();
                    // Check if we should raise an interrupt
                    if (self.status & self.enable) != 0 {
                        interrupt_pending = true;
                    }
                }
            } else {
                self.scan_cycles_remaining -= cycles_left;
                cycles_left = 0;
            }
        }

        interrupt_pending
    }

    /// Query row data (destructive - clears edge flags after reading)
    /// Returns current state OR edge flags, then clears edge flags.
    /// This matches CEmu's keypad_query_keymap() behavior.
    /// Edge flags are set when key is pressed, preserved through release,
    /// and only cleared here. This allows detecting quick press/release.
    fn query_row_data(&mut self, row: usize, key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> u16 {
        let mut result = 0x0000_u16;

        if row < KEYPAD_ROWS {
            for col in 0..KEYPAD_COLS {
                // Skip ON key (row 2, col 0) - CEmu stores it separately from keyMap
                // The ON key has its own interrupt handling and doesn't participate
                // in normal keypad scanning/any_key_check
                if row == 2 && col == 0 {
                    continue;
                }
                // Combine current state with edge flag (CEmu: data | data >> 8)
                if key_state[row][col] || self.key_edge_flags[row][col] {
                    result |= 1 << col;
                }
                // Clear edge flag after query (CEmu: fetch_and with 0xFF mask)
                self.key_edge_flags[row][col] = false;
            }
        }

        result
    }

    /// Check if keypad interrupt should fire based on mode and key state
    /// Returns true in mode 1 (any-key mode) or mode 2 (continuous) when any key is pressed
    /// Note: CPU wake is handled separately via the any_key_wake signal
    pub fn check_interrupt(&self, key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> bool {
        // CEmu's keypad_any_check() runs when mode == 1 (any-key detection mode)
        // Mode 2 (continuous) also generates interrupts via the scan mechanism
        // The TI-OS typically uses mode 1 for key detection
        let m = self.mode();
        if m != mode::SINGLE && m != mode::CONTINUOUS {
            return false;
        }

        // Check if any key is pressed (excluding ON key which has its own handling)
        for (row_idx, row) in key_state.iter().enumerate() {
            for (col_idx, &pressed) in row.iter().enumerate() {
                // Skip ON key (row 2, col 0) - handled separately
                if row_idx == 2 && col_idx == 0 {
                    continue;
                }
                if pressed {
                    return true;
                }
            }
        }

        false
    }

    // ========== Register read/write ==========

    /// Read a register byte
    /// addr is offset from controller base (0x00-0x4F)
    /// key_state is the current keyboard matrix state
    pub fn read(&mut self, addr: u32, _key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> u8 {
        // Beyond implemented registers (0x00-0x47), return 0
        if addr >= 0x48 {
            return 0;
        }
        let index = (addr >> 2) & 0x7F;
        let bit_offset = (addr & 3) * 8;

        match index {
            // control (packed 32-bit)
            0x00 => ((self.control >> bit_offset) & 0xFF) as u8,
            // size (packed 32-bit)
            0x01 => ((self.size >> bit_offset) & 0xFF) as u8,
            // status: reads return (status & enable) per CEmu
            0x02 => (((self.status as u32 & self.enable as u32) >> bit_offset) & 0xFF) as u8,
            // enable
            0x03 => ((self.enable as u32 >> bit_offset) & 0xFF) as u8,
            // data[0..15] — index 0x04-0x0B covers 16 rows x 2 bytes
            0x04..=0x0B => {
                let data_idx = ((addr.wrapping_sub(0x10)) >> 1) & 0x0F;
                let byte_sel = (addr & 1) * 8;
                ((self.data[data_idx as usize] >> byte_sel) & 0xFF) as u8
            }
            // gpioEnable (32-bit)
            0x10 => ((self.gpio_enable >> bit_offset) & 0xFF) as u8,
            // GPIO status is always 0
            0x11 => 0,
            _ => 0,
        }
    }

    /// Read a register byte without side effects (for debugging/testing)
    /// addr is offset from controller base (0x00-0x4F)
    pub fn peek(&self, addr: u32, _key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> u8 {
        let index = (addr >> 2) & 0x7F;
        let bit_offset = (addr & 3) * 8;

        match index {
            0x00 => ((self.control >> bit_offset) & 0xFF) as u8,
            0x01 => ((self.size >> bit_offset) & 0xFF) as u8,
            0x02 => (((self.status as u32 & self.enable as u32) >> bit_offset) & 0xFF) as u8,
            0x03 => ((self.enable as u32 >> bit_offset) & 0xFF) as u8,
            0x04..=0x0B => {
                let data_idx = ((addr.wrapping_sub(0x10)) >> 1) & 0x0F;
                let byte_sel = (addr & 1) * 8;
                ((self.data[data_idx as usize] >> byte_sel) & 0xFF) as u8
            }
            0x10 => ((self.gpio_enable >> bit_offset) & 0xFF) as u8,
            0x11 => 0,
            _ => 0,
        }
    }

    /// Write a register byte
    /// addr is offset from controller base (0x00-0x4F)
    pub fn write(&mut self, addr: u32, value: u8) {
        let index = (addr >> 2) & 0x7F;
        let bit_offset = (addr & 3) * 8;

        match index {
            // control — write byte into packed 32-bit, then handle mode change
            0x00 => {
                let mask = !(0xFF_u32 << bit_offset);
                self.control = (self.control & mask) | ((value as u32) << bit_offset);

                // CEmu: if (mode & 2) start scanning, else call any_key_check
                if self.mode() & 2 != 0 {
                    // Mode 2 or 3: start scheduled scanning
                    self.start_scan();
                } else {
                    // Mode 0 or 1: stop scanning and do immediate key check
                    self.scanning = false;
                    self.needs_any_key_check = true;
                }
            }
            // size — write byte into packed 32-bit
            0x01 => {
                let mask = !(0xFF_u32 << bit_offset);
                self.size = (self.size & mask) | ((value as u32) << bit_offset);
                // CEmu calls keypad_any_check() after SIZE write
                self.needs_any_key_check = true;
            }
            // status — write-1-to-clear (CEmu: write8(status, bit_offset, status >> bit_offset & ~byte))
            0x02 => {
                // CEmu's write-1-to-clear: the byte at bit_offset is replaced with
                // (old_byte & ~value), i.e. clear bits that are set in value.
                // Status is u8, so only byte 0 (bit_offset == 0) is meaningful.
                if bit_offset == 0 {
                    self.status &= !value;
                }
                // CEmu calls keypad_any_check() and keypad_intrpt_check() after clearing status
                self.needs_any_key_check = true;
            }
            // enable — masked to 0x07
            0x03 => {
                if bit_offset == 0 {
                    self.enable = value & 0x07;
                }
                // CEmu calls keypad_intrpt_check() but not any_key_check
                // We don't need the flag here since intrpt_check is handled by mod.rs
            }
            // data registers are read-only (unless poke, which we don't support here)
            0x04..=0x0B => {}
            // gpioEnable
            0x10 => {
                let mask = !(0xFF_u32 << bit_offset);
                self.gpio_enable = (self.gpio_enable & mask) | ((value as u32) << bit_offset);
            }
            // GPIO status is always 0, no bits to reset
            0x11 => {}
            _ => {}
        }
    }

    /// Check if a scan is currently in progress
    pub fn is_scanning(&self) -> bool {
        self.scanning
    }

    /// Get the current status register value (without clearing)
    pub fn status(&self) -> u8 {
        self.status
    }

    /// Immediate key check - called when a key is pressed to update data registers
    /// Matches CEmu's keypad_any_check() function behavior:
    /// - Only runs in mode 1 (any-key mode)
    /// - Queries all rows in the mask and ORs them together (using edge detection)
    /// - Stores the combined result in ALL data registers
    /// Returns true if an interrupt should be raised
    pub fn any_key_check(&mut self, key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> bool {
        let current_mode = self.mode();
        // CEmu: if (keypad.mode != 1) return;
        // Only run in mode 1 (any-key detection mode)
        if current_mode != mode::SINGLE {
            return false;
        }

        // Compute combined key data from all rows in the mask
        // Uses query_row_data which includes edge flags and clears them
        let mut any: u16 = 0;
        let row_limit = self.actual_row_limit();
        let mask = self.mask();

        for row in 0..row_limit {
            // Only query rows that are enabled in the mask
            if (mask & (1 << row)) != 0 {
                // Use query_row_data for edge detection (CEmu: keypad_query_keymap)
                any |= self.query_row_data(row, key_state);
            }
        }

        // Apply column mask (data_mask in CEmu)
        let data_mask = self.data_mask();
        any &= data_mask;

        if any != 0 {
            crate::emu::log_evt!("ANY_KEY_CHECK: any=0x{:04X} mask=0x{:04X} status=0x{:02X}",
                any, mask, self.status);
        }

        // CEmu: Store combined 'any' in ALL rows that are in the mask
        // This is the critical behavior for TI-OS key detection!
        let row_limit_full = self.row_limit();
        for row in 0..row_limit_full {
            if (mask & (1 << row)) != 0 {
                // Check if data changed
                if self.data[row] != any {
                    self.status |= status::DATA_CHANGED;
                }
                self.data[row] = any;
            }
        }

        // Set any-key status if keys are pressed (CEmu: if (any & mask))
        if any != 0 {
            self.status |= status::ANY_KEY;
        }

        // Return true if interrupt should fire (status & enable)
        (self.status & self.enable) != 0
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
        // Enable scanning in mode 2|1=3 (repeating scan) and run enough cycles.
        kp.write(regs::CONTROL, mode::MULTI_GROUP);
        kp.tick(5000, keys);
    }

    fn empty_key_state() -> [[bool; KEYPAD_COLS]; KEYPAD_ROWS] {
        [[false; KEYPAD_COLS]; KEYPAD_ROWS]
    }

    /// Set up mode 1 (any-key) and update data registers with key state.
    /// This matches how TI-OS uses the keypad for key detection.
    fn update_keys(kp: &mut KeypadController, keys: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) {
        kp.write(regs::CONTROL, mode::SINGLE);
        kp.any_key_check(keys);
    }

    #[test]
    fn test_new() {
        let kp = KeypadController::new();
        assert_eq!(kp.mode(), mode::IDLE);
        assert_eq!(kp.rows(), 8); // 8 rows
        assert_eq!(kp.cols(), 8); // 8 columns
        assert_eq!(kp.mask(), 0xFFFF); // CEmu default: all rows enabled
    }

    #[test]
    fn test_reset() {
        let mut kp = KeypadController::new();
        // Set some state via writes
        kp.write(regs::CONTROL, mode::MULTI_GROUP);
        kp.enable = 0x04;
        kp.status = 0x01;

        kp.reset();
        assert_eq!(kp.mode(), mode::IDLE);
        assert_eq!(kp.enable, 0);
        assert_eq!(kp.status, 0);
        assert_eq!(kp.rows(), 8);
        assert_eq!(kp.cols(), 8);
        assert_eq!(kp.mask(), 0xFFFF);
    }

    #[test]
    fn test_read_no_keys() {
        let mut kp = KeypadController::new();
        let keys = empty_key_state();

        // All rows should return 0x00 (no keys pressed, active-high convention)
        for row in 0..KEYPAD_ROWS {
            let data = kp.read(regs::DATA_BASE + (row as u32) * 2, &keys);
            assert_eq!(data, 0x00, "Row {} should be 0x00", row);
        }
    }

    #[test]
    fn test_read_key_pressed() {
        let mut kp = KeypadController::new();
        let mut keys = empty_key_state();

        // Press key at row 2, column 3
        keys[2][3] = true;

        // Use mode 1 (any-key) and update data registers (like TI-OS does)
        update_keys(&mut kp, &keys);

        // In mode 1, all rows contain the combined key data (CEmu behavior)
        // Bit 3 should be set since we pressed (2, 3)
        let data = kp.read(regs::DATA_BASE + 4, &keys);
        assert_eq!(data, 1 << 3, "Row 2 should have bit 3 set");

        // In mode 1, ALL rows contain the same combined data
        // This is how CEmu's any_key_check works - it stores 'any' in all rows
        let data = kp.read(regs::DATA_BASE, &keys);
        assert_eq!(data, 1 << 3, "Row 0 should also have the combined data");
    }

    #[test]
    fn test_read_high_byte() {
        let mut kp = KeypadController::new();
        let mut keys = empty_key_state();

        // Press key - should only affect low byte
        keys[0][3] = true;

        // Use mode 1 and update data registers
        update_keys(&mut kp, &keys);

        // Low byte should have bit 3 set (active-high)
        let lo = kp.read(regs::DATA_BASE, &keys);
        assert_eq!(lo, 1 << 3);

        // High byte should be 0x00 (no keys in columns 8-15)
        let hi = kp.read(regs::DATA_BASE + 1, &keys);
        assert_eq!(hi, 0x00);
    }

    #[test]
    fn test_multiple_keys() {
        let mut kp = KeypadController::new();
        let mut keys = empty_key_state();

        // Press multiple keys in row 0
        keys[0][0] = true;
        keys[0][2] = true;
        keys[0][5] = true;

        // Use mode 1 and update data registers
        update_keys(&mut kp, &keys);

        let data = kp.read(regs::DATA_BASE, &keys);
        let expected = (1 << 0) | (1 << 2) | (1 << 5);
        assert_eq!(data, expected as u8);
    }

    #[test]
    fn test_clear_int_status() {
        let mut kp = KeypadController::new();
        let keys = empty_key_state();

        kp.status = 0xFF;
        kp.enable = 0x07; // Enable all valid bits so we can read them back

        // Writing to INT_STATUS should clear those bits
        kp.write(regs::INT_STATUS, 0x05);
        let status_read = kp.read(regs::INT_STATUS, &keys);
        // CEmu returns status & enable, so we expect (0xFF & !0x05) & 0x07 = 0xFA & 0x07 = 0x02
        assert_eq!(status_read, 0x02);
        // Internal status should be 0xFA
        assert_eq!(kp.status, 0xFA);
    }

    #[test]
    fn test_interrupt_check() {
        let mut kp = KeypadController::new();
        let mut keys = empty_key_state();

        // No keys, no interrupt
        assert!(!kp.check_interrupt(&keys));

        // Enable continuous mode and interrupt mask
        kp.write(regs::CONTROL, mode::CONTINUOUS);
        kp.enable = 0x04;

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
        kp.enable = 0x04; // Enable any key interrupt

        // IDLE mode - no interrupt
        kp.write(regs::CONTROL, mode::IDLE);
        assert!(!kp.check_interrupt(&keys));

        // SINGLE mode (mode 1) - interrupt! (CEmu's keypad_any_check runs for mode 1)
        kp.write(regs::CONTROL, mode::SINGLE);
        assert!(kp.check_interrupt(&keys));

        // CONTINUOUS mode - interrupt!
        kp.write(regs::CONTROL, mode::CONTINUOUS);
        assert!(kp.check_interrupt(&keys));

        // MULTI_GROUP mode - no interrupt (handled differently)
        kp.write(regs::CONTROL, mode::MULTI_GROUP);
        assert!(!kp.check_interrupt(&keys));
    }

    #[test]
    fn test_read_out_of_range_row() {
        let mut kp = KeypadController::new();
        let keys = empty_key_state();

        // Rows beyond KEYPAD_MAX_ROWS wrap around via & 0x0F
        scan_keys(&mut kp, &keys);
        let data = kp.read(regs::DATA_BASE + 0x10, &keys); // Row 8
        assert_eq!(data, 0x00); // Wraps to data[8], which is 0
    }

    #[test]
    fn test_read_unknown_register() {
        let mut kp = KeypadController::new();
        let keys = empty_key_state();

        // Unknown register should return 0x00 (CEmu returns 0)
        let data = kp.read(0x3F, &keys);
        assert_eq!(data, 0x00);
    }

    #[test]
    fn test_packed_control_fields() {
        let mut kp = KeypadController::new();

        // Write mode to 3
        kp.write(regs::CONTROL, 0x03);
        assert_eq!(kp.mode(), 3);

        // Write row_wait: bits 15:2 of control
        // Write byte 0 with mode=1 and some row_wait bits
        // mode=01, rowWait lower bits = 0x10 -> byte 0 = 0x41
        kp.control = 0;
        kp.write(regs::CONTROL, 0x41); // mode=1, rowWait lower 6 bits = 0x10
        assert_eq!(kp.mode(), 1);
        assert_eq!(kp.row_wait(), 0x10);

        // Write scan_wait via byte 2 of control
        kp.write(regs::CONTROL + 2, 0x04); // scanWait low byte = 4
        assert_eq!(kp.scan_wait(), 4);
    }

    #[test]
    fn test_packed_size_fields() {
        let mut kp = KeypadController::new();

        // Default: rows=8, cols=8, mask=0xFFFF
        assert_eq!(kp.rows(), 8);
        assert_eq!(kp.cols(), 8);
        assert_eq!(kp.mask(), 0xFFFF);

        // Write rows via byte 0 of size
        kp.write(regs::SIZE, 4);
        assert_eq!(kp.rows(), 4);
        assert_eq!(kp.cols(), 8); // unchanged

        // Write cols via byte 1 of size
        kp.write(regs::SIZE + 1, 6);
        assert_eq!(kp.cols(), 6);

        // Write mask via bytes 2-3 of size
        kp.write(regs::SIZE + 2, 0x0F); // mask low byte
        kp.write(regs::SIZE + 3, 0x00); // mask high byte
        assert_eq!(kp.mask(), 0x000F);
    }

    #[test]
    fn test_enable_masked() {
        let mut kp = KeypadController::new();
        let keys = empty_key_state();

        // Enable register should be masked to 0x07
        kp.write(regs::INT_ACK, 0xFF);
        assert_eq!(kp.enable, 0x07);

        // Read it back
        let val = kp.read(regs::INT_ACK, &keys);
        assert_eq!(val, 0x07);
    }

    #[test]
    fn test_gpio_enable() {
        let mut kp = KeypadController::new();
        let keys = empty_key_state();

        // Write to GPIO enable register
        kp.write(regs::GPIO_ENABLE, 0xAB);
        kp.write(regs::GPIO_ENABLE + 1, 0xCD);

        assert_eq!(kp.read(regs::GPIO_ENABLE, &keys), 0xAB);
        assert_eq!(kp.read(regs::GPIO_ENABLE + 1, &keys), 0xCD);
        assert_eq!(kp.gpio_enable, 0x0000CDAB);
    }
}
