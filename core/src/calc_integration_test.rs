//! Integration tests for calculation correctness
//!
//! These tests boot the emulator, type expressions, and verify
//! the results match expected values.

#[cfg(test)]
mod tests {
    use crate::Emu;
    use std::path::Path;

    /// TI-84 CE expression result address
    /// Discovered via RAM search: expression results (like "6*7") go to 0xD00619
    /// Simple literals (like "5") go to 0xD01464
    const OP1_ADDR: u32 = 0xD00619;

    /// Alternative addresses to check for floating point result
    /// Order matters - check expression result location first!
    const ALT_OP_ADDRS: [u32; 7] = [
        0xD00619, // Where expression results go (check FIRST!)
        0xD01464, // Ans - where simple results also go
        0xD005F8, // OP1 traditional
        0xD005FB, // OP1 + 3
        0xD00505, // Another common location
        0xD0257C, // Ans variable alternate
        0xD00624, // Found 10 here in scan
    ];

    /// Key mappings for TI-84 CE (row, col)
    fn char_to_key(c: char) -> Option<(usize, usize)> {
        match c {
            '0' => Some((3, 0)),
            '1' => Some((3, 1)),
            '2' => Some((4, 1)),
            '3' => Some((5, 1)),
            '4' => Some((3, 2)),
            '5' => Some((4, 2)),
            '6' => Some((5, 2)),
            '7' => Some((3, 3)),
            '8' => Some((4, 3)),
            '9' => Some((5, 3)),
            '+' => Some((6, 1)),
            '-' => Some((6, 2)),
            '*' => Some((6, 3)),
            '/' => Some((6, 4)),
            _ => None,
        }
    }

    /// Parse TI floating point format from 9 bytes
    /// Returns the floating point value, or None if invalid
    fn parse_ti_float(bytes: &[u8; 9]) -> Option<f64> {
        // TI format:
        // Byte 0: Type/Sign (0x00 = positive real, 0x80 = negative real)
        // Byte 1: Exponent (biased by 0x80)
        // Bytes 2-8: BCD mantissa (14 digits, 2 per byte)

        let sign_byte = bytes[0];
        let negative = (sign_byte & 0x80) != 0;

        // Check for valid real number type (low nibble should be 0 for real)
        if (sign_byte & 0x0F) != 0 {
            return None; // Not a simple real number
        }

        let exp_byte = bytes[1];
        if exp_byte == 0 {
            return Some(0.0); // Zero
        }

        let exponent = (exp_byte as i32) - 0x80;

        // Parse BCD mantissa
        let mut mantissa = 0.0;
        let mut place = 0.1;
        for i in 2..9 {
            let byte = bytes[i];
            let high = (byte >> 4) & 0x0F;
            let low = byte & 0x0F;

            mantissa += (high as f64) * place;
            place *= 0.1;
            mantissa += (low as f64) * place;
            place *= 0.1;
        }

        let result = mantissa * 10.0_f64.powi(exponent + 1);
        Some(if negative { -result } else { result })
    }

    /// Read result from emulator, searching multiple known addresses
    /// Returns (address, bytes) tuple with the most likely result
    fn read_result(emu: &mut Emu) -> (u32, [u8; 9]) {
        // Check expression result address first (0xD00619)
        // Then check simple literal address (0xD01464)
        let primary_addrs = [0xD00619u32, 0xD01464];

        for &addr in &primary_addrs {
            let bytes = read_bytes_at(emu, addr);
            if let Some(v) = parse_ti_float(&bytes) {
                if v != 0.0 {
                    return (addr, bytes);
                }
            }
        }

        // Fall back to scanning all known addresses
        for &addr in &ALT_OP_ADDRS {
            let bytes = read_bytes_at(emu, addr);
            if let Some(v) = parse_ti_float(&bytes) {
                if v != 0.0 {
                    return (addr, bytes);
                }
            }
        }

        // Return default
        (OP1_ADDR, read_bytes_at(emu, OP1_ADDR))
    }

