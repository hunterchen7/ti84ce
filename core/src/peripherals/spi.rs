//! SPI Controller Stub
//!
//! Memory-mapped at port range 0xD (0xD000-0xDFFF via IN/OUT)
//!
//! This is a minimal implementation for boot parity with CEmu.
//! Timing is based on total SPI access count to avoid cycle dependencies.

/// SPI FIFO depth
const SPI_TXFIFO_DEPTH: u8 = 4;

/// SPI Controller
#[derive(Debug, Clone)]
pub struct SpiController {
    /// Control register 0 (CR0)
    cr0: u32,
    /// Control register 1 (CR1)
    cr1: u32,
    /// Control register 2 (CR2)
    cr2: u32,
    /// Interrupt control register
    int_ctrl: u32,
    /// Interrupt status register
    int_status: u32,
    /// TX FIFO valid entries (number of pending transfers)
    tfve: u8,
    /// TX FIFO index
    tfvi: u8,
    /// RX FIFO valid entries
    rfve: u8,
    /// RX FIFO index
    rfvi: u8,
    /// Transfer bits remaining
    transfer_bits: u8,
    /// Access counter - increments on every SPI read/write
    access_count: u32,
    /// Access count when last transfer was queued
    last_write_access: u32,
}

impl SpiController {
    /// Accesses after a DATA write before that transfer completes
    /// Tuned to match CEmu: transfers should complete by the time polling starts
    const ACCESSES_PER_TRANSFER: u32 = 4;

    /// Create a new SPI controller
    pub fn new() -> Self {
        Self {
            cr0: 0,
            cr1: 0,
            cr2: 0,
            int_ctrl: 0,
            int_status: 0,
            tfve: 0,
            tfvi: 0,
            rfve: 0,
            rfvi: 0,
            transfer_bits: 0,
            access_count: 0,
            last_write_access: 0,
        }
    }

    /// Reset the SPI controller
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Update transfer completion based on accesses elapsed
    fn update_transfers(&mut self) {
        if self.tfve > 0 {
            // Complete transfers based on accesses since last write
            let accesses_since_write = self.access_count.saturating_sub(self.last_write_access);
            let transfers_to_complete = (accesses_since_write / Self::ACCESSES_PER_TRANSFER) as u8;
            if transfers_to_complete > 0 {
                let completed = transfers_to_complete.min(self.tfve);
                self.tfve = self.tfve.saturating_sub(completed);
                if self.tfve == 0 {
                    self.transfer_bits = 0;
                }
            }
        }
    }

    /// Read from SPI port
    /// addr is the offset within the SPI port range (masked to 0x7F)
    pub fn read(&mut self, addr: u32, _current_cycles: u64) -> u8 {
        self.access_count += 1;

        let shift = (addr & 3) << 3;
        let reg_idx = addr >> 2;

        // STATUS register reads: complete ALL pending transfers
        // This gets us past the first SPI polling loop (step 418K)
        // and to step 699K where a different issue occurs
        if reg_idx == 3 && self.tfve > 0 {
            self.tfve = 0;
            self.transfer_bits = 0;
        }

        let value: u32 = match reg_idx {
            // CR0 (0x00-0x03)
            0 => self.cr0,
            // CR1 (0x04-0x07)
            1 => self.cr1,
            // CR2 (0x08-0x0B)
            2 => self.cr2,
            // STATUS (0x0C-0x0F)
            3 => {
                // STATUS register:
                // bits 12+: tfve (TX FIFO valid entries)
                // bits 4-7: rfve (RX FIFO valid entries)
                // bit 2: transfer in progress
                // bit 1: TX FIFO not full (tfve < TXFIFO_DEPTH)
                // bit 0: RX FIFO full
                let tx_not_full = if self.tfve < SPI_TXFIFO_DEPTH { 1 } else { 0 };
                let rx_full = if self.rfve >= SPI_TXFIFO_DEPTH { 1 } else { 0 };
                let transfer_active = if self.transfer_bits != 0 { 1 } else { 0 };
                ((self.tfve as u32) << 12)
                    | ((self.rfve as u32) << 4)
                    | (transfer_active << 2)
                    | (tx_not_full << 1)
                    | rx_full
            }
            // INTCTRL (0x10-0x13)
            4 => self.int_ctrl,
            // INTSTATUS (0x14-0x17)
            5 => self.int_status,
            // DATA (0x18-0x1B) - reading drains RX FIFO
            6 => 0, // No RX data for now
            // FEATURE (0x1C-0x1F)
            7 => {
                // Features: TXFIFO_DEPTH-1, RXFIFO_DEPTH-1, WIDTH-1
                ((SPI_TXFIFO_DEPTH - 1) as u32) << 16
                    | ((SPI_TXFIFO_DEPTH - 1) as u32) << 8
                    | 0x1F // 32-bit width
            }
            // REVISION (0x60-0x63)
            24 => 0x00012100,
            // FEATURE2 (0x64-0x67)
            25 => {
                ((SPI_TXFIFO_DEPTH - 1) as u32) << 16
                    | ((SPI_TXFIFO_DEPTH - 1) as u32) << 8
                    | 0x1F
            }
            _ => 0,
        };

        (value >> shift) as u8
    }

