//! TI-84 Plus CE LCD Controller
//!
//! Memory-mapped at 0xE30000 (port offset 0x030000 from 0xE00000)
//!
//! The LCD controller manages the display timing and points to VRAM.
//! VRAM is typically at 0xD40000 and contains 320x240 RGB565 pixels.

/// Display dimensions
pub const LCD_WIDTH: usize = 320;
pub const LCD_HEIGHT: usize = 240;

/// Default VRAM base address
pub const DEFAULT_VRAM_BASE: u32 = 0xD40000;

/// Cycles per frame at ~60Hz with 48MHz clock
/// 48_000_000 / 60 = 800_000 cycles
const CYCLES_PER_FRAME: u32 = 800_000;

/// Register offsets (based on CEmu lcd.c)
mod regs {
    /// Horizontal timing
    pub const TIMING0: u32 = 0x00;
    /// Vertical timing
    pub const TIMING1: u32 = 0x04;
    /// Timing 2
    pub const TIMING2: u32 = 0x08;
    /// Timing 3
    pub const TIMING3: u32 = 0x0C;
    /// Upper panel base address (VRAM pointer)
    pub const UPBASE: u32 = 0x10;
    /// Lower panel base address (unused on TI-84 CE)
    pub const LPBASE: u32 = 0x14;
    /// LCD control register
    pub const CONTROL: u32 = 0x18;
    /// Interrupt mask (single byte at 0x1C)
    pub const INT_MASK: u32 = 0x1C;
    /// Raw interrupt status (single byte at 0x20)
    pub const INT_STATUS: u32 = 0x20;
    /// Palette base address
    pub const PALBASE: u32 = 0x28;
}

/// Control register bits (kept for documentation/future use)
#[allow(dead_code)]
mod ctrl {
    /// LCD enable
    pub const ENABLE: u32 = 1 << 0;
    /// Bits per pixel (bits 1-3)
    pub const BPP_SHIFT: u32 = 1;
    pub const BPP_MASK: u32 = 0x07;
    /// LCD power enable
    pub const PWR: u32 = 1 << 11;
}

/// LCD Controller
#[derive(Debug, Clone)]
pub struct LcdController {
    /// Timing registers
    timing: [u32; 4],
    /// Control register
    control: u32,
    /// Interrupt mask
    int_mask: u32,
    /// Interrupt status
    int_status: u32,
    /// Upper panel base address (VRAM)
    upbase: u32,
    /// Lower panel base address
    lpbase: u32,
    /// Palette base address
    palbase: u32,
    /// Cycle accumulator for frame timing
    frame_cycles: u32,
}

impl LcdController {
    /// Create a new LCD controller
    pub fn new() -> Self {
        Self {
            timing: [0; 4],
            control: 0,
            int_mask: 0,
            int_status: 0,
            upbase: DEFAULT_VRAM_BASE,
            lpbase: 0,
            palbase: 0,
            frame_cycles: 0,
        }
    }

    /// Reset the LCD controller
    pub fn reset(&mut self) {
        self.timing = [0; 4];
        self.control = 0;
        self.int_mask = 0;
        self.int_status = 0;
        self.upbase = DEFAULT_VRAM_BASE;
        self.lpbase = 0;
        self.palbase = 0;
        self.frame_cycles = 0;
    }

    /// Check if LCD is enabled
    pub fn is_enabled(&self) -> bool {
        self.control & ctrl::ENABLE != 0
    }

    /// Get control register
    pub fn control(&self) -> u32 {
        self.control
    }

    /// Get interrupt mask
    pub fn int_mask(&self) -> u32 {
        self.int_mask
    }

    /// Get interrupt status
    pub fn int_status(&self) -> u32 {
        self.int_status
    }

    /// Get timing registers
    pub fn timing(&self) -> [u32; 4] {
        self.timing
    }

    /// Get VRAM base address
    pub fn upbase(&self) -> u32 {
        self.upbase
    }

    /// Get lower panel base address
    pub fn lpbase(&self) -> u32 {
        self.lpbase
    }