    /// Read OP1 (9-byte floating point result) from emulator
    fn read_op1(emu: &mut Emu) -> [u8; 9] {
        read_result(emu).1
    }

    /// Try to load ROM from common locations
    fn try_load_rom() -> Option<Vec<u8>> {
        let paths = [
            "TI-84 CE.rom",
            "../TI-84 CE.rom",
            "../../TI-84 CE.rom",
        ];
        for path in paths {
            if Path::new(path).exists() {
                if let Ok(data) = std::fs::read(path) {
                    return Some(data);
                }
            }
        }
        None
    }

    /// Boot emulator and prepare for calculations
    fn boot_emulator() -> Option<Emu> {
        let rom_data = try_load_rom()?;
        let mut emu = Emu::new();
        emu.load_rom(&rom_data).ok()?;
        emu.press_on_key();

        // Boot: run until past BOOT_COMPLETE_CYCLES (65M)
        // The emulator auto-sends init ENTER after boot
        let boot_cycles = 70_000_000u32;
        let mut total = 0u64;
        while total < boot_cycles as u64 {
            let executed = emu.run_cycles(1_000_000);
            total += executed as u64;
        }

        // Release ON key
        emu.release_on_key();
        emu.run_cycles(1_000_000);

        // Do a warmup calculation to initialize TI-OS expression state
        // This is needed because the first calculation after boot behaves differently
        warmup_expression(&mut emu);

        Some(emu)
    }

    /// Send warmup calculations to initialize TI-OS expression parser
    /// TI-OS needs different operations to be "warmed up" before they work reliably
    fn warmup_expression(emu: &mut Emu) {
        // First warmup: simple number
        press_key_seq(emu, &[(3, 1)]); // "1"
        press_enter(emu);

        // Second warmup: simple addition
        press_key_seq(emu, &[(3, 1), (6, 1), (3, 1)]); // "1+1"
        press_enter(emu);

        // Third warmup: simple multiplication
        press_key_seq(emu, &[(4, 1), (6, 3), (5, 1)]); // "2*3"
        press_enter(emu);
    }

    /// Press a sequence of keys
    fn press_key_seq(emu: &mut Emu, keys: &[(usize, usize)]) {
        for &(row, col) in keys {
            emu.set_key(row, col, true);
            emu.run_cycles(500_000);
            emu.set_key(row, col, false);
            emu.run_cycles(500_000);
        }
    }

    /// Press ENTER and wait
    fn press_enter(emu: &mut Emu) {
        emu.set_key(6, 0, true);
        emu.run_cycles(500_000);
        emu.set_key(6, 0, false);
        emu.run_cycles(5_000_000);
    }

    /// Type an expression and press ENTER, returning OP1 result
    fn evaluate_expression(emu: &mut Emu, expr: &str) -> [u8; 9] {
        println!("Typing expression: '{}'", expr);

        // Type each character with longer delays
        // Use extra delay when repeating the same key
        let mut last_key: Option<(usize, usize)> = None;
        for c in expr.chars() {
            if let Some((row, col)) = char_to_key(c) {
                println!("  Key '{}' -> row={}, col={}", c, row, col);

                // Extra delay if same key as previous (edge detection needs time)
                if last_key == Some((row, col)) {
                    emu.run_cycles(3_000_000); // Extra 60ms gap for same key
                }

                emu.set_key(row, col, true);
                emu.run_cycles(1_500_000); // Hold (~30ms at 48MHz)
                emu.set_key(row, col, false);
                emu.run_cycles(3_000_000); // Gap between keys (~60ms)

                last_key = Some((row, col));
            } else {
                println!("  Unknown key: '{}'", c);
            }
        }

        // Press ENTER
        println!("  Pressing ENTER (row=6, col=0)");
        emu.set_key(6, 0, true);
        emu.run_cycles(1_000_000); // Hold longer
        emu.set_key(6, 0, false);

        // Give TI-OS time to calculate and update OP1
        // Run significantly more cycles to ensure calculation completes
        println!("  Running 20M cycles for calculation...");
        emu.run_cycles(20_000_000);

        read_op1(emu)
    }

