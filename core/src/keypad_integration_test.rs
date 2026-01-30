//! Integration test for keypad pipeline
//! Tests the full path from set_key() to keypad register reads

#[cfg(test)]
mod tests {
    use crate::bus::Bus;
    use crate::peripherals::{KEYPAD_COLS, KEYPAD_ROWS};

    /// Keypad register addresses (memory-mapped at 0xF50000)
    const KEYPAD_BASE: u32 = 0xF50000;
    const KEYPAD_CONTROL: u32 = KEYPAD_BASE + 0x00;
    const KEYPAD_SIZE: u32 = KEYPAD_BASE + 0x04;
    const KEYPAD_STATUS: u32 = KEYPAD_BASE + 0x08;
    const KEYPAD_INT_ACK: u32 = KEYPAD_BASE + 0x0C;
    const KEYPAD_DATA_BASE: u32 = KEYPAD_BASE + 0x10;

    /// Set up keypad in mode 1 (any-key detection mode) for testing.
    /// This is how TI-OS uses the keypad for key detection.
    fn setup_keypad_mode1(bus: &mut Bus) {
        bus.write_byte(KEYPAD_CONTROL, 0x01); // Mode 1
    }

    /// Trigger any_key_check by clearing INT_STATUS (simulates TI-OS acknowledging)
    /// This is how TI-OS causes the keypad data registers to be populated.
    /// CEmu calls keypad_any_check() after INT_STATUS is written.
    fn trigger_keypad_check(bus: &mut Bus) {
        bus.write_byte(KEYPAD_STATUS, 0xFF); // Clear all status bits
    }

    fn read_keypad_row(bus: &mut Bus, row: usize) -> u16 {
        let addr = KEYPAD_DATA_BASE + (row as u32) * 2;
        let lo = bus.read_byte(addr) as u16;
        let hi = bus.read_byte(addr + 1) as u16;
        lo | (hi << 8)
    }

    #[test]
    fn test_keypad_pipeline_basic() {
        let mut bus = Bus::new();

        // Set mode 1 (any-key detection mode, like TI-OS uses)
        setup_keypad_mode1(&mut bus);

        // Initially no keys pressed - all rows should be 0x0000
        for row in 0..KEYPAD_ROWS {
            let data = read_keypad_row(&mut bus, row);
            assert_eq!(data, 0x0000, "Row {} should be 0x0000 with no keys", row);
        }

        // Press key at row 3, col 2 (e.g., the "4" key)
        bus.set_key(3, 2, true);
        // Trigger any_key_check (simulates TI-OS clearing INT_STATUS)
        trigger_keypad_check(&mut bus);

        // In mode 1, all rows contain combined key data (CEmu behavior)
        // Verify the key bit is set (reading any row works)
        let key_data = read_keypad_row(&mut bus, 3);
        assert_eq!(
            key_data,
            1 << 2,
            "Row 3 should have bit 2 set after pressing key (3,2). Got: 0x{:04X}",
            key_data
        );

        // In mode 1, ALL rows have the same combined data
        // This is how CEmu's any_key_check works
        let row0_data = read_keypad_row(&mut bus, 0);
        assert_eq!(
            row0_data,
            1 << 2,
            "In mode 1, all rows have combined data. Got: 0x{:04X}",
            row0_data
        );

        // Release the key
        bus.set_key(3, 2, false);
        trigger_keypad_check(&mut bus);

        // Now all rows should be back to 0 (edge was cleared by previous query)
        let row3_data = read_keypad_row(&mut bus, 3);
        assert_eq!(
            row3_data, 0x0000,
            "Row 3 should be 0x0000 after releasing key"
        );
    }

