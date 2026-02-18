//! TI-84 Plus CE LCD Controller
//!
//! Memory-mapped at 0xE30000 (port offset 0x030000 from 0xE00000)
//!
//! The LCD controller manages the display timing and DMA from VRAM.
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
//! - 0x200-0x3FF: Palette (256 entries x 2 bytes)
//!
//! DMA state machine (CEmu lcd_event / lcd_dma):
//!   FRONT_PORCH -> SYNC -> LNBU -> BACK_PORCH -> ACTIVE_VIDEO -> FRONT_PORCH
//!
//! LCD event uses CLOCK_24M; LCD DMA uses CLOCK_48M.

/// Display dimensions
pub const LCD_WIDTH: usize = 320;
pub const LCD_HEIGHT: usize = 240;

/// Default VRAM base address
pub const DEFAULT_VRAM_BASE: u32 = 0xD40000;

/// Register offsets matching CEmu lcd.c
mod regs {
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

/// LCD DMA state machine compare states (matches CEmu lcd_comp enum)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LcdCompare {
    FrontPorch = 0,
    Sync = 1,
    Lnbu = 2,
    BackPorch = 3,
    ActiveVideo = 4,
}

/// Result from process_event: duration for next LCD event, optional DMA scheduling info
pub struct LcdEventResult {
    /// Duration (in Clock24M ticks) until next LCD event
    pub duration: u64,
    /// If Some, schedule LcdDma relative to this LCD event with given offset (in LCD clock ticks)
    pub schedule_dma_offset: Option<u64>,
    /// Whether interrupt state changed (caller should update interrupt controller)
    pub interrupt_changed: bool,
}