    /// Get palette base address
    pub fn palbase(&self) -> u32 {
        self.palbase
    }

    /// Get current frame cycle accumulator
    pub fn frame_cycles(&self) -> u32 {
        self.frame_cycles
    }

    /// Tick the LCD controller
    /// Returns true if VBLANK interrupt should fire
    pub fn tick(&mut self, cycles: u32) -> bool {
        if !self.is_enabled() {
            return false;
        }

        let total_cycles = self.frame_cycles as u64 + cycles as u64;
        if total_cycles >= CYCLES_PER_FRAME as u64 {
            self.frame_cycles = (total_cycles % CYCLES_PER_FRAME as u64) as u32;
            // Set VBLANK interrupt status (one bit regardless of frames elapsed)
            self.int_status |= 1;
            // Return true if interrupt is enabled
            return (self.int_mask & 1) != 0;
        }

        self.frame_cycles = total_cycles as u32;

        false
    }

    /// Read a register byte
    /// addr is offset from controller base (0-0xFF)
    pub fn read(&self, addr: u32) -> u8 {
        let reg = addr & 0xFC;
        let byte_offset = (addr & 0x03) * 8;

        let value = match reg {
            regs::TIMING0 => self.timing[0],
            regs::TIMING1 => self.timing[1],
            regs::TIMING2 => self.timing[2],
            regs::TIMING3 => self.timing[3],
            regs::CONTROL => self.control,
            regs::INT_MASK => self.int_mask,
            regs::INT_STATUS => self.int_status,
            regs::UPBASE => self.upbase,
            regs::LPBASE => self.lpbase,
            regs::PALBASE => self.palbase,
            _ => 0,
        };

        ((value >> byte_offset) & 0xFF) as u8
    }

    /// Write a register byte
    /// addr is offset from controller base (0-0xFF)
    pub fn write(&mut self, addr: u32, value: u8) {
        let reg = addr & 0xFC;
        let byte_offset = (addr & 0x03) * 8;
        let mask = 0xFF_u32 << byte_offset;
        let shifted_value = (value as u32) << byte_offset;

        match reg {
            regs::TIMING0 => {
                self.timing[0] = (self.timing[0] & !mask) | (shifted_value & mask);
            }
            regs::TIMING1 => {
                self.timing[1] = (self.timing[1] & !mask) | (shifted_value & mask);
            }
            regs::TIMING2 => {
                self.timing[2] = (self.timing[2] & !mask) | (shifted_value & mask);
            }
            regs::TIMING3 => {
                self.timing[3] = (self.timing[3] & !mask) | (shifted_value & mask);
            }
            regs::CONTROL => {
                self.control = (self.control & !mask) | (shifted_value & mask);
            }
            regs::INT_MASK => {
                self.int_mask = (self.int_mask & !mask) | (shifted_value & mask);
            }
            regs::INT_STATUS => {
                // Writing clears interrupt status bits
                self.int_status &= !(shifted_value & mask);
            }
            regs::UPBASE => {
                self.upbase = (self.upbase & !mask) | (shifted_value & mask);
            }
            regs::LPBASE => {
                self.lpbase = (self.lpbase & !mask) | (shifted_value & mask);
            }
            regs::PALBASE => {
                self.palbase = (self.palbase & !mask) | (shifted_value & mask);
            }
            _ => {}
        }
    }
}