    #[test]
    fn test_keypad_multiple_keys() {
        let mut bus = Bus::new();
        setup_keypad_mode1(&mut bus);

        // Press multiple keys in different rows
        bus.set_key(1, 5, true); // 2nd key
        bus.set_key(3, 3, true); // 7 key
        bus.set_key(6, 0, true); // enter key
        trigger_keypad_check(&mut bus);

        // In mode 1, all rows contain the combined OR of all pressed keys
        // Combined: (1 << 5) | (1 << 3) | (1 << 0) = 0x29
        let expected = (1 << 5) | (1 << 3) | (1 << 0);

        // All rows should have the combined data
        for row in 0..KEYPAD_ROWS {
            let data = read_keypad_row(&mut bus, row);
            assert_eq!(
                data, expected,
                "Row {} should have combined data 0x{:04X}, got 0x{:04X}",
                row, expected, data
            );
        }
    }

    #[test]
    fn test_keypad_multiple_keys_same_row() {
        let mut bus = Bus::new();
        setup_keypad_mode1(&mut bus);

        // Press multiple keys in the same row
        bus.set_key(3, 0, true); // 0 key
        bus.set_key(3, 1, true); // 1 key
        bus.set_key(3, 2, true); // 4 key
        bus.set_key(3, 3, true); // 7 key
        trigger_keypad_check(&mut bus);

        let row3_data = read_keypad_row(&mut bus, 3);
        let expected = (1 << 0) | (1 << 1) | (1 << 2) | (1 << 3);
        assert_eq!(
            row3_data, expected,
            "Row 3 should have bits 0-3 set. Got: 0x{:04X}, expected: 0x{:04X}",
            row3_data, expected
        );
    }

    #[test]
    fn test_keypad_on_key() {
        let mut bus = Bus::new();
        setup_keypad_mode1(&mut bus);

        // ON key is at row 2, col 0
        bus.set_key(2, 0, true);
        trigger_keypad_check(&mut bus);

        let row2_data = read_keypad_row(&mut bus, 2);
        assert_eq!(
            row2_data,
            1 << 0,
            "Row 2 should have bit 0 set for ON key. Got: 0x{:04X}",
            row2_data
        );
    }

    #[test]
    fn test_keypad_arrow_keys() {
        let mut bus = Bus::new();
        setup_keypad_mode1(&mut bus);

        // Arrow keys per CEmu mapping:
        // DOWN: row 7, col 0
        // LEFT: row 7, col 1
        // RIGHT: row 7, col 2
        // UP: row 7, col 3

        bus.set_key(7, 0, true); // DOWN
        trigger_keypad_check(&mut bus);
        assert_eq!(read_keypad_row(&mut bus, 7), 1 << 0, "DOWN key");

        bus.set_key(7, 1, true); // LEFT
        trigger_keypad_check(&mut bus);
        assert_eq!(
            read_keypad_row(&mut bus, 7),
            (1 << 0) | (1 << 1),
            "DOWN + LEFT"
        );

        bus.set_key(7, 2, true); // RIGHT
        trigger_keypad_check(&mut bus);
        assert_eq!(
            read_keypad_row(&mut bus, 7),
            (1 << 0) | (1 << 1) | (1 << 2),
            "DOWN + LEFT + RIGHT"
        );

        bus.set_key(7, 3, true); // UP
        trigger_keypad_check(&mut bus);
        assert_eq!(
            read_keypad_row(&mut bus, 7),
            (1 << 0) | (1 << 1) | (1 << 2) | (1 << 3),
            "All arrow keys"
        );
    }

    #[test]
    fn test_keypad_with_emu() {
        // Test the full Emu pipeline using Bus directly
        // (Emu.bus is private, so we test Bus separately)
        let mut bus = Bus::new();
        setup_keypad_mode1(&mut bus);

        // This simulates what Emu.set_key does internally
        bus.set_key(3, 2, true);
        trigger_keypad_check(&mut bus);

        // Read the keypad register through the bus
        let addr = KEYPAD_DATA_BASE + 3 * 2; // Row 3
        let lo = bus.read_byte(addr);
        let hi = bus.read_byte(addr + 1);
        let row_data = (lo as u16) | ((hi as u16) << 8);

        assert_eq!(
            row_data,
            1 << 2,
            "Bus.set_key should result in readable keypad data. Got: 0x{:04X}",
            row_data
        );
    }