    /// Format OP1 bytes for debugging
    fn format_op1(bytes: &[u8; 9]) -> String {
        bytes.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(" ")
    }

    // ========== Actual Tests ==========

    /// Read 9 bytes from a given address
    fn read_bytes_at(emu: &mut Emu, addr: u32) -> [u8; 9] {
        let mut bytes = [0u8; 9];
        for i in 0..9 {
            bytes[i] = emu.peek_byte(addr + i as u32);
        }
        bytes
    }

    /// Search RAM for a specific float value (approximate match)
    fn search_for_value(emu: &mut Emu, target: f64) -> Option<(u32, f64)> {
        // Scan RAM area (0xD00000 - 0xD65800) looking for floats close to target
        // Step by 1 to catch any alignment
        let epsilon = target.abs() * 0.01 + 0.01; // 1% tolerance or 0.01 absolute

        for addr in (0xD00000..0xD10000).step_by(1) {
            let bytes = read_bytes_at(emu, addr);
            if let Some(v) = parse_ti_float(&bytes) {
                if (v - target).abs() < epsilon {
                    return Some((addr, v));
                }
            }
        }
        None
    }

    /// Scan memory for what looks like a floating point result
    fn dump_potential_results(emu: &mut Emu, target: f64) {
        println!("\n=== Memory Scan (looking for {}) ===", target);
        for &addr in &ALT_OP_ADDRS {
            let bytes = read_bytes_at(emu, addr);
            let parsed = parse_ti_float(&bytes);
            println!("  0x{:06X}: {} => {:?}", addr, format_op1(&bytes), parsed);
        }

        // Search for the target value
        println!("\n=== Searching RAM for {} ===", target);
        if let Some((addr, v)) = search_for_value(emu, target) {
            println!("  FOUND at 0x{:06X}: {}", addr, v);
            let bytes = read_bytes_at(emu, addr);
            println!("  Bytes: {}", format_op1(&bytes));
        } else {
            println!("  Not found in RAM 0xD00000-0xD10000");
        }

        // Also show any non-zero floats in OP area
        println!("\n=== Non-zero values in OP area ===");
        for offset in (0..0x100).step_by(1) {
            let addr = 0xD005F0 + offset;
            let bytes = read_bytes_at(emu, addr);
            if let Some(v) = parse_ti_float(&bytes) {
                if v != 0.0 {
                    println!("  0x{:06X}: {} => {}", addr, format_op1(&bytes), v);
                }
            }
        }
    }

    #[test]
    #[ignore = "requires ROM file"]
    fn test_simple_number() {
        let mut emu = boot_emulator().expect("Failed to boot emulator");

        let op1 = evaluate_expression(&mut emu, "5");
        println!("OP1 for '5': {}", format_op1(&op1));

        // Scan for results in different locations
        dump_potential_results(&mut emu, 5.0);

        let value = parse_ti_float(&op1);
        println!("\nParsed value at OP1: {:?}", value);

        // Try to find a valid result in any known location
        let mut found_value: Option<f64> = None;
        for &addr in &ALT_OP_ADDRS {
            let bytes = read_bytes_at(&mut emu, addr);
            if let Some(v) = parse_ti_float(&bytes) {
                println!("Found valid float at 0x{:06X}: {}", addr, v);
                found_value = Some(v);
                break;
            }
        }

        assert!(found_value.is_some(), "No valid float found in known locations");
        let v = found_value.unwrap();
        assert!((v - 5.0).abs() < 0.001, "Expected 5, got {}", v);
    }

    #[test]
    #[ignore = "requires ROM file"]
    fn test_addition() {
        let mut emu = boot_emulator().expect("Failed to boot emulator");

        let op1 = evaluate_expression(&mut emu, "2+3");
        println!("OP1 for '2+3': {}", format_op1(&op1));

        let value = parse_ti_float(&op1);
        println!("Parsed value: {:?}", value);

        assert!(value.is_some(), "Failed to parse OP1");
        let v = value.unwrap();
        assert!((v - 5.0).abs() < 0.001, "Expected 5, got {}", v);
    }

