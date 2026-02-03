//! SPI Controller Stub
//!
//! Memory-mapped at port range 0xD (0xD000-0xDFFF via IN/OUT)
//!
//! This is a minimal implementation for boot parity with CEmu.
//! Timing is based on CPU cycles with a 24 MHz SPI clock model.

/// SPI FIFO depth (matches CEmu)
const SPI_RXFIFO_DEPTH: u8 = 16;
const SPI_TXFIFO_DEPTH: u8 = 16;

/// SPI feature flags (matches CEmu)
const SPI_FEATURES: u8 = 0xE;
const SPI_WIDTH: u8 = 32;

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
    /// Cycle when the current transfer completes
    next_event_cycle: Option<u64>,
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
            next_event_cycle: None,
        }
    }

    /// Reset the SPI controller
    pub fn reset(&mut self) {
        *self = Self::new();
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
    /// CEmu always uses (divider + 1) for all transfers
    fn transfer_ticks(&self) -> u64 {
        let bit_count = self.transfer_bit_count() as u64;
        let divider = (self.cr1 & 0xFFFF) as u64 + 1;
        bit_count * divider.max(1)
    }

    /// Compute the next event cycle using CEmu-like tick conversion
    fn next_event_cycle(&self, base_cycle: u64, cpu_speed: u8, ticks: u64, round_up: bool) -> u64 {
        let cpu_hz = Self::cpu_clock_hz(cpu_speed) as u64;
        let base_tick = ((base_cycle as u128) * (Self::SPI_CLOCK_HZ as u128)) / (cpu_hz as u128);
        let next_tick = base_tick + ticks as u128;
        let next_cycle = if round_up {
            (next_tick * (cpu_hz as u128) + (Self::SPI_CLOCK_HZ as u128 - 1))
                / (Self::SPI_CLOCK_HZ as u128)
        } else {
            (next_tick * (cpu_hz as u128)) / (Self::SPI_CLOCK_HZ as u128)
        };
        (next_cycle as u64).max(base_cycle.saturating_add(1))
    }

    fn trace_enabled() -> bool {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| std::env::var_os("SPI_TRACE").is_some())
    }

    fn start_transfer(&mut self, base_cycle: u64, cpu_speed: u8) -> bool {
        if self.transfer_bits != 0 || !self.spi_enabled() {
            return false;
        }

        let tx_enabled = self.tx_enabled();
        let rx_enabled = self.rx_enabled();
        let tx_available = tx_enabled && self.tfve != 0;

        if rx_enabled {
            if !self.flash_enabled() && tx_enabled && self.tfve == 0 {
                return false;
            }
            let rfve_limit = if self.tfve == 0 {
                SPI_RXFIFO_DEPTH.saturating_sub(1)
            } else {
                SPI_RXFIFO_DEPTH
            };
            if self.rfve >= rfve_limit {
                return false;
            }
        } else if !tx_available {
            return false;
        }

        let queued_before = self.tfve;
        if tx_available {
            self.tfve = self.tfve.saturating_sub(1);
            self.tfvi = self.tfvi.wrapping_add(1);
        }
        self.transfer_bits = self.transfer_bit_count();

        // CEmu always uses (divider + 1) for transfer timing
        let ticks = self.transfer_ticks();
        let next_cycle = self.next_event_cycle(base_cycle, cpu_speed, ticks, tx_available);
        self.next_event_cycle = Some(next_cycle);

        if Self::trace_enabled() {
            eprintln!(
                "[spi] start cycle={} next={} queued_before={} queued_after={} bits={} divider={} tx={} rx={} flash={}",
                base_cycle,
                next_cycle,
                queued_before,
                self.tfve,
                self.transfer_bits,
                (self.cr1 & 0xFFFF) + 1,
                tx_enabled as u8,
                rx_enabled as u8,
                self.flash_enabled() as u8
            );
        }

        true
    }

    /// Advance SPI transfers based on current CPU cycles
    fn update(&mut self, current_cycles: u64, cpu_speed: u8) {
        if !self.spi_enabled() {
            self.transfer_bits = 0;
            self.next_event_cycle = None;
            return;
        }

        while let Some(next_cycle) = self.next_event_cycle {
            if current_cycles < next_cycle {
                break;
            }

            if Self::trace_enabled() {
                eprintln!(
                    "[spi] complete cycle={} now={} queued={} transfer_bits={}",
                    next_cycle,
                    current_cycles,
                    self.tfve,
                    self.transfer_bits
                );
            }

            self.transfer_bits = 0;
            self.next_event_cycle = None;

            if self.rx_enabled() && self.rfve < SPI_RXFIFO_DEPTH {
                self.rfve = self.rfve.saturating_add(1);
                self.rfvi = self.rfvi.wrapping_add(1);
            }

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
                let rx_full = if self.rfve >= SPI_RXFIFO_DEPTH { 1 } else { 0 };
                let transfer_active = if self.transfer_bits != 0 { 1 } else { 0 };
                let status = ((self.tfve as u32) << 12)
                    | ((self.rfve as u32) << 4)
                    | (transfer_active << 2)
                    | (tx_not_full << 1)
                    | rx_full;
                if Self::trace_enabled() {
                    eprintln!(
                        "[spi] status cycle={} speed={} tfve={} rfve={} active={} next={:?} cr0=0x{:04X} cr1=0x{:06X} cr2=0x{:03X}",
                        current_cycles,
                        cpu_speed & 0x03,
                        self.tfve,
                        self.rfve,
                        transfer_active,
                        self.next_event_cycle,
                        self.cr0,
                        self.cr1,
                        self.cr2
                    );
                }
                status
            }
            // INTCTRL (0x10-0x13)
            4 => self.int_ctrl,
            // INTSTATUS (0x14-0x17)
            5 => self.int_status,
            // DATA (0x18-0x1B) - reading drains RX FIFO
            6 => {
                if shift == 0 && self.rfve > 0 {
                    self.rfve = self.rfve.saturating_sub(1);
                    self.rfvi = self.rfvi.wrapping_add(1);
                }
                0
            }
            // FEATURE (0x1C-0x1F)
            7 => {
                // Features: TXFIFO_DEPTH-1, RXFIFO_DEPTH-1, WIDTH-1
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
    /// Returns true if SPI state changed in a way that may need scheduler update
    pub fn write(&mut self, addr: u32, value: u8, _current_cycles: u64, _cpu_speed: u8) -> bool {
        // Note: We no longer call update() here - timing is driven by scheduler

        let shift = (addr & 3) << 3;
        let value32 = (value as u32) << shift;
        let mask = !(0xFF_u32 << shift);
        let mut state_changed = false;

        match addr >> 2 {
            // CR0 (0x00-0x03)
            0 => {
                let new_value = (self.cr0 & mask) | (value32 & 0xFFFF);
                if Self::trace_enabled() && new_value != self.cr0 {
                    eprintln!("[spi] cr0 write value=0x{:04X}", new_value);
                }
                self.cr0 = new_value;
            }
            // CR1 (0x04-0x07)
            1 => {
                let new_value = (self.cr1 & mask) | (value32 & 0x7FFFFF);
                if Self::trace_enabled() && new_value != self.cr1 {
                    eprintln!("[spi] cr1 write value=0x{:06X}", new_value);
                }
                self.cr1 = new_value;
            }
            // CR2 (0x08-0x0B)
            2 => {
                let old_cr2 = self.cr2;
                let mut masked_value = value32;
                // Bit 2: Reset RX FIFO
                if masked_value & (1 << 2) != 0 {
                    self.rfvi = 0;
                    self.rfve = 0;
                }
                // Bit 3: Reset TX FIFO
                if masked_value & (1 << 3) != 0 {
                    self.tfvi = 0;
                    self.tfve = 0;
                }
                // Only low CR2 bits are writable (matches CEmu mask)
                masked_value &= 0xF83;
                let new_value = (self.cr2 & mask) | masked_value;
                if Self::trace_enabled() && new_value != self.cr2 {
                    eprintln!("[spi] cr2 write value=0x{:03X}", new_value);
                }
                self.cr2 = new_value;

                // Check if state changed in a way that affects scheduling
                // CEmu: if ((spi.cr2 ^ value) & ~mask & (1 << 8 | 1 << 7 | 1 << 0)) stateChanged = true
                let relevant_bits = (1 << 8) | (1 << 7) | (1 << 0); // TX_EN, RX_EN, SPI_EN
                if (old_cr2 ^ new_value) & relevant_bits != 0 {
                    state_changed = true;
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
                    state_changed = true; // May need to start transfer
                    if Self::trace_enabled() {
                        eprintln!(
                            "[spi] data write tfve={} cr2=0x{:03X}",
                            self.tfve,
                            self.cr2
                        );
                    }
                }
            }
            _ => {}
        }
        state_changed
    }
}

// === Scheduler integration methods ===
// These methods allow the scheduler to drive SPI timing precisely,
// matching CEmu's sched_set(SCHED_SPI, ticks) approach.

impl SpiController {
    /// Complete a transfer and try to start the next one.
    /// Returns Some(ticks) if a new transfer was started, None otherwise.
    /// Called by scheduler when SPI event fires.
    pub fn complete_transfer_and_continue(&mut self) -> Option<u64> {
        if Self::trace_enabled() {
            eprintln!(
                "[spi] sched_complete queued={} transfer_bits={}",
                self.tfve,
                self.transfer_bits
            );
        }

        // Complete current transfer
        if self.transfer_bits != 0 {
            self.transfer_bits = 0;
            self.next_event_cycle = None;

            // Add to RX FIFO if RX enabled
            if self.rx_enabled() && self.rfve < SPI_RXFIFO_DEPTH {
                self.rfve = self.rfve.saturating_add(1);
                self.rfvi = self.rfvi.wrapping_add(1);
            }
        }

        // Try to start next transfer
        self.try_start_transfer_for_scheduler()
    }

    /// Try to start a transfer. Returns Some(ticks) if successful.
    /// Called after enabling SPI or after a transfer completes.
    pub fn try_start_transfer_for_scheduler(&mut self) -> Option<u64> {
        if self.transfer_bits != 0 || !self.spi_enabled() {
            return None;
        }

        let tx_enabled = self.tx_enabled();
        let rx_enabled = self.rx_enabled();
        let tx_available = tx_enabled && self.tfve != 0;

        if rx_enabled {
            if !self.flash_enabled() && tx_enabled && self.tfve == 0 {
                return None;
            }
            let rfve_limit = if self.tfve == 0 {
                SPI_RXFIFO_DEPTH.saturating_sub(1)
            } else {
                SPI_RXFIFO_DEPTH
            };
            if self.rfve >= rfve_limit {
                return None;
            }
        } else if !tx_available {
            return None;
        }

        // Consume from TX FIFO
        let queued_before = self.tfve;
        if tx_available {
            self.tfve = self.tfve.saturating_sub(1);
            self.tfvi = self.tfvi.wrapping_add(1);
        }

        self.transfer_bits = self.transfer_bit_count();

        // Calculate ticks for scheduler (24 MHz clock)
        // CEmu: bitCount * ((spi.cr1 & 0xFFFF) + 1)
        let ticks = self.transfer_ticks();

        if Self::trace_enabled() {
            eprintln!(
                "[spi] sched_start queued_before={} queued_after={} bits={} ticks={} tx={} rx={}",
                queued_before,
                self.tfve,
                self.transfer_bits,
                ticks,
                tx_enabled as u8,
                rx_enabled as u8
            );
        }

        Some(ticks)
    }

    /// Called when SPI is disabled - clears any pending transfer state
    pub fn cancel_transfer(&mut self) {
        self.transfer_bits = 0;
        self.next_event_cycle = None;
    }

    /// Check if there's an active transfer in progress
    pub fn is_transfer_active(&self) -> bool {
        self.transfer_bits != 0
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
        assert_eq!(spi.tfve, 1); // Directly check tfve increased

        // STATUS byte 1 should report tfve = 1 (upper nibble)
        let status1 = spi.read(0x0D, 0, CPU_SPEED_24MHZ);
        assert_eq!(status1, 0x10);
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
}