    #[test]
    fn test_keypad_control_registers() {
        let mut bus = Bus::new();

        // Read control register (should have default value)
        let control = bus.read_byte(KEYPAD_CONTROL);
        println!("Keypad CONTROL: 0x{:02X}", control);

        // Read size register bytes
        // SIZE register is now 4 bytes: rows (8), cols (8), mask_lo (0xFF), mask_hi (0x00)
        let rows = bus.read_byte(KEYPAD_SIZE);
        assert_eq!(rows, 8, "Rows should be 8");
        let cols = bus.read_byte(KEYPAD_SIZE + 1);
        assert_eq!(cols, 8, "Cols should be 8");
        let mask_lo = bus.read_byte(KEYPAD_SIZE + 2);
        assert_eq!(mask_lo, 0xFF, "Mask low byte should be 0xFF (all 8 rows enabled)");
        let mask_hi = bus.read_byte(KEYPAD_SIZE + 3);
        assert_eq!(mask_hi, 0x00, "Mask high byte should be 0x00");

        // Read status register
        let status = bus.read_byte(KEYPAD_STATUS);
        println!("Keypad STATUS: 0x{:02X}", status);

        // Read interrupt mask
        let int_mask = bus.read_byte(KEYPAD_INT_ACK);
        println!("Keypad INT_MASK: 0x{:02X}", int_mask);
    }

    #[test]
    fn test_keypad_data_format() {
        // This test documents the expected data format
        let mut bus = Bus::new();
        setup_keypad_mode1(&mut bus);

        // Press key at (5, 3) - the "9" key
        bus.set_key(5, 3, true);
        trigger_keypad_check(&mut bus);

        // The data register for row 5 is at KEYPAD_DATA_BASE + 5*2 = 0xF5001A
        let addr = KEYPAD_DATA_BASE + 5 * 2;

        // Read individual bytes
        let lo = bus.read_byte(addr);
        let hi = bus.read_byte(addr + 1);

        println!("Key (5,3) pressed:");
        println!("  Address: 0x{:06X}", addr);
        println!("  Low byte: 0x{:02X} (binary: {:08b})", lo, lo);
        println!("  High byte: 0x{:02X} (binary: {:08b})", hi, hi);
        println!("  Combined: 0x{:04X}", (lo as u16) | ((hi as u16) << 8));

        // Bit 3 should be set in low byte
        assert_eq!(lo, 0x08, "Low byte should be 0x08 (bit 3 set)");
        assert_eq!(hi, 0x00, "High byte should be 0x00");
    }

    #[test]
    fn test_keypad_address_routing() {
        // Test that addresses are correctly routed to the keypad controller
        let mut bus = Bus::new();
        setup_keypad_mode1(&mut bus);

        // Press a key
        bus.set_key(3, 2, true);
        trigger_keypad_check(&mut bus);

        // Test various address formats that might be used to read keypad
        let addresses = [
            (0xF50010u32, "0xF50010 - Row 0 direct"),
            (0xF50016u32, "0xF50016 - Row 3 direct"),
            (0xF50000u32, "0xF50000 - Control register"),
            (0xF50004u32, "0xF50004 - Size register"),
            (0xF50008u32, "0xF50008 - Status register"),
        ];

        println!("\nKeypad address routing test:");
        println!("Key pressed at (3, 2)");
        for (addr, desc) in addresses {
            let value = bus.read_byte(addr);
            println!("  {} = 0x{:02X}", desc, value);
        }

        // Row 3 data should have bit 2 set
        let row3_lo = bus.read_byte(0xF50016);
        let row3_hi = bus.read_byte(0xF50017);
        println!("  Row 3 full: 0x{:02X}{:02X}", row3_hi, row3_lo);

        assert_eq!(row3_lo, 0x04, "Row 3 low byte should be 0x04 (bit 2)");
    }

