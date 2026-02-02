//! SPI Controller with Device Abstraction
//!
//! Memory-mapped at port range 0xD (0xD000-0xDFFF via IN/OUT)
//!
//! This implementation provides CEmu parity with:
//! - Actual FIFO arrays (16 entries each for RX and TX)
//! - Device abstraction for LCD panel and coprocessor communication
//! - Null device that returns 0xC3 for coprocessor reads (OS 5.7.0 compatibility)
//!
//! Timing is based on CPU cycles with a 24 MHz SPI clock model.

/// SPI FIFO depth (matches CEmu)
const SPI_RXFIFO_DEPTH: usize = 16;
const SPI_TXFIFO_DEPTH: usize = 16;

/// SPI feature flags (matches CEmu)
const SPI_FEATURES: u8 = 0xE;
const SPI_WIDTH: u8 = 32;

/// SPI Device trait for abstracting communication with different peripherals
pub trait SpiDevice: std::fmt::Debug {
    /// Called when the device is selected or deselected
    fn select(&mut self, low: bool);

    /// Peek at what data the device would send (without consuming)
    /// Returns the number of bits and sets rx_data to the data
    fn peek(&self, rx_data: &mut u32) -> u8;

    /// Transfer data to/from the device
    /// Takes tx_data sent to device, returns number of bits and sets rx_data
    fn transfer(&mut self, tx_data: u32, rx_data: &mut u32) -> u8;
}

/// Null SPI device - returns 0xC3 for coprocessor compatibility
/// This is the "Hack to make OS 5.7.0 happy" from CEmu
#[derive(Debug, Clone, Default)]
pub struct NullSpiDevice;

impl SpiDevice for NullSpiDevice {
    fn select(&mut self, _low: bool) {
        // Null device does nothing on select
    }

    fn peek(&self, rx_data: &mut u32) -> u8 {
        // Hack to make OS 5.7.0 happy without a coprocessor
        *rx_data = 0xC3;
        8 // Return 8 bits
    }

    fn transfer(&mut self, _tx_data: u32, rx_data: &mut u32) -> u8 {
        self.peek(rx_data)
    }
}

/// Which device is currently selected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpiDeviceSelect {
    /// LCD Panel (default)
    #[default]
    Panel,
    /// ARM coprocessor (uses null device)
    Arm,
}

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
    /// TX FIFO index (read position)
    tfvi: u8,
    /// RX FIFO valid entries
    rfve: u8,
    /// RX FIFO index (read position)
    rfvi: u8,
    /// Transfer bits remaining
    transfer_bits: u8,
    /// Device bits remaining (from device response)
    device_bits: u8,
    /// Current TX frame being shifted out
    tx_frame: u32,
    /// Current RX frame being shifted in
    rx_frame: u32,
    /// Current device frame
    device_frame: u32,
    /// Cycle when the current transfer completes
    next_event_cycle: Option<u64>,
    /// TX FIFO array (actual data storage)
    tx_fifo: [u32; SPI_TXFIFO_DEPTH],
    /// RX FIFO array (actual data storage)
    rx_fifo: [u32; SPI_RXFIFO_DEPTH],
    /// Currently selected device
    device_select: SpiDeviceSelect,
}

