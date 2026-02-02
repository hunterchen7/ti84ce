//! TI-84 Plus CE Keypad Controller
//!
//! Memory-mapped at 0xF50000 (port offset 0x150000 from 0xE00000)
//!
//! The keypad is an 8x8 matrix. The controller scans rows and stores
//! the column data in data registers.
//!
//! ## Scan Timing
//!
//! When scanning is initiated (modes 1, 2 or 3), the controller scans one row
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
    /// Matrix rows configuration (byte 0 of SIZE)
    rows: u8,
    /// Matrix cols configuration (byte 1 of SIZE)
    cols: u8,
    /// Row mask (bytes 2-3 of SIZE) - determines which rows are active
    mask: u16,
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
    /// Current scan results (used for mode 1 combined data)
    current_scan_data: [u16; KEYPAD_ROWS],
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
            rows: 8,
            cols: 8,
            mask: 0x00FF, // Default: all 8 rows enabled
            int_status: 0,
            int_mask: 0,
            scan_row: 0,
            scanning: false,
            scan_cycles_remaining: 0,
            row_wait: DEFAULT_ROW_WAIT,
            scan_wait: DEFAULT_SCAN_WAIT,
            prev_scan_data: [0x0000; KEYPAD_ROWS],
            current_scan_data: [0x0000; KEYPAD_ROWS],
            any_key_in_scan: false,
            data_changed_in_scan: false,
            needs_any_key_check: false,
            key_edge_flags: [[false; KEYPAD_COLS]; KEYPAD_ROWS],
        }
    }

    /// Reset the keypad controller
    pub fn reset(&mut self) {
        self.control = 0;
        self.rows = 8;
        self.cols = 8;
        self.mask = 0x00FF;
        self.int_status = 0;
        self.int_mask = 0;
        self.scan_row = 0;
        self.scanning = false;
        self.scan_cycles_remaining = 0;
        self.row_wait = DEFAULT_ROW_WAIT;
        self.scan_wait = DEFAULT_SCAN_WAIT;
        self.prev_scan_data = [0x0000; KEYPAD_ROWS];
        self.current_scan_data = [0x0000; KEYPAD_ROWS];
        self.any_key_in_scan = false;
        self.data_changed_in_scan = false;
        self.needs_any_key_check = false;
        self.key_edge_flags = [[false; KEYPAD_COLS]; KEYPAD_ROWS];
    }

    /// Update key edge flag (called when key state changes)
    /// When pressed=true, sets edge flag (will be seen by next query)
    /// When pressed=false, does NOT clear edge (CEmu behavior)
    /// Edge flags are cleared only by query_row_data()
    ///
    /// Also immediately updates current_scan_data so the key is visible
    /// in data register reads regardless of keypad mode. This is critical
    /// for TI-OS key detection which may not switch to mode 1 before reading.
    ///
    /// IMPORTANT: On key release, we do NOT clear current_scan_data!
    /// The data should persist until the OS reads and processes it.
    /// CEmu's edge detection works the same way - keymap_edge preserves
    /// the key press until it's queried.
    pub fn set_key_edge(&mut self, row: usize, col: usize, pressed: bool) {
        if row < KEYPAD_ROWS && col < KEYPAD_COLS {
            if pressed {
                // Set edge on press, not on release (CEmu behavior)
                self.key_edge_flags[row][col] = true;

                // Immediately update current_scan_data so the key is visible
                // This is needed because any_key_check only runs in mode 1,
                // but TI-OS might read data registers in mode 0.
                self.current_scan_data[row] |= 1 << col;

                // Set status flags
                self.int_status |= status::DATA_CHANGED | status::ANY_KEY;
            } else {
                // CEmu clears the keyMap bit on release:
                // keyMap[row] &= ~(1 << col)
                // The edge flag persists for detection, but the data should reflect
                // current key state.
                self.current_scan_data[row] &= !(1 << col);
            }
        }
    }

    /// Get the current scan mode
    pub fn mode(&self) -> u8 {
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

        // Log scan completion with results (disabled in WASM)
        #[cfg(not(target_arch = "wasm32"))]
        {
            static mut FINISH_COUNT: u32 = 0;
            unsafe {
                FINISH_COUNT += 1;
                if FINISH_COUNT % 1000 == 1 || self.any_key_in_scan {
                    crate::emu::log_event(&format!(
                        "KEYPAD_SCAN_DONE: mode={} any_key={} data_changed={} data={:?}",
                        self.mode(), self.any_key_in_scan, self.data_changed_in_scan,
                        self.current_scan_data.iter().take(8).collect::<Vec<_>>()
                    ));
                }
            }
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

        // Log scan activity (disabled in WASM)
        #[cfg(not(target_arch = "wasm32"))]
        {
            static mut SCAN_LOG_COUNT: u32 = 0;
            unsafe {
                SCAN_LOG_COUNT += 1;
                if SCAN_LOG_COUNT % 100000 == 1 {
                    crate::emu::log_event(&format!("KEYPAD_SCAN: active, mode={}, row={}", self.mode(), self.scan_row));
                }
            }
        }

        let mut cycles_left = cycles;
        let mut interrupt_pending = false;

        while cycles_left > 0 && self.scanning {
            if cycles_left >= self.scan_cycles_remaining {
                cycles_left -= self.scan_cycles_remaining;
                self.scan_cycles_remaining = 0;

                // Scan the current row
                if self.scan_row < KEYPAD_ROWS {
                    // Use query_row_data instead of compute_row_data!
                    // CEmu's keypad_scan_event calls keypad_query_keymap() which:
                    // 1. Returns current state OR edge flags
                    // 2. Clears edge flags after reading
                    // This allows detecting quick press/release even if released before scan
                    let row_data = self.query_row_data(self.scan_row, key_state);
                    self.current_scan_data[self.scan_row] = row_data;

                    // Check if any key is pressed in this row
                    // Any key pressed = non-zero (active high)
                    if row_data != 0 {
                        self.any_key_in_scan = true;
                        // Log when we detect a key during scan
                        crate::emu::log_event(&format!(
                            "KEYPAD_SCAN_KEY: row={} data=0x{:04X}",
                            self.scan_row, row_data
                        ));
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

    /// Compute row data from key matrix (non-destructive, current state only)
    /// Returns a bitmask where 1 = pressed, 0 = not pressed (active high, matches CEmu)
    fn compute_row_data(&self, row: usize, key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> u16 {
        let mut result = 0x0000_u16; // All keys released

        if row < KEYPAD_ROWS {
            for col in 0..KEYPAD_COLS {
                if key_state[row][col] {
                    result |= 1 << col; // Key pressed = bit set
                }
            }
        }

        result
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

    /// Read a register byte
    /// addr is offset from controller base (0-0x3F)
    /// key_state is the current keyboard matrix state
    pub fn read(&mut self, addr: u32, key_state: &[[bool; KEYPAD_COLS]; KEYPAD_ROWS]) -> u8 {
        match addr {
            regs::CONTROL => {
                self.control
            }
            // SIZE register is 4 bytes: rows, cols, mask_lo, mask_hi
            a if a >= regs::SIZE && a < regs::SIZE + 4 => {
                match a - regs::SIZE {
                    0 => self.rows,
                    1 => self.cols,
                    2 => self.mask as u8,
                    3 => (self.mask >> 8) as u8,
                    _ => 0,
                }
            }
            regs::INT_STATUS => {
                // CEmu returns (status & enable), not raw status!
                self.int_status & self.int_mask
            }
            regs::INT_ACK => self.int_mask,
            a if a >= regs::DATA_BASE && a < regs::DATA_BASE + 0x20 => {
                // Row data registers - each row has 2 bytes (16 bits)
                let row_offset = (a - regs::DATA_BASE) as usize;
                let row = row_offset / 2;
                let byte = row_offset % 2;

                if row < KEYPAD_ROWS {
                    // Read from stored data (populated by any_key_check or scan events)
                    let row_data = self.current_scan_data[row];

                    // Log data reads (disabled in WASM)
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        static mut READ_LOG_COUNT: u32 = 0;
                        static mut LAST_KEY_STATE_HASH: u64 = 0;
                        unsafe {
                            READ_LOG_COUNT += 1;
                            // Hash of key state to detect changes
                            let key_hash: u64 = key_state.iter()
                                .enumerate()
                                .flat_map(|(r, row)| row.iter().enumerate().map(move |(c, &v)| {
                                    if v { ((r as u64) << 8) | c as u64 } else { 0 }
                                }))
                                .fold(0, |acc, x| acc.wrapping_add(x));

                            if row_data != 0 || (key_hash != 0 && key_hash != LAST_KEY_STATE_HASH) {
                                LAST_KEY_STATE_HASH = key_hash;
                                eprintln!(
                                    "KEYPAD_READ: row={} data=0x{:04X} mode={} key_state={:?} count={}",
                                    row, row_data, self.mode(),
                                    key_state[row].iter().enumerate()
                                        .filter(|(_, &v)| v)
                                        .map(|(i, _)| i)
                                        .collect::<Vec<_>>(),
                                    READ_LOG_COUNT
                                );
                            }
                        }
                    }

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
            // SIZE register is 4 bytes: rows, cols, mask_lo, mask_hi
            a if a >= regs::SIZE && a < regs::SIZE + 4 => {
                match a - regs::SIZE {
                    0 => self.rows,
                    1 => self.cols,
                    2 => self.mask as u8,
                    3 => (self.mask >> 8) as u8,
                    _ => 0,
                }
            }
            regs::INT_STATUS => self.int_status & self.int_mask, // CEmu returns masked status
            regs::INT_ACK => self.int_mask,
            a if a >= regs::DATA_BASE && a < regs::DATA_BASE + 0x20 => {
                let row_offset = (a - regs::DATA_BASE) as usize;
                let row = row_offset / 2;
                let byte = row_offset % 2;

                if row < KEYPAD_ROWS {
                    // Return stored scan data (consistent with read behavior)
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

                // Log mode changes only
                if old_mode != new_mode {
                    crate::emu::log_event(&format!("KEYPAD_MODE: changed {} -> {}", old_mode, new_mode));
                }

                // CEmu: if (mode & 2) start scanning, else call any_key_check
                if (new_mode & 2) != 0 {
                    // Mode 2 or 3: start scheduled scanning
                    if old_mode != new_mode || new_mode == mode::MULTI_GROUP {
                        self.start_scan();
                    }
                } else {
                    // Mode 0 or 1: stop scanning and do immediate key check
                    self.scanning = false;
                    self.needs_any_key_check = true;
                }
            }
            // SIZE register is 4 bytes: rows, cols, mask_lo, mask_hi
            a if a >= regs::SIZE && a < regs::SIZE + 4 => {
                match a - regs::SIZE {
                    0 => self.rows = value,
                    1 => self.cols = value,
                    2 => self.mask = (self.mask & 0xFF00) | (value as u16),
                    3 => self.mask = (self.mask & 0x00FF) | ((value as u16) << 8),
                    _ => {}
                }
                // CEmu calls keypad_any_check() after SIZE write
                self.needs_any_key_check = true;
            }
            regs::INT_STATUS => {
                // Writing clears status bits (write-1-to-clear)
                self.int_status &= !value;
                // CEmu calls keypad_any_check() after clearing status!
                // This updates data registers with current key state.
                self.needs_any_key_check = true;
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
        let row_limit = std::cmp::min(self.rows as usize, KEYPAD_ROWS);

        for row in 0..row_limit {
            // Only query rows that are enabled in the mask
            if (self.mask & (1 << row)) != 0 {
                // Use query_row_data for edge detection (CEmu: keypad_query_keymap)
                any |= self.query_row_data(row, key_state);
            }
        }

        // Apply column mask (data_mask in CEmu)
        let col_limit = std::cmp::min(self.cols as usize, KEYPAD_COLS);
        let data_mask: u16 = (1 << col_limit) - 1;
        any &= data_mask;

        // Log when we actually detect keys (debug only)
        // if any != 0 {
        //     crate::emu::log_event(&format!("KEYPAD_CHECK: detected keys! any=0x{:04X}", any));
        // }

        // CEmu: Store combined 'any' in ALL rows that are in the mask
        // This is the critical behavior for TI-OS key detection!
        let row_limit_full = std::cmp::min(self.rows as usize, KEYPAD_ROWS);
        for row in 0..row_limit_full {
            if (self.mask & (1 << row)) != 0 {
                // Check if data changed
                if self.current_scan_data[row] != any {
                    self.int_status |= status::DATA_CHANGED;
                }
                self.current_scan_data[row] = any;
            }
        }

        // Set any-key status if keys are pressed (CEmu: if (any & mask))
        if any != 0 {
            self.int_status |= status::ANY_KEY;
        }

        // Return true if interrupt should fire (status & mask)
        (self.int_status & self.int_mask) != 0
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
        assert_eq!(kp.rows, 8); // 8 rows
        assert_eq!(kp.cols, 8); // 8 columns
        assert_eq!(kp.mask, 0x00FF); // All 8 rows enabled
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
        assert_eq!(kp.rows, 8);
        assert_eq!(kp.cols, 8);
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

        kp.int_status = 0xFF;
        kp.int_mask = 0xFF; // Enable all bits so we can read them back

        // Writing to INT_STATUS should clear those bits
        kp.write(regs::INT_STATUS, 0x05);
        let status = kp.read(regs::INT_STATUS, &keys);
        // CEmu returns status & enable, so we expect (0xFF & !0x05) & 0xFF = 0xFA
        assert_eq!(status, 0xFA);
        // Internal status should also be 0xFA
        assert_eq!(kp.int_status, 0xFA);
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

        // SINGLE mode (mode 1) - interrupt! (CEmu's keypad_any_check runs for mode 1)
        kp.control = mode::SINGLE;
        assert!(kp.check_interrupt(&keys));

        // CONTINUOUS mode - interrupt!
        kp.control = mode::CONTINUOUS;
        assert!(kp.check_interrupt(&keys));

        // MULTI_GROUP mode - no interrupt (handled differently)
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