    #[test]
    fn test_keypad_timing_simulation() {
        // Simulate what happens during emulation with key presses
        let mut bus = Bus::new();
        setup_keypad_mode1(&mut bus);

        println!("\nTiming simulation:");

        // Initial state
        println!("1. Initial state - no keys pressed");
        let row3 = bus.read_byte(0xF50016);
        println!("   Row 3 data: 0x{:02X}", row3);
        assert_eq!(row3, 0x00);

        // Press key
        println!("2. Press key (3, 2)");
        bus.set_key(3, 2, true);
        trigger_keypad_check(&mut bus);

        // Immediate read after keypad check
        let row3 = bus.read_byte(0xF50016);
        println!("   Row 3 data after check: 0x{:02X}", row3);
        assert_eq!(row3, 0x04);

        // Simulate some cycles (tick the peripherals)
        println!("3. Run 1000 cycles");
        bus.ports.tick(1000);

        // Read again - data persists since key is still held
        let row3 = bus.read_byte(0xF50016);
        println!("   Row 3 data after tick: 0x{:02X}", row3);
        assert_eq!(row3, 0x04, "Key should still be pressed after tick");

        // Release key
        println!("4. Release key");
        bus.set_key(3, 2, false);
        trigger_keypad_check(&mut bus);

        let row3 = bus.read_byte(0xF50016);
        println!("   Row 3 data after release: 0x{:02X}", row3);
        assert_eq!(row3, 0x00);
    }

    #[test]
    fn test_what_os_might_see() {
        // Simulate what the TI-OS might be doing to read keys
        let mut bus = Bus::new();

        println!("\nSimulating OS key polling:");

        // The OS typically:
        // 1. Reads control/status to check if scanning is enabled
        // 2. Reads the data registers to get key state

        println!("1. Check keypad status");
        let control = bus.read_byte(0xF50000);
        let size = bus.read_byte(0xF50004);
        let status = bus.read_byte(0xF50008);
        let int_mask = bus.read_byte(0xF5000C);
        println!("   Control: 0x{:02X}", control);
        println!("   Size: 0x{:02X}", size);
        println!("   Status: 0x{:02X}", status);
        println!("   Int mask: 0x{:02X}", int_mask);

        // Set mode 1 for key detection
        bus.write_byte(0xF50000, 0x01);

        println!("2. Press the '5' key (row 4, col 2)");
        bus.set_key(4, 2, true);
        trigger_keypad_check(&mut bus);

        println!("3. Read all row data:");
        for row in 0..8 {
            let addr = 0xF50010 + row * 2;
            let lo = bus.read_byte(addr);
            let hi = bus.read_byte(addr + 1);
            let data = (lo as u16) | ((hi as u16) << 8);
            if data != 0 {
                println!("   Row {}: 0x{:04X} (addr 0x{:06X}) <- KEY PRESSED", row, data, addr);
            } else {
                println!("   Row {}: 0x{:04X} (addr 0x{:06X})", row, data, addr);
            }
        }

        println!("4. Check status after read");
        let status = bus.read_byte(0xF50008);
        println!("   Status: 0x{:02X}", status);
    }