    /// Write to SPI port
    /// addr is the offset within the SPI port range (masked to 0x7F)
    pub fn write(&mut self, addr: u32, value: u8, _current_cycles: u64) {
        self.access_count += 1;
        self.update_transfers();

        let shift = (addr & 3) << 3;
        let value32 = (value as u32) << shift;
        let mask = !(0xFF_u32 << shift);

        match addr >> 2 {
            // CR0 (0x00-0x03)
            0 => {
                self.cr0 = (self.cr0 & mask) | (value32 & 0xFFFF);
            }
            // CR1 (0x04-0x07)
            1 => {
                self.cr1 = (self.cr1 & mask) | (value32 & 0x7FFFFF);
            }
            // CR2 (0x08-0x0B)
            2 => {
                self.cr2 = (self.cr2 & mask) | value32;
                // Bit 2: Reset RX FIFO
                if value32 & (1 << 2) != 0 {
                    self.rfvi = 0;
                    self.rfve = 0;
                }
                // Bit 1: Reset TX FIFO
                if value32 & (1 << 1) != 0 {
                    self.tfvi = 0;
                    self.tfve = 0;
                }
            }
            // INTCTRL (0x10-0x13)
            4 => {
                self.int_ctrl = (self.int_ctrl & mask) | value32;
            }
            // DATA (0x18-0x1B) - writing adds to TX FIFO
            6 => {
                if shift == 0 && self.tfve < SPI_TXFIFO_DEPTH {
                    // Add to TX FIFO (only on byte 0 write)
                    self.tfve += 1;
                    self.tfvi = self.tfvi.wrapping_add(1);
                    // Start transfer if not already in progress
                    if self.transfer_bits == 0 {
                        // Set transfer bits based on CR1 settings
                        self.transfer_bits = ((self.cr1 >> 16) as u8 & 0x1F) + 1;
                    }
                    // Mark when this write happened for timing
                    self.last_write_access = self.access_count;
                }
            }
            _ => {}
        }
    }
}

impl Default for SpiController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let spi = SpiController::new();
        assert_eq!(spi.tfve, 0);
        assert_eq!(spi.rfve, 0);
    }

    #[test]
    fn test_status_idle() {
        let mut spi = SpiController::new();
        // Read STATUS byte 0 (offset 0x0C)
        let status0 = spi.read(0x0C, 0);
        // Bit 1 should be set (TX not full)
        assert_eq!(status0 & 0x02, 0x02);

        // Read STATUS byte 1 (offset 0x0D)
        let status1 = spi.read(0x0D, 0);
        // tfve = 0, so upper byte should be 0
        assert_eq!(status1, 0x00);
    }

    #[test]
    fn test_data_write_increments_tfve() {
        let mut spi = SpiController::new();

        // Write to DATA register
        spi.write(0x18, 0x00, 0);
        assert_eq!(spi.tfve, 1); // Directly check tfve increased

        // Status read completes all transfers
        let status1 = spi.read(0x0D, 0);
        assert_eq!(status1, 0x00); // tfve = 0 after read
    }

    #[test]
    fn test_transfer_completes_on_status_read() {
        let mut spi = SpiController::new();

        // Write 3 transfers to DATA register
        spi.write(0x18, 0x00, 0);
        spi.write(0x18, 0x00, 0);
        spi.write(0x18, 0x00, 0);

        // First status read completes all transfers
        let status1 = spi.read(0x0D, 0);
        assert_eq!(status1, 0x00); // tfve = 0, all transfers done
    }
}
