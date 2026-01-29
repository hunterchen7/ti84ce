//! TI-84 Plus CE Interrupt Controller
//!
//! Memory-mapped at 0xF00000 (port offset 0x100000 from 0xE00000)
//!
//! The controller has 22 interrupt sources. Key sources (from CEmu):
//! - Bit 0: ON key
//! - Bit 1: Timer 1
//! - Bit 2: Timer 2
//! - Bit 3: Timer 3
//! - Bit 4: OS Timer
//! - Bit 10: Keypad (any key in scan mode)
//! - Bit 11: LCD (VBLANK)
//! - Bit 15: Power
//! - Bit 19: Wake (power-on wake signal)

/// Interrupt source bit masks
pub mod sources {
    pub const ON_KEY: u32 = 1 << 0;
    pub const TIMER1: u32 = 1 << 1;
    pub const TIMER2: u32 = 1 << 2;
    pub const TIMER3: u32 = 1 << 3;
    pub const OSTIMER: u32 = 1 << 4;
    pub const KEYPAD: u32 = 1 << 10;
    pub const LCD: u32 = 1 << 11;
    pub const PWR: u32 = 1 << 15;
    pub const WAKE: u32 = 1 << 19;
}

/// Register offsets within the interrupt controller (used in tests)
#[cfg(test)]
mod regs {
    /// Interrupt status/latch (read: status, write: acknowledge)
    pub const STATUS: u32 = 0x00;
    /// Interrupt enable mask
    pub const ENABLED: u32 = 0x04;
    /// Raw interrupt state (before latch)
    pub const RAW: u32 = 0x08;
    /// Latched mode bitmask
    pub const LATCHED: u32 = 0x0C;
}

#[derive(Debug, Clone, Copy)]
struct InterruptBank {
    status: u32,
    enabled: u32,
    latched: u32,
    inverted: u32,
}

/// Interrupt controller for the TI-84 Plus CE
#[derive(Debug, Clone)]
pub struct InterruptController {
    banks: [InterruptBank; 2],
    raw: u32,
}

impl InterruptController {
    /// Create a new interrupt controller
    /// CEmu sets the PWR interrupt (bit 15) immediately on reset
    pub fn new() -> Self {
        let mut controller = Self {
            banks: [
                InterruptBank { status: 0, enabled: 0, latched: 0, inverted: 0 },
                InterruptBank { status: 0, enabled: 0, latched: 0, inverted: 0 },
            ],
            raw: 0,
        };
        controller.raise(sources::PWR);
        controller
    }

    /// Reset the interrupt controller
    /// CEmu sets PWR interrupt after clearing all state
    pub fn reset(&mut self) {
        self.banks = [
            InterruptBank { status: 0, enabled: 0, latched: 0, inverted: 0 },
            InterruptBank { status: 0, enabled: 0, latched: 0, inverted: 0 },
        ];
        self.raw = 0;
        self.raise(sources::PWR);
    }

    /// Check if any enabled interrupt is pending
    pub fn irq_pending(&self) -> bool {
        (self.banks[0].status & self.banks[0].enabled) != 0
            || (self.banks[1].status & self.banks[1].enabled) != 0
    }

    /// Raise an interrupt (set status bit)
    pub fn raise(&mut self, source: u32) {
        self.set_source(source, true);
    }

    /// Clear raw interrupt state (source went inactive)
    pub fn clear_raw(&mut self, source: u32) {
        self.set_source(source, false);
    }

    /// Acknowledge (clear) interrupt status bits
    pub fn acknowledge(&mut self, mask: u32) {
        for bank in &mut self.banks {
            bank.status &= !mask;
        }
    }

    fn set_source(&mut self, mask: u32, set: bool) {
        if set {
            self.raw |= mask;
        } else {
            self.raw &= !mask;
        }

        for bank in &mut self.banks {
            let inverted = bank.inverted & mask;
            if set ^ (inverted != 0) {
                bank.status |= mask;
            } else {
                bank.status &= !mask | bank.latched;
            }
        }
    }

    /// Read a register byte
    /// addr is offset from controller base (0-0x0F)
    pub fn read(&self, addr: u32) -> u8 {
        let index = (addr >> 2) & 0x3F;
        let request = ((addr >> 5) & 0x01) as usize;
        let bit_offset = (addr & 0x03) * 8;
        let bank = &self.banks[request];

        let value = match index {
            0 | 8 => bank.status,
            1 | 9 => bank.enabled,
            2 | 10 => self.raw, // Raw interrupt state
            3 | 11 => bank.latched,
            4 | 12 => bank.inverted,
            5 | 13 => bank.status & bank.enabled,
            20 => 0x00010900,
            21 => {
                if bit_offset & 16 != 0 {
                    0
                } else {
                    22
                }
            }
            _ => 0,
        };

        ((value >> bit_offset) & 0xFF) as u8
    }