    #[test]
    fn test_keypad_android_mapping() {
        // Test the specific key mappings used in Android
        let mut bus = Bus::new();
        setup_keypad_mode1(&mut bus);

        // These are the mappings from MainActivity.kt
        let android_keys = [
            // (row, col, description)
            (1, 4, "y="),
            (1, 3, "window"),
            (1, 2, "zoom"),
            (1, 1, "trace"),
            (1, 0, "graph"),
            (1, 5, "2nd"),
            (1, 6, "mode"),
            (1, 7, "del"),
            (2, 7, "alpha"),
            (3, 7, "X,T,θ,n"),
            (4, 7, "stat"),
            (2, 6, "math"),
            (3, 6, "apps"),
            (4, 6, "prgm"),
            (5, 6, "vars"),
            (6, 6, "clear"),
            (2, 5, "x⁻¹"),
            (3, 5, "sin"),
            (4, 5, "cos"),
            (5, 5, "tan"),
            (6, 5, "^"),
            (2, 4, "x²"),
            (3, 4, ","),
            (4, 4, "("),
            (5, 4, ")"),
            (6, 4, "÷"),
            (2, 3, "log"),
            (2, 2, "ln"),
            (2, 1, "sto→"),
            (2, 0, "on"),
            (3, 3, "7"),
            (3, 2, "4"),
            (3, 1, "1"),
            (3, 0, "0"),
            (4, 3, "8"),
            (4, 2, "5"),
            (4, 1, "2"),
            (4, 0, "."),
            (5, 3, "9"),
            (5, 2, "6"),
            (5, 1, "3"),
            (5, 0, "(−)"),
            (6, 3, "×"),
            (6, 2, "−"),
            (6, 1, "+"),
            (6, 0, "enter"),
            (7, 3, "up"),
            (7, 1, "left"),
            (7, 2, "right"),
            (7, 0, "down"),
        ];

        println!("Testing Android key mappings:");
        for (row, col, name) in android_keys.iter() {
            // Verify the mapping is within bounds
            assert!(
                *row < KEYPAD_ROWS,
                "Key '{}' has invalid row {} (max {})",
                name,
                row,
                KEYPAD_ROWS - 1
            );
            assert!(
                *col < KEYPAD_COLS,
                "Key '{}' has invalid col {} (max {})",
                name,
                col,
                KEYPAD_COLS - 1
            );

            // Press the key
            bus.set_key(*row, *col, true);
            trigger_keypad_check(&mut bus);

            // Verify it's readable
            let row_data = read_keypad_row(&mut bus, *row);
            let expected_bit = 1u16 << col;
            assert!(
                (row_data & expected_bit) != 0,
                "Key '{}' at ({},{}) should set bit {} in row data. Got: 0x{:04X}",
                name,
                row,
                col,
                col,
                row_data
            );

            // Release for next test
            bus.set_key(*row, *col, false);
        }
        println!("All {} Android key mappings verified!", android_keys.len());
    }

    #[test]
    fn test_keypad_edge_detection() {
        // Test the edge detection mechanism:
        // A key pressed and released BEFORE the query should still be detected
        // This is critical for fast key presses on Android where the key might
        // be released before TI-OS has a chance to poll the keypad.
        let mut bus = Bus::new();
        setup_keypad_mode1(&mut bus);

        // Press key
        bus.set_key(3, 2, true);

        // Release key BEFORE triggering the check
        bus.set_key(3, 2, false);

        // Now trigger the keypad check (simulates TI-OS clearing INT_STATUS)
        // Even though the key is released, the edge flag should still be set
        trigger_keypad_check(&mut bus);

        // The key should be detected because the edge flag was preserved!
        let row_data = read_keypad_row(&mut bus, 3);
        assert_eq!(
            row_data,
            1 << 2,
            "Edge detection failed: key pressed+released before query should still be detected. Got: 0x{:04X}",
            row_data
        );

        // A second query should show 0 (edge was cleared by first query, key not held)
        trigger_keypad_check(&mut bus);
        let row_data = read_keypad_row(&mut bus, 3);
        assert_eq!(
            row_data, 0x0000,
            "After edge is consumed, data should be 0. Got: 0x{:04X}",
            row_data
        );
    }

    #[test]
    fn test_keypad_edge_multiple_press_release() {
        // Test that multiple quick press-release cycles accumulate edges
        let mut bus = Bus::new();
        setup_keypad_mode1(&mut bus);

        // Press and release key 1
        bus.set_key(3, 1, true);
        bus.set_key(3, 1, false);

        // Press and release key 2
        bus.set_key(4, 2, true);
        bus.set_key(4, 2, false);

        // Press and release key 3
        bus.set_key(5, 3, true);
        bus.set_key(5, 3, false);

        // Now trigger check - should see all three keys!
        trigger_keypad_check(&mut bus);

        // All edges should be detected (combined in mode 1)
        let expected = (1 << 1) | (1 << 2) | (1 << 3);
        let row_data = read_keypad_row(&mut bus, 0); // Any row in mode 1
        assert_eq!(
            row_data, expected,
            "Multiple press-release should accumulate edges. Got: 0x{:04X}, expected: 0x{:04X}",
            row_data, expected
        );
    }
}