impl SpiController {
    /// SPI base clock (CEmu uses CLOCK_24M)
    const SPI_CLOCK_HZ: u64 = 24_000_000;

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
            device_bits: 0,
            tx_frame: 0,
            rx_frame: 0,
            device_frame: 0,
            next_event_cycle: None,
            tx_fifo: [0; SPI_TXFIFO_DEPTH],
            rx_fifo: [0; SPI_RXFIFO_DEPTH],
            device_select: SpiDeviceSelect::Panel,
        }
    }

    /// Reset the SPI controller
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Select which SPI device to communicate with
    /// arm=true selects ARM coprocessor (null device), arm=false selects LCD panel
    pub fn select_device(&mut self, arm: bool) {
        let new_select = if arm {
            SpiDeviceSelect::Arm
        } else {
            SpiDeviceSelect::Panel
        };

        if self.device_select != new_select {
            // Deselect current device, reset device state
            self.device_frame = 0;
            self.device_bits = 0;
            self.device_select = new_select;
        }
    }

    /// Get the currently selected device type
    pub fn current_device(&self) -> SpiDeviceSelect {
        self.device_select
    }

    /// True if SPI is enabled (CR2 bit 0)
    fn spi_enabled(&self) -> bool {
        self.cr2 & 0x1 != 0
    }

    /// True if TX is enabled (CR2 bit 8)
    fn tx_enabled(&self) -> bool {
        self.cr2 & (1 << 8) != 0
    }

    /// True if RX is enabled (CR2 bit 7)
    fn rx_enabled(&self) -> bool {
        self.cr2 & (1 << 7) != 0
    }

    /// True if FLASH bit is set (CR0 bit 11)
    fn flash_enabled(&self) -> bool {
        self.cr0 & (1 << 11) != 0
    }

    /// True if loopback mode is enabled (CR0 bit 7)
    fn loopback_enabled(&self) -> bool {
        self.cr0 & (1 << 7) != 0
    }

    /// CPU clock rate in Hz based on control port speed value
    fn cpu_clock_hz(speed: u8) -> u32 {
        match speed & 0x03 {
            0 => 6_000_000,
            1 => 12_000_000,
            2 => 24_000_000,
            _ => 48_000_000,
        }
    }

    /// Transfer bit count from CR1 (bits 16-20, +1)
    fn transfer_bit_count(&self) -> u8 {
        ((self.cr1 >> 16) as u8 & 0x1F) + 1
    }

    /// Transfer duration in 24 MHz ticks
    fn transfer_ticks(&self, bit_count: u8) -> u64 {
        let divider = (self.cr1 & 0xFFFF) as u64 + 1;
        bit_count as u64 * divider
    }

    /// Compute the next event cycle using CEmu-like tick conversion
    fn next_event_cycle(&self, base_cycle: u64, cpu_speed: u8, ticks: u64) -> u64 {
        let cpu_hz = Self::cpu_clock_hz(cpu_speed) as u64;
        let base_tick = ((base_cycle as u128) * (Self::SPI_CLOCK_HZ as u128)) / (cpu_hz as u128);
        let next_tick = base_tick + ticks as u128;
        let next_cycle =
            (next_tick * (cpu_hz as u128) + (Self::SPI_CLOCK_HZ as u128 - 1)) / (Self::SPI_CLOCK_HZ as u128);
        (next_cycle as u64).max(base_cycle.saturating_add(1))
    }

    fn trace_enabled() -> bool {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| std::env::var_os("SPI_TRACE").is_some())
    }

    /// Get threshold status for interrupt generation
    fn get_threshold_status(&self) -> u8 {
        let tx_threshold = ((self.int_ctrl >> 12) & 0x1F) as u8;
        let rx_threshold = ((self.int_ctrl >> 7) & 0x1F) as u8;
        let tx_status = if tx_threshold != 0 && self.tfve <= tx_threshold {
            1 << 3
        } else {
            0
        };
        let rx_status = if rx_threshold != 0 && self.rfve >= rx_threshold {
            1 << 2
        } else {
            0
        };
        tx_status | rx_status
    }

    /// Update threshold-based interrupt status
    fn update_thresholds(&mut self) {
        if self.int_ctrl & (0x1F << 12 | 0x1F << 7) != 0 {
            let status_diff = (self.int_status as u8 & (1 << 3 | 1 << 2)) ^ self.get_threshold_status();
            if status_diff != 0 {
                self.int_status ^= status_diff as u32;
                // TODO: Update interrupt controller when integrated
            }
        }
    }

    /// Peek at device data (get data without consuming)
    fn device_peek(&self) -> (u8, u32) {
        match self.device_select {
            SpiDeviceSelect::Arm => {
                // Null device: return 0xC3 (OS 5.7.0 compatibility hack)
                (8, 0xC3)
            }
            SpiDeviceSelect::Panel => {
                // Panel device: return 0 for now (panel not implemented yet)
                // TODO: Implement panel SPI when needed
                (8, 0)
            }
        }
    }

    /// Transfer data to/from device
    fn device_transfer(&mut self, tx_data: u32) -> (u8, u32) {
        match self.device_select {
            SpiDeviceSelect::Arm => {
                // Null device: return 0xC3 regardless of tx_data
                let _ = tx_data;
                (8, 0xC3)
            }
            SpiDeviceSelect::Panel => {
                // Panel device: echo for now
                // TODO: Implement panel SPI when needed
                let _ = tx_data;
                (8, 0)
            }
        }
    }

    /// Try to start the next transfer, returns ticks until completion or 0 if can't start
    fn next_transfer(&mut self) -> u64 {
        if self.transfer_bits != 0 {
            // Already in a transfer, can't start another
            let bit_count = self.transfer_bits.min(self.device_bits);
            return self.transfer_ticks(bit_count);
        }

        let tx_enabled = self.tx_enabled();
        let tx_available = tx_enabled && self.tfve != 0;

        if self.rx_enabled() {
            // If FLASH bit is reset and TX is enabled, only receive when the TX FIFO is non-empty
            if !self.flash_enabled() && tx_enabled && self.tfve == 0 {
                return 0;
            }
            // Odd RX behavior: allow transfer after 15 entries only if TX FIFO is non-empty
            let rfve_limit = SPI_RXFIFO_DEPTH - if self.tfve == 0 { 1 } else { 0 };
            if self.rfve as usize >= rfve_limit {
                return 0;
            }
        } else if !tx_available {
            // If not receiving, disallow transfer if TX FIFO is empty or disabled
            return 0;
        }

        // Start the transfer
        self.transfer_bits = self.transfer_bit_count();
        let tx_index = (self.tfvi as usize) & (SPI_TXFIFO_DEPTH - 1);
        self.tx_frame = self.tx_fifo[tx_index] << (32 - self.transfer_bits);

        if tx_available {
            self.tfvi = self.tfvi.wrapping_add(1);
            self.tfve = self.tfve.saturating_sub(1);
            self.update_thresholds();
        } else if tx_enabled {
            // Set TX underflow if TX enabled but no data
            self.int_status |= 1 << 1;
            // TODO: Update interrupt controller
        }

        // Get device data if needed
        if self.device_bits == 0 {
            let (bits, data) = self.device_peek();
            self.device_bits = bits;
            self.device_frame = data << (32 - bits);
        }

        let bit_count = self.transfer_bits.min(self.device_bits);
        self.transfer_ticks(bit_count)
    }

    /// Process a transfer event (called when scheduled transfer completes)
    fn process_transfer_event(&mut self) {
        if self.transfer_bits == 0 {
            return;
        }

        let bit_count = self.transfer_bits.min(self.device_bits);

        // Shift in received data
        self.rx_frame <<= bit_count;

        // Handle loopback mode
        if self.loopback_enabled() {
            self.rx_frame |= self.tx_frame >> (32 - bit_count);
        } else if self.device_select == SpiDeviceSelect::Arm {
            // For ARM coprocessor, receive device data
            self.rx_frame |= self.device_frame >> (32 - bit_count);
        }

        // Shift device frame and insert TX data
        self.device_frame <<= bit_count;
        self.device_frame |= self.tx_frame >> (32 - bit_count);
        self.tx_frame <<= bit_count;

        self.device_bits -= bit_count;
        if self.device_bits == 0 {
            // Transfer to device complete, get response
            let (bits, data) = self.device_transfer(self.device_frame);
            self.device_bits = bits;
            self.device_frame = data << (32 - bits);
        }

        self.transfer_bits -= bit_count;
        if self.transfer_bits == 0 && self.rx_enabled() {
            // Transfer complete, store in RX FIFO
            let rx_index = ((self.rfvi as usize) + (self.rfve as usize)) & (SPI_RXFIFO_DEPTH - 1);
            self.rx_fifo[rx_index] = self.rx_frame;
            self.rfve = self.rfve.saturating_add(1);
            self.update_thresholds();
        }
    }

    fn start_transfer(&mut self, base_cycle: u64, cpu_speed: u8) -> bool {
        let ticks = self.next_transfer();
        if ticks == 0 {
            return false;
        }

        let next_cycle = self.next_event_cycle(base_cycle, cpu_speed, ticks);
        self.next_event_cycle = Some(next_cycle);

        if Self::trace_enabled() {
            eprintln!(
                "[spi] start cycle={} next={} tfve={} bits={} device={:?}",
                base_cycle, next_cycle, self.tfve, self.transfer_bits, self.device_select
            );
        }

        true
    }

    /// Advance SPI transfers based on current CPU cycles
    fn update(&mut self, current_cycles: u64, cpu_speed: u8) {
        if !self.spi_enabled() {
            self.transfer_bits = 0;
            self.device_bits = 0;
            self.tx_frame = 0;
            self.device_frame = 0;
            self.next_event_cycle = None;
            return;
        }

        while let Some(next_cycle) = self.next_event_cycle {
            if current_cycles < next_cycle {
                break;
            }

            if Self::trace_enabled() {
                eprintln!(
                    "[spi] complete cycle={} now={} tfve={} rfve={}",
                    next_cycle, current_cycles, self.tfve, self.rfve
                );
            }

            self.process_transfer_event();
            self.next_event_cycle = None;

            // Try to start next transfer
            if !self.start_transfer(next_cycle, cpu_speed) {
                break;
            }
        }

        if self.transfer_bits == 0 && self.next_event_cycle.is_none() {
            self.start_transfer(current_cycles, cpu_speed);
        }
    }

    /// Read from SPI port
    /// addr is the offset within the SPI port range (masked to 0x7F)
    pub fn read(&mut self, addr: u32, current_cycles: u64, cpu_speed: u8) -> u8 {
        self.update(current_cycles, cpu_speed);

        let shift = (addr & 3) << 3;
        let reg_idx = addr >> 2;

        // CR0 masks for different modes (from CEmu)
        const CR0_MASKS: [u16; 16] = [
            0xF18C, 0xF8EF, 0xF0AC, 0xF3FC, 0xF18C, 0xF4C0, 0xF3AC, 0xF3AC, 0xF18C, 0xFBEF, 0xF0AC,
            0xF3FC, 0xF18C, 0xF4C0, 0xF3AC, 0xF3AC,
        ];

        let value: u32 = match reg_idx {
            // CR0 (0x00-0x03)
            0 => {
                let masked = self.cr0 & (CR0_MASKS[((self.cr0 >> 12) & 0xF) as usize] as u32);
                masked
            }
            // CR1 (0x04-0x07)
            1 => self.cr1,
            // CR2 (0x08-0x0B)
            2 => self.cr2,
            // STATUS (0x0C-0x0F)
            3 => {
                let tx_not_full = if self.tfve < SPI_TXFIFO_DEPTH as u8 {
                    1
                } else {
                    0
                };
                let rx_full = if self.rfve >= SPI_RXFIFO_DEPTH as u8 {
                    1
                } else {
                    0
                };
                let transfer_active = if self.transfer_bits != 0 { 1 } else { 0 };
                let status = ((self.tfve as u32) << 12)
                    | ((self.rfve as u32) << 4)
                    | (transfer_active << 2)
                    | (tx_not_full << 1)
                    | rx_full;
                if Self::trace_enabled() {
                    eprintln!(
                        "[spi] status cycle={} tfve={} rfve={} active={}",
                        current_cycles, self.tfve, self.rfve, transfer_active
                    );
                }
                status
            }
            // INTCTRL (0x10-0x13)
            4 => self.int_ctrl,
            // INTSTATUS (0x14-0x17)
            5 => {
                let status = self.int_status;
                // Reading clears underflow/overflow bits
                self.int_status &= !(1 << 1 | 1 << 0);
                // TODO: Update interrupt controller
                status
            }
            // DATA (0x18-0x1B) - reading drains RX FIFO
            6 => {
                let rx_index = (self.rfvi as usize) & (SPI_RXFIFO_DEPTH - 1);
                let value = self.rx_fifo[rx_index];
                if shift == 0 && self.rfve > 0 {
                    self.rfvi = self.rfvi.wrapping_add(1);
                    self.rfve = self.rfve.saturating_sub(1);
                    self.update_thresholds();
                    // May enable new transfers
                    self.update(current_cycles, cpu_speed);
                }
                value
            }
            // FEATURE (0x1C-0x1F)
            7 => {
                (SPI_FEATURES as u32) << 24
                    | ((SPI_TXFIFO_DEPTH - 1) as u32) << 16
                    | ((SPI_RXFIFO_DEPTH - 1) as u32) << 8
                    | (SPI_WIDTH as u32 - 1)
            }
            // REVISION (0x60-0x63)
            24 => 0x00012100,
            // FEATURE2 (0x64-0x67)
            25 => {
                (SPI_FEATURES as u32) << 24
                    | ((SPI_TXFIFO_DEPTH - 1) as u32) << 16
                    | ((SPI_RXFIFO_DEPTH - 1) as u32) << 8
                    | (SPI_WIDTH as u32 - 1)
            }
            _ => 0,
        };

        (value >> shift) as u8
    }

    /// Write to SPI port
    /// addr is the offset within the SPI port range (masked to 0x7F)
    pub fn write(&mut self, addr: u32, value: u8, current_cycles: u64, cpu_speed: u8) {
        self.update(current_cycles, cpu_speed);

        let shift = (addr & 3) << 3;
        let value32 = (value as u32) << shift;
        let mask = !(0xFF_u32 << shift);
        let mut state_changed = false;

        match addr >> 2 {
            // CR0 (0x00-0x03)
            0 => {
                // Check if FLASH bit changed
                if (self.cr0 ^ value32) & !mask & (1 << 11) != 0 {
                    state_changed = true;
                }
                let new_value = (self.cr0 & mask) | (value32 & 0xFFFF);
                if Self::trace_enabled() && new_value != self.cr0 {
                    eprintln!(
                        "[spi] cr0 write cycle={} value=0x{:04X}",
                        current_cycles, new_value
                    );
                }
                self.cr0 = new_value;
            }
            // CR1 (0x04-0x07)
            1 => {
                let new_value = (self.cr1 & mask) | (value32 & 0x7FFFFF);
                if Self::trace_enabled() && new_value != self.cr1 {
                    eprintln!(
                        "[spi] cr1 write cycle={} value=0x{:06X}",
                        current_cycles, new_value
                    );
                }
                self.cr1 = new_value;
            }
            // CR2 (0x08-0x0B)
            2 => {
                // Bit 2: Reset RX FIFO
                if value32 & (1 << 2) != 0 {
                    self.rfvi = 0;
                    self.rfve = 0;
                    state_changed = true;
                }
                // Bit 3: Reset TX FIFO
                if value32 & (1 << 3) != 0 {
                    self.tfvi = 0;
                    self.tfve = 0;
                    state_changed = true;
                }
                // Check if TX/RX enable or SPI enable changed
                if (self.cr2 ^ value32) & !mask & (1 << 8 | 1 << 7 | 1 << 0) != 0 {
                    state_changed = true;
                }
                // Only low CR2 bits are writable (matches CEmu mask)
                let masked_value = value32 & 0xF83;
                let new_value = (self.cr2 & mask) | masked_value;
                if Self::trace_enabled() && new_value != self.cr2 {
                    eprintln!(
                        "[spi] cr2 write cycle={} value=0x{:03X}",
                        current_cycles, new_value
                    );
                }
                self.cr2 = new_value;
            }
            // INTCTRL (0x10-0x13)
            4 => {
                let new_value = (self.int_ctrl & mask) | (value32 & 0x1FFBF);
                self.int_ctrl = new_value;
                // Update threshold status
                self.int_status &= !(1 << 3 | 1 << 2);
                self.int_status |= self.get_threshold_status() as u32;
                // TODO: Update interrupt controller
            }
            // DATA (0x18-0x1B) - writing adds to TX FIFO
            6 => {
                // Compute index for the current entry being written
                // This is the next write position: tfvi + tfve
                let tx_index =
                    ((self.tfvi as usize) + (self.tfve as usize)) & (SPI_TXFIFO_DEPTH - 1);

                // Update the FIFO entry with the new byte
                self.tx_fifo[tx_index] = (self.tx_fifo[tx_index] & mask) | value32;

                // Only increment tfve on byte 0 write (completing a 32-bit entry)
                if shift == 0 && (self.tfve as usize) < SPI_TXFIFO_DEPTH {
                    self.tfve += 1;
                    state_changed = true;
                    if Self::trace_enabled() {
                        eprintln!(
                            "[spi] data write cycle={} tfve={} value=0x{:08X}",
                            current_cycles, self.tfve, self.tx_fifo[tx_index]
                        );
                    }
                }
            }
            _ => {}
        }

        if state_changed {
            self.update(current_cycles, cpu_speed);
        }
    }

    // === FIFO access methods for testing and debugging ===

    /// Get the current TX FIFO contents (for testing)
    pub fn tx_fifo_contents(&self) -> &[u32; SPI_TXFIFO_DEPTH] {
        &self.tx_fifo
    }

    /// Get the current RX FIFO contents (for testing)
    pub fn rx_fifo_contents(&self) -> &[u32; SPI_RXFIFO_DEPTH] {
        &self.rx_fifo
    }

    /// Get TX FIFO valid entry count
    pub fn tx_fifo_count(&self) -> u8 {
        self.tfve
    }

    /// Get RX FIFO valid entry count
    pub fn rx_fifo_count(&self) -> u8 {
        self.rfve
    }
}

