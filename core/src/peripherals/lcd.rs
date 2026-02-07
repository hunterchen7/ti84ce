//! TI-84 Plus CE LCD Controller
//!
//! Memory-mapped at 0xE30000 (port offset 0x030000 from 0xE00000)
//!
//! The LCD controller manages the display timing and points to VRAM.
//! VRAM is typically at 0xD40000 and contains 320x240 RGB565 pixels.
//!
//! Register map matches CEmu's lcd.c:
//! - 0x000-0x00F: Timing registers
//! - 0x010-0x013: UPBASE (upper panel base address)
//! - 0x014-0x017: LPBASE (lower panel base address)
//! - 0x018-0x01B: Control register
//! - 0x01C: IMSC (interrupt mask, byte only, bit 0 reserved for cursor)
//! - 0x020: RIS (raw interrupt status, byte only, bit 0 reserved for cursor)
//! - 0x024: MIS (masked interrupt status = imsc & ris, read-only)
//! - 0x028: ICR (interrupt clear register, write-only)
//! - 0x02C-0x02F: UPCURR (upper current address, read-only)
//! - 0x030-0x033: LPCURR (lower current address, read-only)
//! - 0x200-0x3FF: Palette (256 entries × 2 bytes)

/// Display dimensions
pub const LCD_WIDTH: usize = 320;
pub const LCD_HEIGHT: usize = 240;

/// Default VRAM base address
pub const DEFAULT_VRAM_BASE: u32 = 0xD40000;

/// Cycles per frame at ~60Hz with 48MHz clock
/// 48_000_000 / 60 = 800_000 cycles
const CYCLES_PER_FRAME: u32 = 800_000;

/// Register offsets matching CEmu lcd.c
mod regs {
    // TODO: Used when LCD timing is implemented (Milestone 6C)
    pub const _TIMING0: u32 = 0x00;
    pub const _TIMING1: u32 = 0x04;
    pub const _TIMING2: u32 = 0x08;
    pub const _TIMING3: u32 = 0x0C;
    pub const UPBASE: u32 = 0x10;
    pub const LPBASE: u32 = 0x14;
    pub const CONTROL: u32 = 0x18;
    pub const IMSC: u32 = 0x1C;
    pub const RIS: u32 = 0x20;
    pub const MIS: u32 = 0x24;
    pub const ICR: u32 = 0x28;
    pub const UPCURR: u32 = 0x2C;
    pub const LPCURR: u32 = 0x30;
    /// Palette starts at offset 0x200
    pub const PALETTE_START: u32 = 0x200;
    pub const PALETTE_END: u32 = 0x400;
}

/// Control register bits
#[allow(dead_code)]
mod ctrl {
    /// LCD enable
    pub const ENABLE: u32 = 1 << 0;
    /// Bits per pixel (bits 1-3)
    pub const BPP_SHIFT: u32 = 1;
    pub const BPP_MASK: u32 = 0x07;
    /// BGR swap (bit 8)
    pub const BGR: u32 = 1 << 8;
    /// LCD power enable
    pub const PWR: u32 = 1 << 11;
}

/// Peripheral ID bytes (CEmu lcd_read at 0xFE0)
const LCD_PERIPH_ID: [u8; 8] = [0x11, 0x11, 0x14, 0x00, 0x0D, 0xF0, 0x05, 0xB1];

/// LCD Controller
#[derive(Debug, Clone)]
pub struct LcdController {
    /// Timing registers
    timing: [u32; 4],
    /// Control register
    control: u32,
    /// Interrupt mask set/clear (byte, bit 0 reserved for cursor)
    imsc: u8,
    /// Raw interrupt status (byte, bit 0 reserved for cursor)
    ris: u8,
    /// Upper panel base address (VRAM)
    upbase: u32,
    /// Lower panel base address
    lpbase: u32,
    /// Upper panel current address (DMA progress)
    upcurr: u32,
    /// Lower panel current address
    lpcurr: u32,
    /// 256-entry color palette (stored as raw bytes, 2 bytes per entry)
    palette: [u8; 512],
    /// Cycle accumulator for frame timing
    frame_cycles: u32,
}