/// Result from process_dma: optional reschedule info
pub struct LcdDmaResult {
    /// If Some(ticks), repeat LcdDma after this many Clock48M ticks.
    /// If None, don't reschedule (DMA complete for now).
    pub repeat_ticks: Option<u64>,
    /// If true, schedule DMA relative to LCD event with given offset instead of repeating
    pub schedule_relative: Option<u64>,
}

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
    /// Pre-converted palette: BGR565 (from 1555 raw palette)
    /// Updated on every palette write, matching CEmu's lcd.palettes[0]
    palette_bgr565: [u16; 256],
    /// Pre-converted palette: RGB565 (R/B swapped from BGR565)
    /// Updated on every palette write, matching CEmu's lcd.palettes[1]
    palette_rgb565: [u16; 256],

    // === DMA state machine (CEmu parity) ===

    /// Current compare state in the LCD event state machine
    compare: LcdCompare,
    /// Whether in prefill phase (initial FIFO fill before active video)
    prefill: bool,
    /// FIFO position counter (u8, wraps at 256 to signal prefill complete)
    pos: u8,
    /// Current row being rendered
    cur_row: u32,
    /// Current column being rendered
    cur_col: u32,

    // === Extracted timing parameters (parsed from timing registers at SYNC) ===

    /// Pixels per line
    ppl: u32,
    /// Horizontal sync width
    hsw: u32,
    /// Horizontal front porch
    hfp: u32,
    /// Horizontal back porch
    hbp: u32,
    /// Lines per panel
    lpp: u32,
    /// Vertical sync width
    vsw: u32,
    /// Vertical front porch
    vfp: u32,
    /// Vertical back porch
    vbp: u32,
    /// Panel clock divisor
    pcd: u32,
    /// Clocks per line
    cpl: u32,
    /// LCD BPP mode from control register
    lcdbpp: u32,
    /// Effective BPP exponent
    bpp: u32,
    /// Watermark flag (control bit 16)
    wtrmrk: u32,
    /// Pixels per FIFO fill
    ppf: u32,

    // === Scheduling flags (checked by emu.rs after writes) ===

    /// Set when LCD is enabled via control write — emu.rs should schedule LCD event
    pub needs_lcd_event: bool,
    /// Set when LCD is disabled via control write — emu.rs should clear LCD event
    pub needs_lcd_clear: bool,

    /// Cursor image RAM (offsets 0x800-0xBFF, 1024 bytes)
    /// Used by CE programs (e.g. LibLoad) as scratch storage
    cursor_image: [u8; 1024],

    // === Cursor registers (0xC00-0xC2C) ===
    // These registers control the hardware cursor, but CE programs
    // (notably fileioc) repurpose them as scratch storage.

    /// Cursor control (0xC00, bit 0 = enable)
    crsr_control: u8,
    /// Cursor image index (0xC00, bits 5:4)
    crsr_image: u8,
    /// Cursor config (0xC04, bits 1:0)
    crsr_config: u8,
    /// Cursor palette 0 (0xC08-0xC0B, 24-bit RGB)
    crsr_palette0: u32,
    /// Cursor palette 1 (0xC0C-0xC0F, 24-bit RGB)
    /// fileioc stores `resize_amount` here
    crsr_palette1: u32,
    /// Cursor XY position (0xC10-0xC13, two 12-bit fields)
    /// fileioc stores `curr_slot` at byte 1 ($E30C11)
    crsr_xy: u32,
    /// Cursor clip (0xC14-0xC15, two 6-bit fields)
    crsr_clip: u32,
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
            palette_bgr565: [0; 256],
            palette_rgb565: [0; 256],
            compare: LcdCompare::FrontPorch,
            prefill: false,
            pos: 0,
            cur_row: 0,
            cur_col: 0,
            ppl: 0,
            hsw: 0,
            hfp: 0,
            hbp: 0,
            lpp: 0,
            vsw: 0,
            vfp: 0,
            vbp: 0,
            pcd: 0,
            cpl: 0,
            lcdbpp: 0,
            bpp: 0,
            wtrmrk: 0,
            ppf: 0,
            needs_lcd_event: false,
            needs_lcd_clear: false,
            cursor_image: [0; 1024],
            crsr_control: 0,
            crsr_image: 0,
            crsr_config: 0,
            crsr_palette0: 0,
            crsr_palette1: 0,
            crsr_xy: 0,
            crsr_clip: 0,
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
        self.palette_bgr565 = [0; 256];
        self.palette_rgb565 = [0; 256];
        self.compare = LcdCompare::FrontPorch;
        self.prefill = false;
        self.pos = 0;
        self.cur_row = 0;
        self.cur_col = 0;
        self.ppl = 0;
        self.hsw = 0;
        self.hfp = 0;
        self.hbp = 0;
        self.lpp = 0;
        self.vsw = 0;
        self.vfp = 0;
        self.vbp = 0;
        self.pcd = 0;
        self.cpl = 0;
        self.lcdbpp = 0;
        self.bpp = 0;
        self.wtrmrk = 0;
        self.ppf = 0;
        self.needs_lcd_event = false;
        self.needs_lcd_clear = false;
        self.cursor_image = [0; 1024];
        self.crsr_control = 0;
        self.crsr_image = 0;
        self.crsr_config = 0;
        self.crsr_palette0 = 0;
        self.crsr_palette1 = 0;
        self.crsr_xy = 0;
        self.crsr_clip = 0;
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

    /// Get BPP mode from control register (bits 1-3)
    pub fn bpp_mode(&self) -> u8 {
        ((self.control >> ctrl::BPP_SHIFT) & ctrl::BPP_MASK) as u8
    }

    /// Get pre-converted palette for current BGR mode.
    /// Returns BGR565 palette when BGR bit (control bit 8) is clear,
    /// or RGB565 palette when BGR bit is set.
    /// Matches CEmu's `lcd.palettes[bgr & 1]` selection.
    pub fn palette_for_mode(&self) -> &[u16; 256] {
        if self.control & ctrl::BGR != 0 {
            &self.palette_rgb565
        } else {
            &self.palette_bgr565
        }
    }

    /// Check if LCD interrupt should be active (ris & imsc)
    pub fn check_interrupt(&self) -> bool {
        (self.ris & self.imsc) != 0
    }

    /// Extract timing parameters from timing registers.
    /// Called at SYNC state, matching CEmu lcd_event case LCD_SYNC.
    fn extract_timing(&mut self) {
        self.ppl = ((self.timing[0] >> 2 & 0x3F) + 1) << 4;
        self.hsw = (self.timing[0] >> 8 & 0xFF) + 1;
        self.hfp = (self.timing[0] >> 16 & 0xFF) + 1;
        self.hbp = (self.timing[0] >> 24 & 0xFF) + 1;
        self.lpp = (self.timing[1] & 0x3FF) + 1;
        self.vsw = (self.timing[1] >> 10 & 0x3F) + 1;
        self.vfp = self.timing[1] >> 16 & 0xFF;
        self.vbp = self.timing[1] >> 24 & 0xFF;
        self.pcd = ((self.timing[2] & 0x1F) | ((self.timing[2] >> 27 & 0x1F) << 5)) + 2;
        self.cpl = (self.timing[2] >> 16 & 0x3FF) + 1;
        if self.timing[2] >> 26 & 1 != 0 {
            self.pcd = 1;
        }
        self.lcdbpp = self.control >> 1 & 7;
        self.wtrmrk = self.control >> 16 & 1;
        self.bpp = if self.lcdbpp <= 5 { self.lcdbpp } else { 4 };
        self.ppf = 1 << (8 + self.wtrmrk - self.bpp);
    }

    /// Horizontal line duration in PCD units (used repeatedly in timing calculations)
    fn hline(&self) -> u32 {
        self.hsw + self.hbp + self.cpl + self.hfp
    }

    /// Process LCD event (called when SCHED_LCD fires).
    /// Returns the result containing the duration for the next event and optional DMA scheduling.
    /// Matches CEmu's lcd_event() state machine.
    pub fn process_event(&mut self) -> LcdEventResult {
        let compare_setting = (self.control >> 12 & 3) as u8;
        let duration;
        let schedule_dma_offset = None;

        match self.compare {
            LcdCompare::FrontPorch => {
                // Set cursor interrupt bit (simplified: always set)
                self.ris |= 1 << 0;

                if self.vfp > 0 {
                    if compare_setting == LcdCompare::FrontPorch as u8 {
                        self.ris |= 1 << 3;
                    }
                    duration = self.vfp as u64 * self.hline() as u64 * self.pcd as u64;
                    self.compare = LcdCompare::Sync;
                } else {
                    // Fall through to SYNC if VFP is 0
                    return self.process_sync_state(compare_setting);
                }
            }
            LcdCompare::Sync => {
                return self.process_sync_state(compare_setting);
            }
            LcdCompare::Lnbu => {
                self.ris |= 1 << 2; // LNBU interrupt
                duration = (self.hbp as u64 + self.cpl as u64 + self.hfp as u64)
                    * self.pcd as u64 - 1;
                self.compare = LcdCompare::BackPorch;
            }
            LcdCompare::BackPorch => {
                if self.vbp > 0 {
                    if compare_setting == LcdCompare::BackPorch as u8 {
                        self.ris |= 1 << 3;
                    }
                    duration = self.vbp as u64 * self.hline() as u64 * self.pcd as u64;
                    self.compare = LcdCompare::ActiveVideo;
                } else {
                    // Fall through to ACTIVE_VIDEO if VBP is 0
                    return self.process_active_video_state(compare_setting);
                }
            }
            LcdCompare::ActiveVideo => {
                return self.process_active_video_state(compare_setting);
            }
        }

        LcdEventResult {
            duration,
            schedule_dma_offset,
            interrupt_changed: true,
        }
    }

    /// Process SYNC state — extract timing, start DMA prefill
    fn process_sync_state(&mut self, compare_setting: u8) -> LcdEventResult {
        if compare_setting == LcdCompare::Sync as u8 {
            self.ris |= 1 << 3;
        }
        self.extract_timing();

        let duration = ((self.vsw - 1) as u64 * self.hline() as u64 + self.hsw as u64)
            * self.pcd as u64 + 1;

        self.prefill = true;
        self.pos = 0;
        self.cur_row = 0;
        self.cur_col = 0;

        // Schedule DMA prefill relative to this LCD event with offset = duration
        let schedule_dma_offset = Some(duration);

        self.compare = LcdCompare::Lnbu;

        LcdEventResult {
            duration,
            schedule_dma_offset,
            interrupt_changed: true,
        }
    }

    /// Process ACTIVE_VIDEO state
    fn process_active_video_state(&mut self, compare_setting: u8) -> LcdEventResult {
        if compare_setting == LcdCompare::ActiveVideo as u8 {
            self.ris |= 1 << 3;
        }
        let duration = self.lpp as u64 * self.hline() as u64 * self.pcd as u64;

        // If not in prefill, schedule DMA for active video rendering
        let schedule_dma_offset = if !self.prefill {
            Some((self.hsw as u64 + self.hbp as u64) * self.pcd as u64)
        } else {
            None
        };

        self.compare = LcdCompare::FrontPorch;

        LcdEventResult {
            duration,
            schedule_dma_offset,
            interrupt_changed: true,
        }
    }

    /// Process LCD DMA event (called when SCHED_LCD_DMA fires).
    /// Advances UPCURR through VRAM.
    /// Returns DMA result with reschedule info.
    pub fn process_dma(&mut self) -> LcdDmaResult {
        if self.prefill {
            // Prefill phase: fill 64 bytes at a time
            if self.pos == 0 {
                self.upcurr = self.upbase;
            }
            // Advance UPCURR by 64 bytes (simulating DMA read)
            self.upcurr = self.upcurr.wrapping_add(64);
            self.pos = self.pos.wrapping_add(64);

            // pos wraps u8: after 4 fills (4*64=256), pos wraps to 0 -> prefill done
            if self.pos != 0 {
                // More prefill needed
                let repeat = if self.pos == 128 { 22 } else { 19 };
                return LcdDmaResult {
                    repeat_ticks: Some(repeat),
                    schedule_relative: None,
                };
            }
            // Prefill complete (pos wrapped to 0)
            self.prefill = false;
            if self.compare == LcdCompare::FrontPorch {
                // Prefill finished during active video transition —
                // schedule DMA relative to LCD event
                return LcdDmaResult {
                    repeat_ticks: None,
                    schedule_relative: Some(
                        (self.hsw as u64 + self.hbp as u64) * self.pcd as u64,
                    ),
                };
            }
            // Prefill done, no more DMA until ACTIVE_VIDEO
            return LcdDmaResult {
                repeat_ticks: None,
                schedule_relative: None,
            };
        }

        // Active video phase: process pixels and advance UPCURR
        let fill_bytes: u32 = if self.wtrmrk != 0 { 64 } else { 32 };
        let words: u32 = if self.wtrmrk != 0 { 16 } else { 8 };

        // Process pixels (advance cur_col and cur_row)
        let pixels = words << (5 - self.bpp);
        self.cur_col += pixels;
        while self.cur_col >= self.cpl {
            self.cur_col -= self.cpl;
            self.cur_row += 1;
        }

        // Calculate ticks for lcd_words equivalent
        let ticks = self.process_words_ticks(words);

        // Fill FIFO (advance upcurr)
        self.upcurr = self.upcurr.wrapping_add(fill_bytes);

        if self.cur_row < self.lpp {
            // More scanlines to process
            LcdDmaResult {
                repeat_ticks: Some(ticks),
                schedule_relative: None,
            }
        } else {
            // Frame complete
            LcdDmaResult {
                repeat_ticks: None,
                schedule_relative: None,
            }
        }
    }

    /// Calculate ticks consumed by processing `words` words.
    /// Simplified: each pixel takes PCD*2 ticks at end-of-line boundaries,
    /// otherwise just the base processing time.
    fn process_words_ticks(&self, words: u32) -> u64 {
        // Base: each word group takes some ticks for pixel processing.
        // Approximate with scanline-based timing matching CEmu's lcd_words return value.
        // For each scanline transition, add (HFP + HSW + HBP) * PCD * 2
        // This is a simplification — full pixel-accurate timing is done in CEmu's lcd_process_pixel.
        let pixels = words << (5 - self.bpp);
        let mut ticks = 0u64;
        let mut col = self.cur_col.wrapping_sub(pixels); // pre-advance column

        for _ in 0..pixels {
            col += 1;
            if col >= self.cpl {
                col = 0;
                ticks += (self.hfp as u64 + self.hsw as u64 + self.hbp as u64)
                    * self.pcd as u64 * 2;
            }
        }

        // Minimum ticks for the DMA call itself
        let base_ticks = if self.wtrmrk != 0 { 19 } else { 11 };
        ticks.max(base_ticks)
    }

    /// Fast-forward LCD DMA state by `count` events worth of pixel/UPCURR advancement.
    /// Used to skip bulk catch-up events in O(1) instead of processing each one individually.
    /// This only advances the pixel/scanline counters and UPCURR — DMA timestamp
    /// accounting is handled by the caller.
    pub fn fast_forward_dma_events(&mut self, count: u64) {
        if self.prefill || count == 0 || self.cpl == 0 {
            return;
        }

        let fill_bytes: u32 = if self.wtrmrk != 0 { 64 } else { 32 };
        let words: u32 = if self.wtrmrk != 0 { 16 } else { 8 };
        let pixels_per_event = words << (5 - self.bpp);

        // Calculate how many events until frame complete
        let remaining_pixels = if self.cur_row < self.lpp {
            (self.lpp - self.cur_row) as u64 * self.cpl as u64
                - self.cur_col.min(self.cpl) as u64
        } else {
            0
        };
        let events_to_end = if pixels_per_event > 0 && remaining_pixels > 0 {
            (remaining_pixels + pixels_per_event as u64 - 1) / pixels_per_event as u64
        } else {
            0
        };

        let actual = count.min(events_to_end);
        if actual == 0 {
            return;
        }

        let total_pixels = actual * pixels_per_event as u64;
        let new_abs = self.cur_col as u64 + total_pixels;
        self.cur_col = (new_abs % self.cpl as u64) as u32;
        self.cur_row += (new_abs / self.cpl as u64) as u32;
        self.upcurr = self.upcurr.wrapping_add(actual as u32 * fill_bytes);
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
            let palette_idx = (index - regs::PALETTE_START) as usize;
            self.palette[palette_idx]
        } else if index >= 0x800 && index < 0xC00 {
            // Cursor image RAM (0x800-0xBFF)
            self.cursor_image[(index - 0x800) as usize]
        } else if index >= 0xC00 && index < 0xE00 {
            // Cursor registers (matches CEmu lcd_read cursor handling)
            match index {
                0xC00 => self.crsr_control | (self.crsr_image << 4),
                0xC04 => self.crsr_config,
                0xC08..=0xC0B => ((self.crsr_palette0 >> bit_offset) & 0xFF) as u8,
                0xC0C..=0xC0F => ((self.crsr_palette1 >> bit_offset) & 0xFF) as u8,
                0xC10..=0xC13 => ((self.crsr_xy >> bit_offset) & 0xFF) as u8,
                0xC14..=0xC15 => ((self.crsr_clip >> bit_offset) & 0xFF) as u8,
                0xC20 => self.imsc & 1,
                0xC28 => self.ris & 1,
                0xC2C => self.ris & self.imsc & 1,
                _ => 0,
            }
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
                // UPBASE -- CEmu aligns to 8 bytes
                regs::UPBASE => {
                    self.upbase = (self.upbase & !mask) | (shifted & mask);
                    self.upbase &= !7;
                }
                // LPBASE -- CEmu aligns to 8 bytes
                regs::LPBASE => {
                    self.lpbase = (self.lpbase & !mask) | (shifted & mask);
                    self.lpbase &= !7;
                }
                // Control register — detect enable/disable transitions
                regs::CONTROL => {
                    let old = self.control;
                    self.control = (self.control & !mask) | (shifted & mask);
                    // Detect lcdEn bit change (bit 0)
                    if (self.control ^ old) & ctrl::ENABLE != 0 {
                        if self.control & ctrl::ENABLE != 0 {
                            // LCD just enabled — start at SYNC state
                            self.compare = LcdCompare::Sync;
                            self.needs_lcd_event = true;
                        } else {
                            // LCD just disabled
                            self.needs_lcd_clear = true;
                        }
                    }
                }
                // IMSC -- byte write only at offset 0, bits [4:1] only
                regs::IMSC if bit_offset == 0 => {
                    self.imsc &= !0x1E;
                    self.imsc |= value & 0x1E;
                }
                // ICR -- interrupt clear, write clears bits in ris
                regs::ICR if bit_offset == 0 => {
                    self.ris &= !(value & 0x1E);
                }
                _ => {}
            }
        } else if index < regs::PALETTE_END {
            // Palette write (0x200-0x3FF)
            let palette_idx = (index - regs::PALETTE_START) as usize;
            if self.palette[palette_idx] != value {
                self.palette[palette_idx] = value;
                // Pre-convert 1555 → BGR565/RGB565 (matches CEmu lcd_write palette handling)
                let entry = palette_idx >> 1;
                let lo = self.palette[entry * 2] as u16;
                let hi = self.palette[entry * 2 + 1] as u16;
                let color = lo | (hi << 8);
                // 1555→BGR565: doubles the green MSB as the new 6th bit
                let bgr565 = color.wrapping_add(color & 0xFFE0)
                    .wrapping_add((color >> 10) & 0x0020);
                self.palette_bgr565[entry] = bgr565;
                // RGB565: swap R and B channels (CEmu lcd_bgr565swap with mask=0x1F)
                let diff = (bgr565 ^ (bgr565 >> 11)) & 0x1F;
                self.palette_rgb565[entry] = bgr565 ^ diff ^ (diff << 11);
            }
        } else if index >= 0x800 && index < 0xC00 {
            // Cursor image RAM (0x800-0xBFF)
            self.cursor_image[(index - 0x800) as usize] = value;
        } else if index >= 0xC00 && index < 0xE00 {
            // Cursor registers (matches CEmu lcd_write cursor handling)
            // CEmu word-aligns the index for write register matching
            let crsr_reg = index & 0xFFC;
            let mask = 0xFF_u32 << bit_offset;
            let shifted = (value as u32) << bit_offset;

            match crsr_reg {
                0xC00 if bit_offset == 0 => {
                    self.crsr_control = value & 1;
                    self.crsr_image = (value >> 4) & 3;
                }
                0xC04 if bit_offset == 0 => {
                    self.crsr_config = value & 3;
                }
                0xC08 if bit_offset < 24 => {
                    self.crsr_palette0 = (self.crsr_palette0 & !mask) | (shifted & mask);
                }
                0xC0C if bit_offset < 24 => {
                    self.crsr_palette1 = (self.crsr_palette1 & !mask) | (shifted & mask);
                }
                0xC10 => {
                    self.crsr_xy = (self.crsr_xy & !mask) | (shifted & mask);
                    self.crsr_xy &= 0x0FFF_0FFF;
                }
                0xC14 => {
                    self.crsr_clip = (self.crsr_clip & !mask) | (shifted & mask);
                    self.crsr_clip &= 0x003F_003F;
                }
                0xC20 if bit_offset == 0 => {
                    self.imsc = (self.imsc & !1) | (value & 1);
                }
                0xC24 if bit_offset == 0 => {
                    self.ris &= !(value & 1);
                }
                _ => {}
            }
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

    /// Get the DMA compare state (for snapshot)
    pub fn compare_state(&self) -> u8 {
        self.compare as u8
    }

    /// Set the DMA compare state (for loading snapshot)
    pub fn set_compare_state(&mut self, state: u8) {
        self.compare = match state {
            0 => LcdCompare::FrontPorch,
            1 => LcdCompare::Sync,
            2 => LcdCompare::Lnbu,
            3 => LcdCompare::BackPorch,
            _ => LcdCompare::ActiveVideo,
        };
    }

    /// Set timing registers directly (for state restore)
    pub fn set_timing(&mut self, timing: [u32; 4]) {
        self.timing = timing;
    }

    /// Set UPCURR directly (for state restore)
    pub fn set_upcurr(&mut self, value: u32) {
        self.upcurr = value;
    }

    /// Get current DMA row
    pub fn cur_row(&self) -> u32 {
        self.cur_row
    }

    /// Set current DMA row (for state restore)
    pub fn set_cur_row(&mut self, value: u32) {
        self.cur_row = value;
    }

    /// Get current DMA column
    pub fn cur_col(&self) -> u32 {
        self.cur_col
    }

    /// Set current DMA column (for state restore)
    pub fn set_cur_col(&mut self, value: u32) {
        self.cur_col = value;
    }

    /// Get prefill flag
    pub fn prefill(&self) -> bool {
        self.prefill
    }

    /// Set prefill flag (for state restore)
    pub fn set_prefill(&mut self, value: bool) {
        self.prefill = value;
    }

    /// Get FIFO position
    pub fn pos(&self) -> u8 {
        self.pos
    }

    /// Set FIFO position (for state restore)
    pub fn set_pos(&mut self, value: u8) {
        self.pos = value;
    }

    /// Get UPCURR value
    pub fn upcurr(&self) -> u32 {
        self.upcurr
    }

    /// Re-derive timing parameters from timing registers.
    /// Must be called after restoring timing registers from a state snapshot.
    pub fn recompute_timing(&mut self) {
        self.extract_timing();
    }

    // ========== Snapshot getters/setters for palette + cursor state ==========

    pub fn palette_bgr565(&self) -> &[u16; 256] {
        &self.palette_bgr565
    }

    pub fn palette_rgb565(&self) -> &[u16; 256] {
        &self.palette_rgb565
    }

    pub fn set_palette_bgr565(&mut self, data: &[u16; 256]) {
        self.palette_bgr565 = *data;
    }

    pub fn set_palette_rgb565(&mut self, data: &[u16; 256]) {
        self.palette_rgb565 = *data;
    }

    /// Reconstruct raw 1555 palette bytes from the BGR565 lookup table.
    /// Called after state restore since the state format saves derived arrays
    /// but not the raw palette bytes. Without this, any palette byte write
    /// after restore reads the other byte of the entry as zero, corrupting it.
    /// The green LSB may differ by 1 from the original 1555 value, but the
    /// forward conversion back to BGR565 produces identical results.
    pub fn reconstruct_raw_palette(&mut self) {
        for entry in 0..256 {
            let bgr565 = self.palette_bgr565[entry];
            let r = bgr565 & 0x1F;
            let g6 = (bgr565 >> 5) & 0x3F;
            let b = (bgr565 >> 11) & 0x1F;
            // Pack into 1555: alpha=0, B in bits 14:10, G5 in bits 9:5, R in bits 4:0
            let color = (b << 10) | ((g6 >> 1) << 5) | r;
            self.palette[entry * 2] = color as u8;
            self.palette[entry * 2 + 1] = (color >> 8) as u8;
        }
    }

    pub fn cursor_image(&self) -> &[u8; 1024] {
        &self.cursor_image
    }

    pub fn set_cursor_image(&mut self, data: &[u8; 1024]) {
        self.cursor_image = *data;
    }

    pub fn crsr_registers(&self) -> [u32; 5] {
        [
            (self.crsr_control as u32) | ((self.crsr_image as u32) << 4) | ((self.crsr_config as u32) << 8),
            self.crsr_palette0,
            self.crsr_palette1,
            self.crsr_xy,
            self.crsr_clip,
        ]
    }

    pub fn set_crsr_registers(&mut self, regs: &[u32; 5]) {
        self.crsr_control = (regs[0] & 1) as u8;
        self.crsr_image = ((regs[0] >> 4) & 3) as u8;
        self.crsr_config = ((regs[0] >> 8) & 3) as u8;
        self.crsr_palette0 = regs[1];
        self.crsr_palette1 = regs[2];
        self.crsr_xy = regs[3] & 0x0FFF_0FFF;
        self.crsr_clip = regs[4] & 0x003F_003F;
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
        assert_eq!(lcd.compare, LcdCompare::FrontPorch);
    }

    #[test]
    fn test_reset() {
        let mut lcd = LcdController::new();
        lcd.control = ctrl::ENABLE;
        lcd.upbase = 0xD50000;
        lcd.imsc = 0x1E;
        lcd.compare = LcdCompare::ActiveVideo;

        lcd.reset();
        assert!(!lcd.is_enabled());
        assert_eq!(lcd.upbase(), DEFAULT_VRAM_BASE);
        assert_eq!(lcd.imsc, 0);
        assert_eq!(lcd.compare, LcdCompare::FrontPorch);
    }

    #[test]
    fn test_enable_triggers_lcd_event_flag() {
        let mut lcd = LcdController::new();
        lcd.write(regs::CONTROL, ctrl::ENABLE as u8);
        assert!(lcd.is_enabled());
        assert!(lcd.needs_lcd_event);
        assert_eq!(lcd.compare, LcdCompare::Sync);
    }

    #[test]
    fn test_disable_triggers_lcd_clear_flag() {
        let mut lcd = LcdController::new();
        // Enable first
        lcd.control = ctrl::ENABLE;
        // Now disable
        lcd.write(regs::CONTROL, 0x00);
        assert!(!lcd.is_enabled());
        assert!(lcd.needs_lcd_clear);
    }

    #[test]
    fn test_upbase_write() {
        let mut lcd = LcdController::new();
        lcd.write(regs::UPBASE, 0x00);
        lcd.write(regs::UPBASE + 1, 0x00);
        lcd.write(regs::UPBASE + 2, 0xD5);
        assert_eq!(lcd.upbase(), 0xD50000);
    }

    #[test]
    fn test_upbase_alignment() {
        let mut lcd = LcdController::new();
        lcd.write(regs::UPBASE, 0x07);
        assert_eq!(lcd.upbase & 0x07, 0);
    }

    #[test]
    fn test_icr_clears_ris() {
        let mut lcd = LcdController::new();
        lcd.ris = 0x1E;
        lcd.write(regs::ICR, 0x08);
        assert_eq!(lcd.ris, 0x16);
        lcd.write(regs::ICR, 0x16);
        assert_eq!(lcd.ris, 0x00);
    }

    #[test]
    fn test_imsc_masks_bit0() {
        let mut lcd = LcdController::new();
        lcd.write(regs::IMSC, 0x1F);
        assert_eq!(lcd.imsc, 0x1E);
        assert_eq!(lcd.read(regs::IMSC), 0x1E);
    }

    #[test]
    fn test_ris_read_masks_bit0() {
        let mut lcd = LcdController::new();
        lcd.ris = 0x1F;
        assert_eq!(lcd.read(regs::RIS), 0x1E);
    }

    #[test]
    fn test_mis_read() {
        let mut lcd = LcdController::new();
        lcd.imsc = 0x08;
        lcd.ris = 0x1E;
        assert_eq!(lcd.read(regs::MIS), 0x08);
    }

    #[test]
    fn test_palette_read_write() {
        let mut lcd = LcdController::new();
        lcd.write(0x200, 0xAB);
        lcd.write(0x201, 0xCD);
        assert_eq!(lcd.read(0x200), 0xAB);
        assert_eq!(lcd.read(0x201), 0xCD);
        lcd.write(0x3FE, 0x12);
        lcd.write(0x3FF, 0x34);
        assert_eq!(lcd.read(0x3FE), 0x12);
        assert_eq!(lcd.read(0x3FF), 0x34);
    }

    #[test]
    fn test_palette_preconversion_1555_to_565() {
        let mut lcd = LcdController::new();

        // Write pure red in 1555: bit15=0, B=00000, G=00000, R=11111
        // 1555 raw = 0b0_00000_00000_11111 = 0x001F
        lcd.write(0x200, 0x1F); // lo byte
        lcd.write(0x201, 0x00); // hi byte

        // After 1555→BGR565 conversion:
        // color = 0x001F
        // bgr565 = 0x001F + (0x001F & 0xFFE0) + ((0x001F >> 10) & 0x0020)
        //        = 0x001F + 0x0000 + 0x0000 = 0x001F
        // So BGR565 has R=11111 in bits 4:0, G=000000 in 10:5, B=00000 in 15:11
        assert_eq!(lcd.palette_bgr565[0], 0x001F);
        // RGB565 swap: R and B swapped → R in 15:11
        // diff = (0x001F ^ (0x001F >> 11)) & 0x1F = (0x001F ^ 0) & 0x1F = 0x1F
        // rgb565 = 0x001F ^ 0x1F ^ (0x1F << 11) = 0x0000 ^ 0xF800 = 0xF800
        assert_eq!(lcd.palette_rgb565[0], 0xF800);

        // Write pure blue in 1555: B=11111, G=00000, R=00000
        // 1555 raw = 0b0_11111_00000_00000 = 0x7C00
        lcd.write(0x202, 0x00);
        lcd.write(0x203, 0x7C);

        // bgr565 = 0x7C00 + (0x7C00 & 0xFFE0) + ((0x7C00 >> 10) & 0x0020)
        //        = 0x7C00 + 0x7C00 + 0x0000 = 0xF800
        // (0x7C00 >> 10 = 0x1F, & 0x0020 = 0 since bit 5 is clear)
        // BGR565: B=11111 in 15:11, G=000000 in 10:5, R=00000 in 4:0
        assert_eq!(lcd.palette_bgr565[1], 0xF800);

        // Write a green-only 1555: B=00000, G=11111, R=00000
        // 1555 raw = 0b0_00000_11111_00000 = 0x03E0
        lcd.write(0x204, 0xE0);
        lcd.write(0x205, 0x03);

        // bgr565 = 0x03E0 + (0x03E0 & 0xFFE0) + ((0x03E0 >> 10) & 0x0020)
        //        = 0x03E0 + 0x03E0 + 0x0000 = 0x07C0
        // BGR565: B=00000 in 15:11, G=111110 in 10:5, R=00000 in 4:0
        assert_eq!(lcd.palette_bgr565[2], 0x07C0);
    }

    #[test]
    fn test_palette_for_mode_selects_by_bgr_bit() {
        let mut lcd = LcdController::new();
        // Write a known palette entry
        lcd.write(0x200, 0x1F); // pure red 1555
        lcd.write(0x201, 0x00);

        // BGR=0 (default): should return BGR565 palette
        assert_eq!(lcd.palette_for_mode()[0], lcd.palette_bgr565[0]);

        // BGR=1 (set bit 8 of control): should return RGB565 palette
        lcd.control |= ctrl::BGR;
        assert_eq!(lcd.palette_for_mode()[0], lcd.palette_rgb565[0]);
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

    #[test]
    fn test_extract_timing() {
        let mut lcd = LcdController::new();
        // Set up timing registers matching typical TI-84 CE config
        // TIMING0: PPL=320/16-1=19 (bits 7:2), HSW=1 (bits 15:8), HFP=1 (bits 23:16), HBP=1 (bits 31:24)
        lcd.timing[0] = (19 << 2) | (0 << 8) | (0 << 16) | (0 << 24);
        // TIMING1: LPP=240-1=239 (bits 9:0), VSW=1 (bits 15:10), VFP=2 (bits 23:16), VBP=2 (bits 31:24)
        lcd.timing[1] = 239 | (0 << 10) | (2 << 16) | (2 << 24);
        // TIMING2: PCD_LO=0 (bits 4:0), CPL=320-1=319 (bits 25:16)
        lcd.timing[2] = 0 | (319 << 16);
        lcd.control = ctrl::ENABLE; // BPP=0, no watermark

        lcd.extract_timing();

        assert_eq!(lcd.ppl, 320);
        assert_eq!(lcd.hsw, 1);
        assert_eq!(lcd.hfp, 1);
        assert_eq!(lcd.hbp, 1);
        assert_eq!(lcd.lpp, 240);
        assert_eq!(lcd.vsw, 1);
        assert_eq!(lcd.vfp, 2);
        assert_eq!(lcd.vbp, 2);
        assert_eq!(lcd.pcd, 2);
        assert_eq!(lcd.cpl, 320);
    }

    #[test]
    fn test_process_event_sync_to_lnbu() {
        let mut lcd = LcdController::new();
        // Set up basic timing
        lcd.timing[0] = (19 << 2) | (0 << 8) | (0 << 16) | (0 << 24);
        lcd.timing[1] = 239 | (0 << 10) | (2 << 16) | (2 << 24);
        lcd.timing[2] = 0 | (319 << 16);
        lcd.control = ctrl::ENABLE;
        lcd.compare = LcdCompare::Sync;

        let result = lcd.process_event();

        assert_eq!(lcd.compare, LcdCompare::Lnbu);
        assert!(lcd.prefill);
        assert_eq!(lcd.pos, 0);
        assert!(result.schedule_dma_offset.is_some());
        assert!(result.duration > 0);
    }

    #[test]
    fn test_process_dma_prefill() {
        let mut lcd = LcdController::new();
        lcd.upbase = 0xD40000;
        lcd.prefill = true;
        lcd.pos = 0;
        lcd.lpp = 240;

        // First DMA call — sets upcurr to upbase, advances 64 bytes
        let result = lcd.process_dma();
        assert_eq!(lcd.upcurr, 0xD40000 + 64);
        assert_eq!(lcd.pos, 64);
        assert!(result.repeat_ticks.is_some());

        // Second DMA call — pos = 128
        let result = lcd.process_dma();
        assert_eq!(lcd.pos, 128);
        assert!(result.repeat_ticks.is_some());
        assert_eq!(result.repeat_ticks.unwrap(), 22); // special timing for pos==128

        // Third DMA call — pos = 192
        let result = lcd.process_dma();
        assert_eq!(lcd.pos, 192);
        assert!(result.repeat_ticks.is_some());

        // Fourth DMA call — pos wraps to 0, prefill complete
        let result = lcd.process_dma();
        assert_eq!(lcd.pos, 0);
        assert!(!lcd.prefill);
        // Not in FRONT_PORCH, so no schedule_relative
        assert!(result.repeat_ticks.is_none());
    }

    #[test]
    fn test_process_dma_active() {
        let mut lcd = LcdController::new();
        lcd.upbase = 0xD40000;
        lcd.upcurr = 0xD40000;
        lcd.prefill = false;
        lcd.lpp = 240;
        lcd.cpl = 320;
        lcd.bpp = 4; // 16bpp
        lcd.wtrmrk = 0;
        lcd.hsw = 1;
        lcd.hfp = 1;
        lcd.hbp = 1;
        lcd.pcd = 2;

        let result = lcd.process_dma();
        assert!(lcd.upcurr > 0xD40000); // UPCURR advanced
        assert!(result.repeat_ticks.is_some()); // More rows to process
    }

    #[test]
    fn test_upcurr_read() {
        let mut lcd = LcdController::new();
        lcd.upcurr = 0xD40100;
        assert_eq!(lcd.read(regs::UPCURR), 0x00);
        assert_eq!(lcd.read(regs::UPCURR + 1), 0x01);
        assert_eq!(lcd.read(regs::UPCURR + 2), 0xD4);
    }

    #[test]
    fn test_reconstruct_raw_palette_roundtrip() {
        let mut lcd = LcdController::new();

        // Set up a diverse palette via normal writes
        let test_colors: [(u8, u8); 4] = [
            (0x1F, 0x00), // Pure red 1555
            (0x00, 0x7C), // Pure blue 1555
            (0xE0, 0x03), // Pure green 1555
            (0xFF, 0x7F), // White 1555 (A=0, B=31, G=31, R=31)
        ];
        for (i, &(lo, hi)) in test_colors.iter().enumerate() {
            lcd.write(0x200 + (i as u32) * 2, lo);
            lcd.write(0x201 + (i as u32) * 2, hi);
        }

        // Save the derived arrays
        let saved_bgr = lcd.palette_bgr565;
        let saved_rgb = lcd.palette_rgb565;

        // Simulate state restore: zero the raw palette, set derived arrays
        lcd.palette = [0; 512];
        lcd.palette_bgr565 = saved_bgr;
        lcd.palette_rgb565 = saved_rgb;

        // Reconstruct raw palette from BGR565
        lcd.reconstruct_raw_palette();

        // Now if a single byte of entry 0 is re-written with same value,
        // the conversion should produce the same BGR565
        let lo = lcd.palette[0]; // Reconstructed low byte
        lcd.write(0x200, lo);    // Re-trigger conversion
        assert_eq!(lcd.palette_bgr565[0], saved_bgr[0],
            "BGR565 entry 0 corrupted after single byte re-write");

        // Verify all test entries still produce correct BGR565 after reconstruction
        for i in 0..4 {
            assert_eq!(lcd.palette_bgr565[i], saved_bgr[i],
                "BGR565 mismatch at entry {}", i);
        }
    }
}