    /// Write a register byte
    /// addr is offset from controller base (0-0x0F)
    pub fn write(&mut self, addr: u32, value: u8) {
        let index = (addr >> 2) & 0x3F;
        let request = ((addr >> 5) & 0x01) as usize;
        let bit_offset = (addr & 0x03) * 8;
        let mask = 0xFF_u32 << bit_offset;
        let shifted_value = (value as u32) << bit_offset;

        let bank = &mut self.banks[request];
        match index {
            1 | 9 => {
                bank.enabled = (bank.enabled & !mask) | (shifted_value & mask);
            }
            2 | 10 => {
                bank.status &= !((shifted_value) & bank.latched);
            }
            3 | 11 => {
                bank.latched = (bank.latched & !mask) | (shifted_value & mask);
            }
            4 | 12 => {
                bank.inverted = (bank.inverted & !mask) | (shifted_value & mask);
            }
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
        // CEmu sets PWR interrupt (bit 15) on power-up/reset
        assert_eq!(ic.read(0x01), 0x80); // status byte containing bit 15
        assert_eq!(ic.read(0x04), 0x00); // enabled low byte
        // PWR is set but not enabled, so no IRQ pending
        assert!(!ic.irq_pending());
    }

    #[test]
    fn test_reset() {
        let mut ic = InterruptController::new();
        ic.raise(sources::TIMER1);
        ic.write(0x04, sources::TIMER1 as u8);
        assert!(ic.irq_pending());

        ic.reset();
        // CEmu sets PWR interrupt on reset
        assert_eq!(ic.read(0x01), 0x80);
        assert_eq!(ic.read(0x04), 0x00);
        // PWR is set but not enabled, so no IRQ pending
        assert!(!ic.irq_pending());
    }

    #[test]
    fn test_raise_and_pending() {
        let mut ic = InterruptController::new();

        // Raise interrupt, but it's not enabled
        ic.raise(sources::TIMER1);
        assert!(!ic.irq_pending());

        // Enable the interrupt
        ic.write(0x04, sources::TIMER1 as u8);
        assert!(ic.irq_pending());
    }

    #[test]
    fn test_acknowledge() {
        let mut ic = InterruptController::new();

        ic.raise(sources::TIMER1);
        ic.write(0x04, sources::TIMER1 as u8);
        assert!(ic.irq_pending());

        // Latch and then acknowledge the interrupt
        ic.write(0x0C, sources::TIMER1 as u8);
        ic.write(0x08, sources::TIMER1 as u8);
        assert!(!ic.irq_pending());
    }

    #[test]
    fn test_read_write_enabled() {
        let mut ic = InterruptController::new();

        ic.write(0x04, 0x12);
        assert_eq!(ic.read(0x04), 0x12);

        ic.write(0x05, 0x34);
        assert_eq!(ic.read(0x04), 0x12);
        assert_eq!(ic.read(0x05), 0x34);
    }

    #[test]
    fn test_multiple_sources() {
        let mut ic = InterruptController::new();

        // Enable timer1 and keypad
        ic.write(0x04, sources::TIMER1 as u8);
        // Keypad is bit 10 -> byte 0x05 bit 2
        ic.write(0x05, 0x04);

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

        // Configure TIMER2 as latched so status persists after raw clears
        ic.write(regs::LATCHED, sources::TIMER2 as u8);

        // Raise interrupt - sets both raw and status
        ic.raise(sources::TIMER2);
        assert_eq!(ic.read(regs::RAW), sources::TIMER2 as u8);
        assert_eq!(ic.read(regs::STATUS), sources::TIMER2 as u8);

        // Clear raw state (source went inactive)
        ic.clear_raw(sources::TIMER2);
        assert_eq!(ic.read(regs::RAW), 0);
        // Status should still be latched (because TIMER2 is configured as latched)
        assert_eq!(ic.read(regs::STATUS), sources::TIMER2 as u8);
    }

    #[test]
    fn test_multi_byte_status() {
        let mut ic = InterruptController::new();

        // Raise LCD interrupt (bit 11 - in byte 1)
        ic.raise(sources::LCD);

        // Read byte 0 should be 0 (no interrupts in low byte, PWR is bit 15)
        assert_eq!(ic.read(regs::STATUS), 0);
        // Read byte 1 should have bit 3 set (LCD, bit 11 >> 8 = bit 3)
        // Plus bit 7 set (PWR, bit 15 >> 8 = bit 7)
        let expected = ((sources::LCD >> 8) | (sources::PWR >> 8)) as u8;
        assert_eq!(ic.read(regs::STATUS + 1), expected);
    }

    #[test]
    fn test_on_key_interrupt() {
        let mut ic = InterruptController::new();

        // Enable ON key interrupt via register write
        ic.write(regs::ENABLED, sources::ON_KEY as u8);

        // Raise ON key
        ic.raise(sources::ON_KEY);
        assert!(ic.irq_pending());
        assert_eq!(ic.read(regs::STATUS), sources::ON_KEY as u8);
    }

    #[test]
    fn test_all_timer_sources() {
        let mut ic = InterruptController::new();

        // Enable all timer interrupts via register write
        let mask = sources::TIMER1 | sources::TIMER2 | sources::TIMER3;
        ic.write(regs::ENABLED, mask as u8);

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