impl LcdController {
    /// Create a new LCD controller
    pub fn new() -> Self {
        Self {
            timing: [0; 4],
            control: 0,
            imsc: 0,
            ris: 0,
            upbase: DEFAULT_VRAM_BASE,
            lpbase: 0,
            upcurr: 0,
            lpcurr: 0,
            palette: [0; 512],
            frame_cycles: 0,
        }
    }

    /// Reset the LCD controller (matches CEmu lcd_reset)
    pub fn reset(&mut self) {
        self.timing = [0; 4];
        self.control = 0;
        self.imsc = 0;
        self.ris = 0;
        self.upbase = DEFAULT_VRAM_BASE;
        self.lpbase = 0;
        self.upcurr = 0;
        self.lpcurr = 0;
        self.palette = [0; 512];
        self.frame_cycles = 0;
    }

    /// Check if LCD is enabled (bit 0)
    pub fn is_enabled(&self) -> bool {
        self.control & ctrl::ENABLE != 0
    }

    /// Check if LCD power is on (bit 11)
    pub fn is_powered(&self) -> bool {
        self.control & ctrl::PWR != 0
    }

    /// Get control register
    pub fn control(&self) -> u32 {
        self.control
    }

    /// Get interrupt mask
    pub fn int_mask(&self) -> u32 {
        self.imsc as u32
    }

    /// Get interrupt status
    pub fn int_status(&self) -> u32 {
        self.ris as u32
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

    /// Get current frame cycle accumulator
    pub fn frame_cycles(&self) -> u32 {
        self.frame_cycles
    }

    /// Get BPP mode from control register (bits 1-3)
    pub fn bpp_mode(&self) -> u8 {
        ((self.control >> ctrl::BPP_SHIFT) & ctrl::BPP_MASK) as u8
    }

    /// Check interrupt and return whether LCD interrupt should be active
    fn check_interrupt(&self) -> bool {
        (self.ris & self.imsc) != 0
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
            // Set VBLANK interrupt status bit (bit 3 in CEmu is LCD_VERT_COMP)
            // We use bit 0 for the simple vblank for now
            self.ris |= 1 << 3;
            return self.check_interrupt();
        }

        self.frame_cycles = total_cycles as u32;

        false
    }