// === Scheduler integration stub methods ===
// Note: SPI timing is currently handled internally via cycle-based update()
// These methods are stubs for future scheduler integration

impl SpiController {
    /// Advance one SPI transfer (scheduler callback)
    /// This is a stub - SPI currently uses internal cycle-based timing
    pub fn advance_transfer(&mut self) {
        // SPI timing is handled internally via update()
        // This stub exists for future scheduler integration
    }

    /// Check if there are pending transfers for scheduler
    pub fn has_pending_transfers(&self) -> bool {
        self.tfve > 0 || self.transfer_bits > 0
    }

    /// Get transfer duration in 24MHz ticks for scheduler
    pub fn sched_transfer_ticks(&self) -> u64 {
        let bit_count = self.transfer_bit_count() as u64;
        let divider = ((self.cr1 & 0xFFFF) + 1) as u64;
        bit_count * divider
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

    const CPU_SPEED_24MHZ: u8 = 0x02;
    const CPU_SPEED_48MHZ: u8 = 0x03;

    #[test]
    fn test_new() {
        let spi = SpiController::new();
        assert_eq!(spi.tfve, 0);
        assert_eq!(spi.rfve, 0);
        assert_eq!(spi.device_select, SpiDeviceSelect::Panel);
    }

    #[test]
    fn test_status_idle() {
        let mut spi = SpiController::new();
        // Read STATUS byte 0 (offset 0x0C)
        let status0 = spi.read(0x0C, 0, CPU_SPEED_24MHZ);
        // Bit 1 should be set (TX not full)
        assert_eq!(status0 & 0x02, 0x02);

        // Read STATUS byte 1 (offset 0x0D)
        let status1 = spi.read(0x0D, 0, CPU_SPEED_24MHZ);
        // tfve = 0, so upper nibble should be 0
        assert_eq!(status1, 0x00);
    }

    #[test]
    fn test_data_write_increments_tfve() {
        let mut spi = SpiController::new();

        // Write to DATA register
        spi.write(0x18, 0x00, 0, CPU_SPEED_24MHZ);
        assert_eq!(spi.tfve, 1);

        // STATUS byte 1 should report tfve = 1 (upper nibble)
        let status1 = spi.read(0x0D, 0, CPU_SPEED_24MHZ);
        assert_eq!(status1, 0x10);
    }

    #[test]
    fn test_tx_fifo_stores_data() {
        let mut spi = SpiController::new();

        // Write 32-bit value to TX FIFO (4 bytes)
        // Must write bytes 3,2,1 BEFORE byte 0, since byte 0 commits the entry
        // and advances tfve to point to the next entry
        spi.write(0x1B, 0xDD, 0, CPU_SPEED_24MHZ); // byte 3 (doesn't commit)
        spi.write(0x1A, 0xCC, 0, CPU_SPEED_24MHZ); // byte 2 (doesn't commit)
        spi.write(0x19, 0xBB, 0, CPU_SPEED_24MHZ); // byte 1 (doesn't commit)
        spi.write(0x18, 0xAA, 0, CPU_SPEED_24MHZ); // byte 0 (commits, tfve++)

        // Only byte 0 write increments tfve
        assert_eq!(spi.tfve, 1);

        // Check the stored value
        assert_eq!(spi.tx_fifo[0], 0xDDCCBBAA);

        // Write another entry the same way
        spi.write(0x1B, 0x44, 0, CPU_SPEED_24MHZ);
        spi.write(0x1A, 0x33, 0, CPU_SPEED_24MHZ);
        spi.write(0x19, 0x22, 0, CPU_SPEED_24MHZ);
        spi.write(0x18, 0x11, 0, CPU_SPEED_24MHZ);

        assert_eq!(spi.tfve, 2);
        assert_eq!(spi.tx_fifo[1], 0x44332211);
    }

    #[test]
    fn test_transfer_completes_after_cycles() {
        let mut spi = SpiController::new();

        // Enable SPI (CR2 bit 0) and TX (CR2 bit 8)
        spi.write(0x08, 0x01, 0, CPU_SPEED_24MHZ);
        spi.write(0x09, 0x01, 0, CPU_SPEED_24MHZ);

        // CR1: divider = 3 (value 2), bit count = 8 (value 7)
        spi.write(0x04, 0x02, 0, CPU_SPEED_24MHZ);
        spi.write(0x06, 0x07, 0, CPU_SPEED_24MHZ);

        // Queue one transfer
        spi.write(0x18, 0x00, 0, CPU_SPEED_24MHZ);

        // Before completion (24 cycles total), transfer should be active
        let status0 = spi.read(0x0C, 23, CPU_SPEED_24MHZ);
        assert_eq!(status0 & 0x04, 0x04);

        // At completion, transfer should be inactive
        let status0_done = spi.read(0x0C, 24, CPU_SPEED_24MHZ);
        assert_eq!(status0_done & 0x04, 0x00);
    }

    #[test]
    fn test_device_select() {
        let mut spi = SpiController::new();
        assert_eq!(spi.current_device(), SpiDeviceSelect::Panel);

        spi.select_device(true); // Select ARM coprocessor
        assert_eq!(spi.current_device(), SpiDeviceSelect::Arm);

        spi.select_device(false); // Select Panel
        assert_eq!(spi.current_device(), SpiDeviceSelect::Panel);
    }

    #[test]
    fn test_null_device_returns_0xc3() {
        // Test the NullSpiDevice directly
        let device = NullSpiDevice;
        let mut rx_data = 0u32;

        let bits = device.peek(&mut rx_data);
        assert_eq!(bits, 8);
        assert_eq!(rx_data, 0xC3);
    }

    #[test]
    fn test_arm_coprocessor_rx_returns_0xc3() {
        let mut spi = SpiController::new();

        // Select ARM coprocessor
        spi.select_device(true);

        // Configure CR1 first: divider = 1, bit count = 8
        // Set these BEFORE enabling SPI to avoid triggering transfers
        spi.cr1 = 0x070000; // bit count = 8 (7+1)

        // Enable SPI with TX and RX enabled (CR2 = 0x181)
        // bit 0: SPI enable
        // bit 7: RX enable
        // bit 8: TX enable
        spi.cr2 = 0x181;

        // Set FLASH bit to allow RX-only transfers (without this, RX needs TX data)
        spi.cr0 |= 1 << 11;

        // Reset FIFOs to start clean
        spi.tfve = 0;
        spi.tfvi = 0;
        spi.rfve = 0;
        spi.rfvi = 0;
        spi.transfer_bits = 0;
        spi.device_bits = 0;
        spi.next_event_cycle = None;

        // Queue a single transfer
        spi.write(0x18, 0x00, 0, CPU_SPEED_24MHZ);

        // The transfer should complete in 8 SPI ticks = 8 CPU cycles at 24MHz
        // Allow just enough time for one transfer
        spi.update(10, CPU_SPEED_24MHZ);

        // RX FIFO should have received 0xC3 from null device
        assert_eq!(spi.rfve, 1, "Expected 1 entry in RX FIFO");
        // The RX FIFO should contain 0xC3
        assert_eq!(spi.rx_fifo[0], 0xC3, "Expected 0xC3 from ARM coprocessor");
    }

    #[test]
    fn test_rx_fifo_read_drains() {
        let mut spi = SpiController::new();

        // Manually populate RX FIFO for testing
        spi.rx_fifo[0] = 0x12345678;
        spi.rfve = 1;

        // Read DATA register byte 0 (this should drain the FIFO)
        let byte0 = spi.read(0x18, 0, CPU_SPEED_24MHZ);
        assert_eq!(byte0, 0x78); // Little-endian, lowest byte
        assert_eq!(spi.rfve, 0); // FIFO should be drained
    }

    #[test]
    fn test_fifo_reset_via_cr2() {
        let mut spi = SpiController::new();

        // Add data to both FIFOs
        spi.tx_fifo[0] = 0xAAAA;
        spi.tfve = 3;
        spi.tfvi = 5;
        spi.rx_fifo[0] = 0xBBBB;
        spi.rfve = 2;
        spi.rfvi = 7;

        // Reset TX FIFO (bit 3)
        spi.write(0x08, 0x08, 0, CPU_SPEED_24MHZ);
        assert_eq!(spi.tfve, 0);
        assert_eq!(spi.tfvi, 0);
        assert_eq!(spi.rfve, 2); // RX unchanged

        // Reset RX FIFO (bit 2)
        spi.write(0x08, 0x04, 0, CPU_SPEED_24MHZ);
        assert_eq!(spi.rfve, 0);
        assert_eq!(spi.rfvi, 0);
    }

    #[test]
    fn test_loopback_mode() {
        let mut spi = SpiController::new();

        // Enable loopback (CR0 bit 7)
        spi.cr0 |= 1 << 7;

        // Enable SPI, TX, and RX
        spi.write(0x08, 0x01, 0, CPU_SPEED_24MHZ);
        spi.write(0x09, 0x01, 0, CPU_SPEED_24MHZ);
        spi.cr2 |= 1 << 7; // RX enable

        // CR1: divider = 1, bit count = 8
        spi.write(0x04, 0x00, 0, CPU_SPEED_24MHZ);
        spi.write(0x06, 0x07, 0, CPU_SPEED_24MHZ);

        // Queue a transfer with known data
        spi.write(0x18, 0x5A, 0, CPU_SPEED_24MHZ);

        // Wait for transfer to complete
        spi.update(100, CPU_SPEED_24MHZ);

        // In loopback mode, RX should receive what we sent
        assert_eq!(spi.rfve, 1);
        assert_eq!(spi.rx_fifo[0], 0x5A);
    }

    #[test]
    fn test_tx_fifo_full() {
        let mut spi = SpiController::new();

        // Fill TX FIFO
        for i in 0..SPI_TXFIFO_DEPTH {
            spi.write(0x18, i as u8, 0, CPU_SPEED_24MHZ);
        }

        assert_eq!(spi.tfve, 16);

        // Status should show TX FIFO full (bit 1 clear)
        let status0 = spi.read(0x0C, 0, CPU_SPEED_24MHZ);
        assert_eq!(status0 & 0x02, 0x00); // TX full

        // Additional writes should not increase count
        spi.write(0x18, 0xFF, 0, CPU_SPEED_24MHZ);
        assert_eq!(spi.tfve, 16);
    }

    #[test]
    fn test_feature_register() {
        let mut spi = SpiController::new();

        // Read FEATURE register (0x1C)
        let feat0 = spi.read(0x1C, 0, CPU_SPEED_24MHZ);
        let feat1 = spi.read(0x1D, 0, CPU_SPEED_24MHZ);
        let feat2 = spi.read(0x1E, 0, CPU_SPEED_24MHZ);
        let feat3 = spi.read(0x1F, 0, CPU_SPEED_24MHZ);

        // WIDTH - 1 = 31
        assert_eq!(feat0, 31);
        // RXFIFO_DEPTH - 1 = 15
        assert_eq!(feat1, 15);
        // TXFIFO_DEPTH - 1 = 15
        assert_eq!(feat2, 15);
        // FEATURES = 0xE
        assert_eq!(feat3, 0xE);
    }

    #[test]
    fn test_revision_register() {
        let mut spi = SpiController::new();

        // Read REVISION register (0x60)
        let rev0 = spi.read(0x60, 0, CPU_SPEED_24MHZ);
        let rev1 = spi.read(0x61, 0, CPU_SPEED_24MHZ);
        let rev2 = spi.read(0x62, 0, CPU_SPEED_24MHZ);

        assert_eq!(rev0, 0x00);
        assert_eq!(rev1, 0x21);
        assert_eq!(rev2, 0x01);
    }

    #[test]
    fn test_threshold_interrupts() {
        let mut spi = SpiController::new();

        // Set TX threshold to 2 (bits 12-16 of INTCTRL)
        spi.write(0x11, 0x20, 0, CPU_SPEED_24MHZ); // 2 << 4 in byte 1

        // With tfve=0, threshold status should be set (tfve <= threshold)
        let status = spi.get_threshold_status();
        assert_eq!(status & (1 << 3), 1 << 3); // TX threshold hit

        // Add entries to exceed threshold
        spi.tfve = 3;
        let status = spi.get_threshold_status();
        assert_eq!(status & (1 << 3), 0); // TX threshold not hit
    }

    #[test]
    fn test_48mhz_transfer_timing() {
        let mut spi = SpiController::new();

        // Enable SPI and TX at 48MHz
        spi.write(0x08, 0x01, 0, CPU_SPEED_48MHZ);
        spi.write(0x09, 0x01, 0, CPU_SPEED_48MHZ);

        // CR1: divider = 1, bit count = 8
        spi.write(0x04, 0x00, 0, CPU_SPEED_48MHZ);
        spi.write(0x06, 0x07, 0, CPU_SPEED_48MHZ);

        // Queue transfer
        spi.write(0x18, 0x00, 0, CPU_SPEED_48MHZ);

        // At 48MHz CPU with 24MHz SPI, timing is different
        // 8 bits * 1 divider = 8 SPI ticks = 16 CPU cycles at 48MHz
        let status0 = spi.read(0x0C, 15, CPU_SPEED_48MHZ);
        assert_eq!(status0 & 0x04, 0x04); // Still active

        let status0_done = spi.read(0x0C, 16, CPU_SPEED_48MHZ);
        assert_eq!(status0_done & 0x04, 0x00); // Complete
    }
}
