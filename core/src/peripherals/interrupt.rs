//! TI-84 Plus CE Interrupt Controller
//!
//! Memory-mapped at 0xF00000 (port offset 0x100000 from 0xE00000)
//!
//! The controller has 22 interrupt sources. Key sources:
//! - Bit 0: ON key
//! - Bit 1: Timer 1
//! - Bit 2: Timer 2
//! - Bit 3: Timer 3
//! - Bit 4: (reserved)
//! - Bit 5: Keypad (any key in scan mode)
//! - Bit 10: LCD (VBLANK)

/// Interrupt source bit masks
pub mod sources {
    pub const ON_KEY: u32 = 1 << 0;
    pub const TIMER1: u32 = 1 << 1;
    pub const TIMER2: u32 = 1 << 2;
    pub const TIMER3: u32 = 1 << 3;
    pub const KEYPAD: u32 = 1 << 5;
    pub const LCD: u32 = 1 << 10;
}

/// Register offsets within the interrupt controller
mod regs {
    /// Interrupt status/latch (read: status, write: acknowledge)
    pub const STATUS: u32 = 0x00;
    /// Interrupt enable mask
    pub const ENABLED: u32 = 0x04;
    /// Raw interrupt state (before latch)
    pub const RAW: u32 = 0x08;
}

/// Interrupt controller for the TI-84 Plus CE
#[derive(Debug, Clone)]
pub struct InterruptController {
    /// Latched interrupt status (must be acknowledged to clear)
    status: u32,
    /// Interrupt enable mask
    enabled: u32,
    /// Raw interrupt state (direct from sources)
    raw: u32,
}

impl InterruptController {
    /// Create a new interrupt controller
    pub fn new() -> Self {
        Self {
            status: 0,
            enabled: 0,
            raw: 0,
        }
    }

    /// Reset the interrupt controller
    pub fn reset(&mut self) {
        self.status = 0;
        self.enabled = 0;
        self.raw = 0;
    }

    /// Check if any enabled interrupt is pending
    pub fn irq_pending(&self) -> bool {
        (self.status & self.enabled) != 0
    }

    /// Raise an interrupt (set status bit)
    pub fn raise(&mut self, source: u32) {
        self.raw |= source;
        self.status |= source;
    }

    /// Clear raw interrupt state (source went inactive)
    pub fn clear_raw(&mut self, source: u32) {
        self.raw &= !source;
    }

    /// Acknowledge (clear) interrupt status bits
    pub fn acknowledge(&mut self, mask: u32) {
        self.status &= !mask;
    }

    /// Read a register byte
    /// addr is offset from controller base (0-0x0F)
    pub fn read(&self, addr: u32) -> u8 {
        let reg = addr & 0x0C; // Align to 4-byte register
        let byte_offset = (addr & 0x03) * 8;

        let value = match reg {
            regs::STATUS => self.status,
            regs::ENABLED => self.enabled,
            regs::RAW => self.raw,
            _ => 0,
        };

        ((value >> byte_offset) & 0xFF) as u8
    }

    /// Write a register byte
    /// addr is offset from controller base (0-0x0F)
    pub fn write(&mut self, addr: u32, value: u8) {
        let reg = addr & 0x0C;
        let byte_offset = (addr & 0x03) * 8;
        let mask = 0xFF_u32 << byte_offset;
        let shifted_value = (value as u32) << byte_offset;

        match reg {
            regs::STATUS => {
                // Writing to status acknowledges (clears) those bits
                self.status &= !(shifted_value & mask);
            }
            regs::ENABLED => {
                self.enabled = (self.enabled & !mask) | (shifted_value & mask);
            }
            // RAW is read-only
            _ => {}
        }
    }
}