    /// Read a register byte
    /// addr is offset from controller base (0x000-0xFFF)
    pub fn read(&self, addr: u32) -> u8 {
        let index = addr & 0xFFF;
        let bit_offset = (index & 3) << 3;

        if index < 0x200 {
            // Register space
            let reg = index & 0xFFC;
            match reg {
                0x00..=0x0C => {
                    let val = self.timing[(reg >> 2) as usize];
                    ((val >> bit_offset) & 0xFF) as u8
                }
                regs::UPBASE => ((self.upbase >> bit_offset) & 0xFF) as u8,
                regs::LPBASE => ((self.lpbase >> bit_offset) & 0xFF) as u8,
                regs::CONTROL => ((self.control >> bit_offset) & 0xFF) as u8,
                // CEmu: imsc & ~1 (bit 0 excluded from normal mask register)
                regs::IMSC if bit_offset == 0 => self.imsc & !1,
                // CEmu: ris & ~1
                regs::RIS if bit_offset == 0 => self.ris & !1,
                // CEmu: imsc & ris & ~1
                regs::MIS if bit_offset == 0 => self.imsc & self.ris & !1,
                // UPCURR
                regs::UPCURR => ((self.upcurr >> bit_offset) & 0xFF) as u8,
                // LPCURR
                regs::LPCURR => ((self.lpcurr >> bit_offset) & 0xFF) as u8,
                _ => 0,
            }
        } else if index < regs::PALETTE_END {
            // Palette read (0x200-0x3FF)
            // CEmu adds 1 cycle penalty for palette reads
            let palette_idx = (index - regs::PALETTE_START) as usize;
            self.palette[palette_idx]
        } else if index >= 0xFE0 {
            // Peripheral ID
            let id_idx = ((index - 0xFE0) >> 2) as usize;
            if id_idx < LCD_PERIPH_ID.len() {
                LCD_PERIPH_ID[id_idx]
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Write a register byte
    /// addr is offset from controller base (0x000-0xFFF)
    pub fn write(&mut self, addr: u32, value: u8) {
        let index = addr & 0xFFF;
        let bit_offset = (index & 3) << 3;

        if index < 0x200 {
            let reg = index & 0xFFC;
            let mask = 0xFF_u32 << bit_offset;
            let shifted = (value as u32) << bit_offset;

            match reg {
                // Timing registers
                0x00..=0x0C => {
                    let i = (reg >> 2) as usize;
                    self.timing[i] = (self.timing[i] & !mask) | (shifted & mask);
                }
                // UPBASE — CEmu aligns to 8 bytes
                regs::UPBASE => {
                    self.upbase = (self.upbase & !mask) | (shifted & mask);
                    self.upbase &= !7;
                }
                // LPBASE — CEmu aligns to 8 bytes
                regs::LPBASE => {
                    self.lpbase = (self.lpbase & !mask) | (shifted & mask);
                    self.lpbase &= !7;
                }
                // Control register
                regs::CONTROL => {
                    self.control = (self.control & !mask) | (shifted & mask);
                }
                // IMSC — byte write only at offset 0, bits [4:1] only
                regs::IMSC if bit_offset == 0 => {
                    self.imsc &= !0x1E;
                    self.imsc |= value & 0x1E;
                }
                // ICR — interrupt clear, write clears bits in ris
                regs::ICR if bit_offset == 0 => {
                    self.ris &= !(value & 0x1E);
                }
                _ => {}
            }
        } else if index < regs::PALETTE_END {
            // Palette write (0x200-0x3FF)
            let palette_idx = (index - regs::PALETTE_START) as usize;
            self.palette[palette_idx] = value;
        }
    }

    // ========== State Persistence ==========

    /// Set control register directly
    pub fn set_control(&mut self, value: u32) {
        self.control = value;
    }

    /// Set upbase directly
    pub fn set_upbase(&mut self, value: u32) {
        self.upbase = value;
    }

    /// Set interrupt mask directly
    pub fn set_int_mask(&mut self, value: u32) {
        self.imsc = value as u8;
    }

    /// Set interrupt status directly
    pub fn set_int_status(&mut self, value: u32) {
        self.ris = value as u8;
    }

    /// Set frame cycles directly
    pub fn set_frame_cycles(&mut self, value: u32) {
        self.frame_cycles = value;
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
        lcd.imsc = 0x1E;
        lcd.frame_cycles = 500000;

        lcd.reset();
        assert!(!lcd.is_enabled());
        assert_eq!(lcd.upbase(), DEFAULT_VRAM_BASE);
        assert_eq!(lcd.imsc, 0);
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
        lcd.imsc = 0x08; // Enable vert comp interrupt (bit 3)

        let irq = lcd.tick(CYCLES_PER_FRAME);
        assert!(!irq);
        assert_eq!(lcd.frame_cycles, 0);
        assert_eq!(lcd.ris & 0x08, 0);
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
    fn test_upbase_alignment() {
        let mut lcd = LcdController::new();
        // Write non-aligned address — should be forced to 8-byte alignment
        lcd.write(regs::UPBASE, 0x07); // low byte 0x07
        assert_eq!(lcd.upbase & 0x07, 0); // Should be cleared
    }

    #[test]
    fn test_icr_clears_ris() {
        let mut lcd = LcdController::new();
        lcd.ris = 0x1E; // All interrupt bits set

        // Write ICR to clear bit 3 (vert comp)
        lcd.write(regs::ICR, 0x08);
        assert_eq!(lcd.ris, 0x16); // Bit 3 cleared

        // Write ICR to clear remaining bits
        lcd.write(regs::ICR, 0x16);
        assert_eq!(lcd.ris, 0x00);
    }

    #[test]
    fn test_imsc_masks_bit0() {
        let mut lcd = LcdController::new();

        // Write IMSC with all bits including bit 0
        lcd.write(regs::IMSC, 0x1F);
        // Bit 0 should NOT be set (reserved for cursor)
        assert_eq!(lcd.imsc, 0x1E);

        // Reading should also mask bit 0
        assert_eq!(lcd.read(regs::IMSC), 0x1E);
    }

    #[test]
    fn test_ris_read_masks_bit0() {
        let mut lcd = LcdController::new();
        lcd.ris = 0x1F; // Set all bits including reserved bit 0

        // Read should mask bit 0
        assert_eq!(lcd.read(regs::RIS), 0x1E);
    }

    #[test]
    fn test_mis_read() {
        let mut lcd = LcdController::new();
        lcd.imsc = 0x08; // Mask for bit 3 only
        lcd.ris = 0x1E; // All bits set

        // MIS = imsc & ris & ~1
        assert_eq!(lcd.read(regs::MIS), 0x08);
    }

    #[test]
    fn test_vblank_interrupt() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.imsc = 0x08; // Enable vert comp interrupt (bit 3)

        // Tick less than a frame
        let irq = lcd.tick(100);
        assert!(!irq);

        // Tick to complete a frame
        let irq = lcd.tick(CYCLES_PER_FRAME);
        assert!(irq);
        assert_eq!(lcd.ris & 0x08, 0x08);

        // Clear interrupt via ICR
        lcd.write(regs::ICR, 0x08);
        assert_eq!(lcd.ris & 0x08, 0);
    }

    #[test]
    fn test_multiple_frames() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.imsc = 0x08;

        let mut interrupt_count = 0;

        // Run for 3 frames
        for _ in 0..(CYCLES_PER_FRAME * 3) {
            if lcd.tick(1) {
                interrupt_count += 1;
                lcd.write(regs::ICR, 0x08);
            }
        }

        assert_eq!(interrupt_count, 3);
    }

    #[test]
    fn test_vblank_no_interrupt_when_masked() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.imsc = 0; // Interrupt NOT enabled

        let irq = lcd.tick(CYCLES_PER_FRAME);
        assert!(!irq);
        // But ris should still be set
        assert_eq!(lcd.ris & 0x08, 0x08);
    }

    #[test]
    fn test_vblank_exact_boundary() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.imsc = 0x08;

        let irq = lcd.tick(CYCLES_PER_FRAME - 1);
        assert!(!irq);
        assert_eq!(lcd.frame_cycles, CYCLES_PER_FRAME - 1);

        let irq = lcd.tick(1);
        assert!(irq);
        assert_eq!(lcd.frame_cycles, 0);
    }