impl Default for LcdController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let lcd = LcdController::new();
        assert!(!lcd.is_enabled());
        assert_eq!(lcd.upbase(), DEFAULT_VRAM_BASE);
    }

    #[test]
    fn test_reset() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.upbase = 0xD50000;
        lcd.int_mask = 1;
        lcd.frame_cycles = 500000;

        lcd.reset();
        assert!(!lcd.is_enabled());
        assert_eq!(lcd.upbase(), DEFAULT_VRAM_BASE);
        assert_eq!(lcd.int_mask, 0);
        assert_eq!(lcd.frame_cycles, 0);
    }

    #[test]
    fn test_enable() {
        let mut lcd = LcdController::new();
        lcd.write(regs::CONTROL, ctrl::ENABLE as u8);
        assert!(lcd.is_enabled());
    }

    #[test]
    fn test_disabled_no_interrupt() {
        let mut lcd = LcdController::new();
        lcd.int_mask = 1; // Enable VBLANK interrupt
        // LCD is disabled by default

        // Tick a full frame - should not fire interrupt
        let irq = lcd.tick(CYCLES_PER_FRAME);
        assert!(!irq);
        assert_eq!(lcd.frame_cycles, 0);
        assert_eq!(lcd.int_status & 1, 0);
    }

    #[test]
    fn test_upbase_write() {
        let mut lcd = LcdController::new();

        // Write new VRAM address 0xD50000
        lcd.write(regs::UPBASE, 0x00);
        lcd.write(regs::UPBASE + 1, 0x00);
        lcd.write(regs::UPBASE + 2, 0xD5);

        assert_eq!(lcd.upbase(), 0xD50000);
    }

    #[test]
    fn test_vblank_interrupt() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.int_mask = 1; // Enable VBLANK interrupt

        // Tick less than a frame
        let irq = lcd.tick(100);
        assert!(!irq);

        // Tick to complete a frame
        let irq = lcd.tick(CYCLES_PER_FRAME);
        assert!(irq);
        assert_eq!(lcd.int_status & 1, 1);

        // Clear interrupt
        lcd.write(regs::INT_STATUS, 1);
        assert_eq!(lcd.int_status & 1, 0);
    }

    #[test]
    fn test_multiple_frames() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.int_mask = 1;

        let mut interrupt_count = 0;

        // Run for 3 frames
        for _ in 0..(CYCLES_PER_FRAME * 3) {
            if lcd.tick(1) {
                interrupt_count += 1;
                // Clear the interrupt
                lcd.write(regs::INT_STATUS, 1);
            }
        }

        assert_eq!(interrupt_count, 3);
    }

    #[test]
    fn test_vblank_no_interrupt_when_masked() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.int_mask = 0; // Interrupt NOT enabled

        // Complete a frame
        let irq = lcd.tick(CYCLES_PER_FRAME);
        assert!(!irq); // No IRQ returned
        // But status should still be set
        assert_eq!(lcd.int_status & 1, 1);
    }

    #[test]
    fn test_vblank_exact_boundary() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.int_mask = 1;

        // Tick to exactly 1 cycle before frame end
        let irq = lcd.tick(CYCLES_PER_FRAME - 1);
        assert!(!irq);
        assert_eq!(lcd.frame_cycles, CYCLES_PER_FRAME - 1);

        // One more cycle completes the frame
        let irq = lcd.tick(1);
        assert!(irq);
        assert_eq!(lcd.frame_cycles, 0); // Reset after frame
    }

    #[test]
    fn test_tick_larger_than_frame() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.int_mask = 1;

        // Tick more than one frame at once - only one interrupt per tick
        let irq = lcd.tick(CYCLES_PER_FRAME * 2 + 100);
        assert!(irq);
        // frame_cycles should be the remainder within the current frame
        // (CYCLES_PER_FRAME * 2 + 100) % CYCLES_PER_FRAME = 100
        assert_eq!(lcd.frame_cycles, 100);
    }

    #[test]
    fn test_tick_multiple_frames_remainder_and_status() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.int_mask = 1;

        let irq = lcd.tick(CYCLES_PER_FRAME * 3 + 5);
        assert!(irq);
        assert_eq!(lcd.int_status & 1, 1);
        assert_eq!(lcd.frame_cycles, 5);
    }

    #[test]
    fn test_tick_multiple_frames_masked_sets_status() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.int_mask = 0;

        let irq = lcd.tick(CYCLES_PER_FRAME * 2 + 7);
        assert!(!irq);
        assert_eq!(lcd.int_status & 1, 1);
        assert_eq!(lcd.frame_cycles, 7);
    }

}