impl Default for InterruptController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let ic = InterruptController::new();
        assert_eq!(ic.status, 0);
        assert_eq!(ic.enabled, 0);
        assert!(!ic.irq_pending());
    }

    #[test]
    fn test_reset() {
        let mut ic = InterruptController::new();
        ic.raise(sources::TIMER1);
        ic.enabled = sources::TIMER1;
        assert!(ic.irq_pending());

        ic.reset();
        assert_eq!(ic.status, 0);
        assert_eq!(ic.enabled, 0);
        assert_eq!(ic.raw, 0);
        assert!(!ic.irq_pending());
    }

    #[test]
    fn test_raise_and_pending() {
        let mut ic = InterruptController::new();

        // Raise interrupt, but it's not enabled
        ic.raise(sources::TIMER1);
        assert!(!ic.irq_pending());

        // Enable the interrupt
        ic.write(regs::ENABLED, sources::TIMER1 as u8);
        assert!(ic.irq_pending());
    }

    #[test]
    fn test_acknowledge() {
        let mut ic = InterruptController::new();

        ic.raise(sources::TIMER1);
        ic.write(regs::ENABLED, sources::TIMER1 as u8);
        assert!(ic.irq_pending());

        // Acknowledge the interrupt
        ic.write(regs::STATUS, sources::TIMER1 as u8);
        assert!(!ic.irq_pending());
    }

    #[test]
    fn test_read_write_enabled() {
        let mut ic = InterruptController::new();

        ic.write(regs::ENABLED, 0x12);
        assert_eq!(ic.read(regs::ENABLED), 0x12);

        ic.write(regs::ENABLED + 1, 0x34);
        assert_eq!(ic.read(regs::ENABLED), 0x12);
        assert_eq!(ic.read(regs::ENABLED + 1), 0x34);
    }

    #[test]
    fn test_multiple_sources() {
        let mut ic = InterruptController::new();

        // Enable timer1 and keypad
        ic.enabled = sources::TIMER1 | sources::KEYPAD;

        // Raise only timer1
        ic.raise(sources::TIMER1);
        assert!(ic.irq_pending());

        // Acknowledge timer1, no more pending
        ic.acknowledge(sources::TIMER1);
        assert!(!ic.irq_pending());

        // Raise keypad
        ic.raise(sources::KEYPAD);
        assert!(ic.irq_pending());
    }

    #[test]
    fn test_raw_state() {
        let mut ic = InterruptController::new();

        // Raise interrupt - sets both raw and status
        ic.raise(sources::TIMER2);
        assert_eq!(ic.read(regs::RAW), sources::TIMER2 as u8);
        assert_eq!(ic.read(regs::STATUS), sources::TIMER2 as u8);

        // Clear raw state (source went inactive)
        ic.clear_raw(sources::TIMER2);
        assert_eq!(ic.read(regs::RAW), 0);
        // Status should still be latched
        assert_eq!(ic.read(regs::STATUS), sources::TIMER2 as u8);
    }

    #[test]
    fn test_multi_byte_status() {
        let mut ic = InterruptController::new();

        // Raise LCD interrupt (bit 10 - in byte 1)
        ic.raise(sources::LCD);

        // Read byte 0 should be 0
        assert_eq!(ic.read(regs::STATUS), 0);
        // Read byte 1 should have bit 2 set (bit 10 >> 8 = bit 2)
        assert_eq!(ic.read(regs::STATUS + 1), (sources::LCD >> 8) as u8);
    }

    #[test]
    fn test_on_key_interrupt() {
        let mut ic = InterruptController::new();

        // Enable ON key interrupt
        ic.enabled = sources::ON_KEY;

        // Raise ON key
        ic.raise(sources::ON_KEY);
        assert!(ic.irq_pending());
        assert_eq!(ic.read(regs::STATUS), sources::ON_KEY as u8);
    }

    #[test]
    fn test_all_timer_sources() {
        let mut ic = InterruptController::new();

        // Enable all timer interrupts
        ic.enabled = sources::TIMER1 | sources::TIMER2 | sources::TIMER3;

        // Raise timer 3
        ic.raise(sources::TIMER3);
        assert!(ic.irq_pending());

        // Acknowledge timer 3
        ic.acknowledge(sources::TIMER3);
        assert!(!ic.irq_pending());

        // Raise all timers
        ic.raise(sources::TIMER1);
        ic.raise(sources::TIMER2);
        ic.raise(sources::TIMER3);

        // Acknowledge only timer 1
        ic.acknowledge(sources::TIMER1);
        assert!(ic.irq_pending()); // Timer 2 and 3 still pending

        // Acknowledge timer 2 and 3
        ic.acknowledge(sources::TIMER2 | sources::TIMER3);
        assert!(!ic.irq_pending());
    }
}