    #[test]
    fn test_tick_larger_than_frame() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.imsc = 0x08;

        let irq = lcd.tick(CYCLES_PER_FRAME * 2 + 100);
        assert!(irq);
        assert_eq!(lcd.frame_cycles, 100);
    }

    #[test]
    fn test_tick_multiple_frames_remainder_and_status() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.imsc = 0x08;

        let irq = lcd.tick(CYCLES_PER_FRAME * 3 + 5);
        assert!(irq);
        assert_eq!(lcd.ris & 0x08, 0x08);
        assert_eq!(lcd.frame_cycles, 5);
    }

    #[test]
    fn test_tick_multiple_frames_masked_sets_status() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.imsc = 0;

        let irq = lcd.tick(CYCLES_PER_FRAME * 2 + 7);
        assert!(!irq);
        assert_eq!(lcd.ris & 0x08, 0x08);
        assert_eq!(lcd.frame_cycles, 7);
    }

    #[test]
    fn test_palette_read_write() {
        let mut lcd = LcdController::new();

        // Write palette entry 0 (offset 0x200-0x201)
        lcd.write(0x200, 0xAB);
        lcd.write(0x201, 0xCD);

        assert_eq!(lcd.read(0x200), 0xAB);
        assert_eq!(lcd.read(0x201), 0xCD);

        // Write palette entry 255 (offset 0x3FE-0x3FF)
        lcd.write(0x3FE, 0x12);
        lcd.write(0x3FF, 0x34);

        assert_eq!(lcd.read(0x3FE), 0x12);
        assert_eq!(lcd.read(0x3FF), 0x34);
    }

    #[test]
    fn test_periph_id() {
        let lcd = LcdController::new();
        assert_eq!(lcd.read(0xFE0), 0x11);
        assert_eq!(lcd.read(0xFE4), 0x11);
        assert_eq!(lcd.read(0xFE8), 0x14);
        assert_eq!(lcd.read(0xFEC), 0x00);
        assert_eq!(lcd.read(0xFF0), 0x0D);
        assert_eq!(lcd.read(0xFF4), 0xF0);
        assert_eq!(lcd.read(0xFF8), 0x05);
        assert_eq!(lcd.read(0xFFC), 0xB1);
    }
}