    #[test]
    #[ignore = "requires ROM file"]
    fn test_multiplication() {
        let mut emu = boot_emulator().expect("Failed to boot emulator");

        // Search for result both before and after to see what changes
        println!("\n=== Before expression ===");
        if let Some((addr, v)) = search_for_value(&mut emu, 42.0) {
            println!("  Found 42 at 0x{:06X}: {}", addr, v);
        } else {
            println!("  42 not found in RAM");
        }

        let op1 = evaluate_expression(&mut emu, "6*7");
        println!("OP1 for '6*7': {}", format_op1(&op1));

        // Search for 42 after evaluation
        println!("\n=== After expression ===");
        if let Some((addr, v)) = search_for_value(&mut emu, 42.0) {
            println!("  Found 42 at 0x{:06X}: {}", addr, v);
        } else {
            println!("  42 not found in RAM - the multiplication may not have executed");
        }

        let value = parse_ti_float(&op1);
        println!("Parsed value: {:?}", value);

        assert!(value.is_some(), "Failed to parse OP1");
        let v = value.unwrap();
        assert!((v - 42.0).abs() < 0.001, "Expected 42, got {}", v);
    }

    #[test]
    #[ignore = "requires ROM file"]
    fn test_99_times_99() {
        let mut emu = boot_emulator().expect("Failed to boot emulator");

        let op1 = evaluate_expression(&mut emu, "99*99");
        println!("OP1 for '99*99': {}", format_op1(&op1));

        let value = parse_ti_float(&op1);
        println!("Parsed value: {:?}", value);

        assert!(value.is_some(), "Failed to parse OP1");
        let v = value.unwrap();
        assert!((v - 9801.0).abs() < 0.1, "Expected 9801, got {}", v);
    }

    #[test]
    #[ignore = "requires ROM file"]
    fn test_subtraction() {
        let mut emu = boot_emulator().expect("Failed to boot emulator");

        let op1 = evaluate_expression(&mut emu, "10-3");
        println!("OP1 for '10-3': {}", format_op1(&op1));

        let value = parse_ti_float(&op1);
        println!("Parsed value: {:?}", value);

        assert!(value.is_some(), "Failed to parse OP1");
        let v = value.unwrap();
        assert!((v - 7.0).abs() < 0.001, "Expected 7, got {}", v);
    }

    #[test]
    #[ignore = "requires ROM file"]
    fn test_sequential_calculations() {
        let mut emu = boot_emulator().expect("Failed to boot emulator");

        // Test a series of calculations to see if the issue is with specific operations
        let test_cases = [
            ("5", 5.0),
            ("7", 7.0),
            ("2+3", 5.0),
            ("8+1", 9.0),
            ("6*7", 42.0),
            ("9-4", 5.0),
        ];

        for (expr, expected) in test_cases {
            let op1 = evaluate_expression(&mut emu, expr);
            let value = parse_ti_float(&op1).unwrap_or(f64::NAN);
            let pass = (value - expected).abs() < 0.001;
            println!(
                "{} '{}' => {} (expected {}) {}",
                if pass { "✓" } else { "✗" },
                expr,
                value,
                expected,
                if pass { "" } else { "FAIL" }
            );

            // Also search for the expected value if test failed
            if !pass {
                if let Some((addr, v)) = search_for_value(&mut emu, expected) {
                    println!("    (found {} at 0x{:06X})", v, addr);
                }
            }
        }
    }

    #[test]
    #[ignore = "requires ROM file"]
    fn test_division() {
        let mut emu = boot_emulator().expect("Failed to boot emulator");

        let op1 = evaluate_expression(&mut emu, "15/3");
        println!("OP1 for '15/3': {}", format_op1(&op1));

        let value = parse_ti_float(&op1);
        println!("Parsed value: {:?}", value);

        assert!(value.is_some(), "Failed to parse OP1");
        let v = value.unwrap();
        assert!((v - 5.0).abs() < 0.001, "Expected 5, got {}", v);
    }
}
