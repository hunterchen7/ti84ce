//! System bus for TI-84 Plus CE
//!
//! The bus provides address decoding and routes memory accesses to the
//! appropriate memory region (Flash, RAM, Ports, or unmapped).
//!
//! Memory Map (24-bit address space):
//! ```text
//! 0x000000 - 0x3FFFFF : Flash (4MB)
//! 0x400000 - 0xCFFFFF : Unmapped
//! 0xD00000 - 0xD657FF : RAM (including VRAM)
//! 0xD65800 - 0xDFFFFF : Unmapped
//! 0xE00000 - 0xFFFFFF : Memory-mapped I/O
//! ```
//!
//! Reference: CEmu (https://github.com/CE-Programming/CEmu)

use crate::memory::{addr, Flash, FlashError, Ports, Ram};
use crate::peripherals::SpiController;
use std::collections::BTreeMap;

/// Bus access type for debugging/tracing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessType {
    /// Instruction fetch
    Fetch,
    /// Data read
    Read,
    /// Data write
    Write,
}

/// Memory region that an address maps to
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryRegion {
    /// Flash memory (0x000000 - 0x3FFFFF)
    Flash,
    /// RAM (0xD00000 - 0xD657FF)
    Ram,
    /// VRAM portion of RAM (0xD40000 - 0xD657FF)
    Vram,
    /// Memory-mapped I/O (0xE00000 - 0xFFFFFF)
    Ports,
    /// Unmapped region
    Unmapped,
}

/// Simple pseudo-random generator for unmapped reads
/// Based on CEmu's bus_rand implementation
struct BusRng {
    state: [u8; 3],
}

impl BusRng {
    fn new() -> Self {
        Self { state: [0x9A, 0x59, 0xC6] }
    }

    fn seed(&mut self, s1: u8, s2: u8, s3: u8) {
        self.state = [s1, s2, s3];
    }

    /// Generate next pseudo-random byte
    fn next(&mut self) -> u8 {
        // Simple LFSR-style generator
        let bit = ((self.state[0] >> 7) ^ (self.state[0] >> 5) ^
                   (self.state[0] >> 4) ^ (self.state[0] >> 3)) & 1;
        let result = self.state[0];
        self.state[0] = (self.state[0] << 1) | ((self.state[1] >> 7) & 1);
        self.state[1] = (self.state[1] << 1) | ((self.state[2] >> 7) & 1);
        self.state[2] = (self.state[2] << 1) | bit;
        result
    }
}

/// Flash unlock sequence detected during instruction fetch
/// This specific sequence in the ROM unlocks flash write access.
/// From CEmu mem.c: triggers when BIT 2, A is fetched after the unlock sequence
/// The sequence must be fetched by PRIVILEGED code (PC <= privileged boundary)
/// NOTE: Some ROMs use a single DI, others use double DI before IM 2/IM 1.
const FLASH_UNLOCK_SEQUENCE: [u8; 16] = [
    0xF3, 0x18, 0x00, // DI; JR 0
    0xF3,             // DI (single, not double like CEmu's sequence)
    0xED, 0x7E,       // IM 2
    0xED, 0x56,       // IM 1
    0xED, 0x39, 0x28, // OUT0 (0x28), A
    0xED, 0x38, 0x28, // IN0 A, (0x28)
    0xCB, 0x57,       // BIT 2, A - detection triggers on this last byte
];

/// Alternate unlock sequence with double DI (CEmu reference)
const FLASH_UNLOCK_SEQUENCE_DOUBLE_DI: [u8; 17] = [
    0xF3, 0x18, 0x00, // DI; JR 0
    0xF3, 0xF3,       // DI, DI (double)
    0xED, 0x7E,       // IM 2
    0xED, 0x56,       // IM 1
    0xED, 0x39, 0x28, // OUT0 (0x28), A
    0xED, 0x38, 0x28, // IN0 A, (0x28)
    0xCB, 0x57,       // BIT 2, A - detection triggers on this last byte
];

/// Size of the fetch buffer for sequence detection
const FETCH_BUFFER_SIZE: usize = 32;

/// Maximum number of unique write addresses to track before stopping
const MAX_TRACKED_WRITES: usize = 10000;

/// A single recorded write operation for detailed tracing
#[derive(Debug, Clone, Copy)]
pub struct WriteRecord {
    /// Address written to (absolute, in RAM range 0xD00000-0xD657FF)
    pub addr: u32,
    /// Value written
    pub value: u8,
    /// Bus cycle when write occurred
    pub cycle: u64,
}

/// Write tracer for debugging RAM writes during boot
///
/// This is designed for investigating boot behavior to determine
/// if/when RAM is being initialized.
pub struct WriteTracer {
    /// Whether tracing is enabled
    enabled: bool,
    /// Total count of RAM writes
    total_writes: u64,
    /// Per-address write counts (address -> count)
    address_counts: BTreeMap<u32, u32>,
    /// First N writes recorded in detail
    detailed_log: Vec<WriteRecord>,
    /// Max detailed records to keep
    max_detailed: usize,
    /// Address range filter (start, end) - only track writes in this range
    /// If None, track all RAM writes
    filter_range: Option<(u32, u32)>,
}

impl WriteTracer {
    /// Create a new disabled write tracer
    pub fn new() -> Self {
        Self {
            enabled: false,
            total_writes: 0,
            address_counts: BTreeMap::new(),
            detailed_log: Vec::new(),
            max_detailed: 1000,
            filter_range: None,
        }
    }

    /// Enable tracing
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable tracing
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Check if tracing is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set a filter range - only track writes within [start, end)
    /// Addresses are absolute (0xD00000 range)
    pub fn set_filter_range(&mut self, start: u32, end: u32) {
        self.filter_range = Some((start, end));
    }

    /// Clear the filter range (track all RAM writes)
    pub fn clear_filter_range(&mut self) {
        self.filter_range = None;
    }

    /// Set the maximum number of detailed records to keep
    pub fn set_max_detailed(&mut self, max: usize) {
        self.max_detailed = max;
    }

    /// Record a write operation
    pub fn record(&mut self, addr: u32, value: u8, cycle: u64) {
        if !self.enabled {
            return;
        }

        // Apply filter if set
        if let Some((start, end)) = self.filter_range {
            if addr < start || addr >= end {
                return;
            }
        }

        self.total_writes += 1;

        // Track per-address counts (with limit to prevent memory explosion)
        if self.address_counts.len() < MAX_TRACKED_WRITES {
            *self.address_counts.entry(addr).or_insert(0) += 1;
        }

        // Keep detailed log of first N writes
        if self.detailed_log.len() < self.max_detailed {
            self.detailed_log.push(WriteRecord { addr, value, cycle });
        }
    }

    /// Reset all tracking data
    pub fn reset(&mut self) {
        self.total_writes = 0;
        self.address_counts.clear();
        self.detailed_log.clear();
    }

    /// Get total number of writes recorded
    pub fn total_writes(&self) -> u64 {
        self.total_writes
    }

    /// Get number of unique addresses written to
    pub fn unique_addresses(&self) -> usize {
        self.address_counts.len()
    }

    /// Get the detailed write log
    pub fn detailed_log(&self) -> &[WriteRecord] {
        &self.detailed_log
    }

    /// Get addresses sorted by write count (descending)
    pub fn top_addresses(&self, limit: usize) -> Vec<(u32, u32)> {
        let mut sorted: Vec<_> = self.address_counts.iter()
            .map(|(&addr, &count)| (addr, count))
            .collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));
        sorted.truncate(limit);
        sorted
    }

    /// Get the lowest and highest addresses written to
    pub fn address_range(&self) -> Option<(u32, u32)> {
        if self.address_counts.is_empty() {
            None
        } else {
            let min = *self.address_counts.keys().next().unwrap();
            let max = *self.address_counts.keys().next_back().unwrap();
            Some((min, max))
        }
    }

    /// Check if a specific address was written to
    pub fn was_written(&self, addr: u32) -> bool {
        self.address_counts.contains_key(&addr)
    }

    /// Get write count for a specific address
    pub fn write_count(&self, addr: u32) -> u32 {
        self.address_counts.get(&addr).copied().unwrap_or(0)
    }

    /// Generate a summary report
    pub fn summary(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("=== RAM Write Trace Summary ===\n"));
        s.push_str(&format!("Tracing enabled: {}\n", self.enabled));
        s.push_str(&format!("Total writes: {}\n", self.total_writes));
        s.push_str(&format!("Unique addresses: {}\n", self.unique_addresses()));

        if let Some((min, max)) = self.address_range() {
            s.push_str(&format!("Address range: 0x{:06X} - 0x{:06X}\n", min, max));
        } else {
            s.push_str("Address range: (no writes)\n");
        }

        if !self.address_counts.is_empty() {
            s.push_str("\nTop 20 written addresses:\n");
            for (addr, count) in self.top_addresses(20) {
                s.push_str(&format!("  0x{:06X}: {} writes\n", addr, count));
            }
        }

        if !self.detailed_log.is_empty() {
            s.push_str(&format!("\nFirst {} writes:\n", self.detailed_log.len().min(50)));
            for (i, rec) in self.detailed_log.iter().take(50).enumerate() {
                s.push_str(&format!(
                    "  {:4}: cycle {:8} | 0x{:06X} <- 0x{:02X}\n",
                    i, rec.cycle, rec.addr, rec.value
                ));
            }
            if self.detailed_log.len() > 50 {
                s.push_str(&format!("  ... and {} more\n", self.detailed_log.len() - 50));
            }
        }

        s
    }
}

impl Default for WriteTracer {
    fn default() -> Self {
        Self::new()
    }
}

/// System bus connecting CPU to memory subsystems
pub struct Bus {
    /// Flash memory
    pub flash: Flash,
    /// RAM (including VRAM)
    pub ram: Ram,
    /// Memory-mapped I/O peripherals
    pub ports: Ports,
    /// SPI controller (port range 0xD)
    spi: SpiController,
    /// RNG for unmapped region reads
    rng: BusRng,
    /// Cycle counter for timing
    cycles: u64,
    /// Circular buffer of recently fetched instruction bytes
    fetch_buffer: [u8; FETCH_BUFFER_SIZE],
    /// Current index in fetch buffer (points to most recent byte + 1)
    fetch_index: usize,
    /// Write tracer for debugging RAM writes
    pub write_tracer: WriteTracer,
}

impl Bus {
    /// Wait states for different memory regions
    /// These affect CPU timing for accurate emulation
    pub const FLASH_READ_CYCLES: u64 = 10;  // ~4 wait states + fetch
    pub const RAM_READ_CYCLES: u64 = 4;     // 3 wait states + 1
    pub const RAM_WRITE_CYCLES: u64 = 2;    // 1 wait state + 1
    pub const UNMAPPED_CYCLES: u64 = 2;

    /// Per-port-range read cycles (indexed by port range 0x0-0xF)
    /// From CEmu port.c: {2,2,2,4,3,3,3,3,3,3,3,3,3,3,3,3}
    /// Port ranges: 0=Control, 1=Flash, 2=SHA256, 3=USB, 4=LCD, 5=Interrupt,
    ///              6=Watchdog, 7=Timers, 8=RTC, 9=Protected, A=Keypad,
    ///              B=Backlight, C=Cxxx, D=SPI, E=UART, F=Control (alt)
    const PORT_READ_CYCLES: [u64; 16] = [2, 2, 2, 4, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3];

    /// Per-port-range write cycles (indexed by port range 0x0-0xF)
    /// From CEmu port.c: {2,2,2,4,2,3,3,3,3,3,3,3,3,3,3,3}
    const PORT_WRITE_CYCLES: [u64; 16] = [2, 2, 2, 4, 2, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3];

    /// Memory-mapped I/O read cycles (used for read_byte at 0xE00000+)
    /// This is the default timing; IN/OUT use per-port timing above
    const MMIO_READ_CYCLES: u64 = 3;

    /// Memory-mapped I/O write cycles (used for write_byte at 0xE00000+)
    const MMIO_WRITE_CYCLES: u64 = 3;

    /// Create a new bus with fresh memory
    pub fn new() -> Self {
        Self {
            flash: Flash::new(),
            ram: Ram::new(),
            ports: Ports::new(),
            spi: SpiController::new(),
            rng: BusRng::new(),
            cycles: 0,
            fetch_buffer: [0; FETCH_BUFFER_SIZE],
            fetch_index: 0,
            write_tracer: WriteTracer::new(),
        }
    }

    /// Determine which memory region an address maps to
    pub fn decode_address(addr: u32) -> MemoryRegion {
        let addr = addr & addr::ADDR_MASK;

        if addr < addr::FLASH_END {
            MemoryRegion::Flash
        } else if addr < addr::RAM_START {
            MemoryRegion::Unmapped
        } else if addr < addr::RAM_END {
            if addr >= addr::VRAM_START {
                MemoryRegion::Vram
            } else {
                MemoryRegion::Ram
            }
        } else if addr < addr::PORT_START {
            MemoryRegion::Unmapped
        } else {
            MemoryRegion::Ports
        }
    }

    /// Read a byte from the bus
    ///
    /// # Arguments
    /// * `addr` - 24-bit address
    ///
    /// # Returns
    /// The byte at the given address
    pub fn read_byte(&mut self, addr: u32) -> u8 {
        let addr = addr & addr::ADDR_MASK;

        match Self::decode_address(addr) {
            MemoryRegion::Flash => {
                self.cycles += Self::FLASH_READ_CYCLES;
                self.flash.read(addr)
            }
            MemoryRegion::Ram | MemoryRegion::Vram => {
                self.cycles += Self::RAM_READ_CYCLES;
                self.ram.read(addr - addr::RAM_START)
            }
            MemoryRegion::Ports => {
                self.cycles += Self::MMIO_READ_CYCLES;
                let keys = *self.ports.key_state();
                self.ports.read(addr - addr::PORT_START, &keys, self.cycles)
            }
            MemoryRegion::Unmapped => {
                self.cycles += Self::UNMAPPED_CYCLES;
                self.rng.next()
            }
        }
    }

    /// Fetch a byte for instruction execution
    /// This records the byte in the fetch buffer for flash unlock sequence detection
    ///
    /// # Arguments
    /// * `addr` - 24-bit address to fetch from
    /// * `pc` - Current program counter (for privilege check)
    ///
    /// # Returns
    /// The byte at the given address
    pub fn fetch_byte(&mut self, addr: u32, pc: u32) -> u8 {
        let addr = addr & addr::ADDR_MASK;
        let is_flash = matches!(Self::decode_address(addr), MemoryRegion::Flash);

        let value = match Self::decode_address(addr) {
            MemoryRegion::Flash => {
                self.cycles += Self::FLASH_READ_CYCLES;
                self.flash.read(addr)
            }
            MemoryRegion::Ram | MemoryRegion::Vram => {
                self.cycles += Self::RAM_READ_CYCLES;
                self.ram.read(addr - addr::RAM_START)
            }
            MemoryRegion::Ports => {
                self.cycles += Self::MMIO_READ_CYCLES;
                let keys = *self.ports.key_state();
                self.ports.read(addr - addr::PORT_START, &keys, self.cycles)
            }
            MemoryRegion::Unmapped => {
                self.cycles += Self::UNMAPPED_CYCLES;
                self.rng.next()
            }
        };

        // CEmu: When fetching from flash, check for unlock sequence BEFORE updating buffer
        // Only privileged code can trigger the unlock (is_unprivileged returns false)
        // The detection must happen BEFORE we add the current byte to the buffer,
        // because the buffer should contain the previous N-1 bytes of the sequence
        if is_flash && self.detect_flash_unlock_sequence(value, pc) {
            self.ports.control.set_flash_ready();
        }

        // Record in fetch buffer AFTER checking (like CEmu's check then mem.buffer[++mem.fetch] = value)
        self.fetch_buffer[self.fetch_index] = value;
        self.fetch_index = (self.fetch_index + 1) % FETCH_BUFFER_SIZE;

        // CEmu: If flash is unlocked AND unprivileged code is fetching, clear the unlock
        // This happens after the buffer update
        if self.ports.control.flash_ready() && self.ports.control.is_unprivileged(pc) {
            self.ports.control.clear_flash_ready();
        }

        value
    }

    /// Check if the fetch buffer contains the flash unlock sequence
    /// CEmu: Only triggers for privileged code (unprivileged_code() returns false)
    fn detect_flash_unlock_sequence(&self, current: u8, pc: u32) -> bool {
        // The sequence ends with 0x57 (last byte of BIT 2, A)
        if current != FLASH_UNLOCK_SEQUENCE[FLASH_UNLOCK_SEQUENCE.len() - 1] {
            return false;
        }

        // Protected ports must be unlocked (port 0x06 bit 2)
        if !self.ports.control.protected_ports_unlocked() {
            return false;
        }

        // CEmu: Only privileged code can unlock flash
        if self.ports.control.is_unprivileged(pc) {
            return false;
        }

        // Accept either single-DI or double-DI sequences.
        self.matches_flash_unlock_sequence(&FLASH_UNLOCK_SEQUENCE)
            || self.matches_flash_unlock_sequence(&FLASH_UNLOCK_SEQUENCE_DOUBLE_DI)
    }

    fn matches_flash_unlock_sequence(&self, sequence: &[u8]) -> bool {
        for i in 1..sequence.len() {
            let buf_idx = (self.fetch_index + FETCH_BUFFER_SIZE - i) % FETCH_BUFFER_SIZE;
            if self.fetch_buffer[buf_idx] != sequence[sequence.len() - 1 - i] {
                return false;
            }
        }
        true
    }


    /// Write a byte to the bus
    ///
    /// # Arguments
    /// * `addr` - 24-bit address
    /// * `value` - Byte to write
    pub fn write_byte(&mut self, addr: u32, value: u8) {
        let addr = addr & addr::ADDR_MASK;

        match Self::decode_address(addr) {
            MemoryRegion::Flash => {
                // Flash writes are ignored unless flash is unlocked
                self.cycles += Self::UNMAPPED_CYCLES;
                if self.ports.control.flash_unlocked() {
                    self.flash.write_cpu(addr, value);
                }
            }
            MemoryRegion::Ram | MemoryRegion::Vram => {
                self.cycles += Self::RAM_WRITE_CYCLES;
                // Record write for tracing (before actually writing)
                if self.write_tracer.is_enabled() {
                    self.write_tracer.record(addr, value, self.cycles);
                }
                self.ram.write(addr - addr::RAM_START, value);
            }
            MemoryRegion::Ports => {
                self.cycles += Self::MMIO_WRITE_CYCLES;
                self.ports.write(addr - addr::PORT_START, value, self.cycles);
            }
            MemoryRegion::Unmapped => {
                // Writes to unmapped regions are ignored
                self.cycles += Self::UNMAPPED_CYCLES;
            }
        }
    }

    /// Read a 16-bit word (little-endian)
    pub fn read_word(&mut self, addr: u32) -> u16 {
        let lo = self.read_byte(addr) as u16;
        let hi = self.read_byte(addr.wrapping_add(1)) as u16;
        lo | (hi << 8)
    }

    /// Write a 16-bit word (little-endian)
    pub fn write_word(&mut self, addr: u32, value: u16) {
        self.write_byte(addr, value as u8);
        self.write_byte(addr.wrapping_add(1), (value >> 8) as u8);
    }

    /// Read a 24-bit address (little-endian, for eZ80 ADL mode)
    pub fn read_addr24(&mut self, addr: u32) -> u32 {
        let b0 = self.read_byte(addr) as u32;
        let b1 = self.read_byte(addr.wrapping_add(1)) as u32;
        let b2 = self.read_byte(addr.wrapping_add(2)) as u32;
        b0 | (b1 << 8) | (b2 << 16)
    }

    /// Write a 24-bit address (little-endian)
    pub fn write_addr24(&mut self, addr: u32, value: u32) {
        self.write_byte(addr, value as u8);
        self.write_byte(addr.wrapping_add(1), (value >> 8) as u8);
        self.write_byte(addr.wrapping_add(2), (value >> 16) as u8);
    }

    /// Read a 32-bit value (little-endian)
    pub fn read_dword(&mut self, addr: u32) -> u32 {
        let b0 = self.read_byte(addr) as u32;
        let b1 = self.read_byte(addr.wrapping_add(1)) as u32;
        let b2 = self.read_byte(addr.wrapping_add(2)) as u32;
        let b3 = self.read_byte(addr.wrapping_add(3)) as u32;
        b0 | (b1 << 8) | (b2 << 16) | (b3 << 24)
    }

    /// Write a 32-bit value (little-endian)
    pub fn write_dword(&mut self, addr: u32, value: u32) {
        self.write_byte(addr, value as u8);
        self.write_byte(addr.wrapping_add(1), (value >> 8) as u8);
        self.write_byte(addr.wrapping_add(2), (value >> 16) as u8);
        self.write_byte(addr.wrapping_add(3), (value >> 24) as u8);
    }

    /// Peek at a byte without affecting cycles (for debugging)
    pub fn peek_byte(&mut self, addr: u32) -> u8 {
        let addr = addr & addr::ADDR_MASK;

        match Self::decode_address(addr) {
            MemoryRegion::Flash => self.flash.peek(addr),
            MemoryRegion::Ram | MemoryRegion::Vram => {
                self.ram.read(addr - addr::RAM_START)
            }
            MemoryRegion::Ports => {
                let keys = *self.ports.key_state();
                // Use 0 for cycles in debug peek (no timing effects)
                self.ports.read(addr - addr::PORT_START, &keys, 0)
            }
            MemoryRegion::Unmapped => 0x00,
        }
    }

    /// Peek a byte as it would be fetched by the CPU (includes flash command status)
    pub fn peek_byte_fetch(&mut self, addr: u32) -> u8 {
        let addr = addr & addr::ADDR_MASK;
        match Self::decode_address(addr) {
            MemoryRegion::Flash => self.flash.peek_status(addr),
            MemoryRegion::Ram | MemoryRegion::Vram => {
                self.ram.read(addr - addr::RAM_START)
            }
            MemoryRegion::Ports => {
                let keys = *self.ports.key_state();
                // Use 0 for cycles in debug peek (no timing effects)
                self.ports.read(addr - addr::PORT_START, &keys, 0)
            }
            MemoryRegion::Unmapped => 0x00,
        }
    }

    /// Poke a byte without affecting cycles (for debugging)
    pub fn poke_byte(&mut self, addr: u32, value: u8) {
        let addr = addr & addr::ADDR_MASK;

        match Self::decode_address(addr) {
            MemoryRegion::Flash => {
                self.flash.write_direct(addr, value);
            }
            MemoryRegion::Ram | MemoryRegion::Vram => {
                self.ram.write(addr - addr::RAM_START, value);
            }
            MemoryRegion::Ports => {
                // Use 0 for cycles in debug poke (no timing effects)
                self.ports.write(addr - addr::PORT_START, value, 0);
            }
            MemoryRegion::Unmapped => {}
        }
    }

    /// Load ROM into flash
    pub fn load_rom(&mut self, data: &[u8]) -> Result<(), FlashError> {
        self.flash.load_rom(data)
    }

    /// Get current cycle count
    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    /// Reset cycle counter
    pub fn reset_cycles(&mut self) {
        self.cycles = 0;
    }

    /// Add cycles (for CPU internal operations)
    pub fn add_cycles(&mut self, count: u64) {
        self.cycles += count;
    }

    /// Get direct access to VRAM for LCD rendering
    pub fn vram(&self) -> &[u8] {
        self.ram.vram()
    }

    /// Read from I/O port (for IN instructions)
    ///
    /// The eZ80 uses a 16-bit port address space separate from memory.
    /// Port addresses are routed based on bits 15:12:
    ///   0x0xxx -> Control ports
    ///   0x1xxx -> Flash controller
    ///   0x2xxx -> SHA256 (stub)
    ///   0x3xxx -> USB (stub)
    ///   0x4xxx -> LCD controller
    ///   0x5xxx -> Interrupt controller
    ///   0x6xxx -> Watchdog
    ///   0x7xxx -> Timers
    ///   0x8xxx -> RTC (stub)
    ///   0x9xxx -> Protected (stub)
    ///   0xAxxx -> Keypad
    ///   0xBxxx -> Backlight (stub)
    ///   0xCxxx -> Cxxx (stub)
    ///   0xDxxx -> SPI (stub)
    ///   0xExxx -> UART (stub)
    ///   0xFxxx -> Control ports (alternate)
    ///
    /// Based on CEmu's port.c port_map array
    pub fn port_read(&mut self, port: u16) -> u8 {
        let range = (port >> 12) & 0xF;
        self.cycles += Self::PORT_READ_CYCLES[range as usize];
        let keys = *self.ports.key_state();

        match range {
            0x0 => {
                // Control ports - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.control.read(offset)
            }
            0x1 => {
                // Flash controller - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.flash.read(offset)
            }
            0x2 => {
                // SHA256 accelerator - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.sha256.read(offset)
            }
            0x4 => {
                // LCD controller - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.lcd.read(offset)
            }
            0x5 => {
                // Interrupt controller - mask with 0xFF (CEmu port_mirrors)
                let offset = (port & 0xFF) as u32;
                self.ports.interrupt.read(offset)
            }
            0x6 => {
                // Watchdog - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.watchdog.read(offset)
            }
            0x7 => {
                // Timers - mask with 0x7F
                let offset = (port & 0x7F) as u32;
                if offset >= 0x30 {
                    match offset {
                        0x30 => self.ports.timer1.read_control(),
                        0x34 => self.ports.timer2.read_control(),
                        0x38 => self.ports.timer3.read_control(),
                        _ => 0x00,
                    }
                } else {
                    let timer_idx = offset / 0x10;
                    let reg_offset = offset % 0x10;
                    match timer_idx {
                        0 => self.ports.timer1.read(reg_offset),
                        1 => self.ports.timer2.read(reg_offset),
                        2 => self.ports.timer3.read(reg_offset),
                        _ => 0x00,
                    }
                }
            }
            0x8 => {
                // RTC - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.rtc.read(offset, self.cycles, self.ports.control.cpu_speed())
            }
            0xA => {
                // Keypad - mask with 0x7F
                let offset = (port & 0x7F) as u32;
                self.ports.keypad.read(offset, &keys)
            }
            0xD => {
                // SPI - mask with 0x7F (CEmu port_mirrors)
                let offset = (port & 0x7F) as u32;
                self.spi.read(offset, self.cycles, self.ports.control.cpu_speed())
            }
            0xF => {
                // Control ports alternate - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.control.read(offset)
            }
            // Unimplemented: USB(3), Protected(9), Backlight(B), Cxxx(C), UART(E)
            _ => 0x00,
        }
    }

    /// Write to I/O port (for OUT instructions)
    pub fn port_write(&mut self, port: u16, value: u8) {
        let range = (port >> 12) & 0xF;
        self.cycles += Self::PORT_WRITE_CYCLES[range as usize];

        match range {
            0x0 => {
                // Control ports - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.control.write(offset, value);
            }
            0x1 => {
                // Flash controller - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.flash.write(offset, value);
            }
            0x2 => {
                // SHA256 accelerator - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.sha256.write(offset, value);
            }
            0x4 => {
                // LCD controller - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.lcd.write(offset, value);
            }
            0x5 => {
                // Interrupt controller - mask with 0xFF (CEmu port_mirrors)
                let offset = (port & 0xFF) as u32;
                self.ports.interrupt.write(offset, value);
            }
            0x6 => {
                // Watchdog - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.watchdog.write(offset, value);
            }
            0x7 => {
                // Timers - mask with 0x7F
                let offset = (port & 0x7F) as u32;
                if offset >= 0x30 {
                    match offset {
                        0x30 => self.ports.timer1.write_control(value),
                        0x34 => self.ports.timer2.write_control(value),
                        0x38 => self.ports.timer3.write_control(value),
                        _ => {}
                    }
                } else {
                    let timer_idx = offset / 0x10;
                    let reg_offset = offset % 0x10;
                    match timer_idx {
                        0 => self.ports.timer1.write(reg_offset, value),
                        1 => self.ports.timer2.write(reg_offset, value),
                        2 => self.ports.timer3.write(reg_offset, value),
                        _ => {}
                    }
                }
            }
            0x8 => {
                // RTC - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.rtc.write(offset, value, self.cycles, self.ports.control.cpu_speed());
            }
            0xA => {
                // Keypad - mask with 0x7F
                let offset = (port & 0x7F) as u32;
                self.ports.keypad.write(offset, value);
            }
            0xD => {
                // SPI - mask with 0x7F
                let offset = (port & 0x7F) as u32;
                self.spi.write(offset, value, self.cycles, self.ports.control.cpu_speed());
            }
            0xF => {
                // Control ports alternate - mask with 0xFF
                let offset = (port & 0xFF) as u32;
                self.ports.control.write(offset, value);
            }
            // Unimplemented: USB(3), Protected(9), Backlight(B), Cxxx(C), UART(E)
            _ => {}
        }
    }

    /// Reset bus and all memory to initial state
    pub fn reset(&mut self) {
        self.ram.reset();
        self.ports.reset();
        self.spi.reset();
        self.cycles = 0;
        self.rng = BusRng::new();
        self.fetch_buffer = [0; FETCH_BUFFER_SIZE];
        self.fetch_index = 0;
        self.write_tracer.reset();
        // Note: Flash is NOT reset - ROM data is preserved
        // Note: Write tracer enabled state is preserved across reset
    }

    /// Set key state for peripheral reads
    pub fn set_key(&mut self, row: usize, col: usize, pressed: bool) {
        self.ports.set_key(row, col, pressed);
    }

    /// Get key state reference (delegates to peripherals)
    pub fn key_state(&self) -> &[[bool; crate::peripherals::KEYPAD_COLS]; crate::peripherals::KEYPAD_ROWS] {
        self.ports.key_state()
    }

    /// Full reset including flash
    pub fn hard_reset(&mut self) {
        self.flash.reset();
        self.reset();
    }

    /// Seed the RNG (for deterministic testing)
    pub fn seed_rng(&mut self, s1: u8, s2: u8, s3: u8) {
        self.rng.seed(s1, s2, s3);
    }
}

impl Default for Bus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_decoding() {
        // Flash region
        assert_eq!(Bus::decode_address(0x000000), MemoryRegion::Flash);
        assert_eq!(Bus::decode_address(0x100000), MemoryRegion::Flash);
        assert_eq!(Bus::decode_address(0x3FFFFF), MemoryRegion::Flash);

        // Unmapped between flash and RAM
        assert_eq!(Bus::decode_address(0x400000), MemoryRegion::Unmapped);
        assert_eq!(Bus::decode_address(0x800000), MemoryRegion::Unmapped);
        assert_eq!(Bus::decode_address(0xCFFFFF), MemoryRegion::Unmapped);

        // RAM region
        assert_eq!(Bus::decode_address(0xD00000), MemoryRegion::Ram);
        assert_eq!(Bus::decode_address(0xD30000), MemoryRegion::Ram);
        assert_eq!(Bus::decode_address(0xD3FFFF), MemoryRegion::Ram);

        // VRAM region (within RAM)
        assert_eq!(Bus::decode_address(0xD40000), MemoryRegion::Vram);
        assert_eq!(Bus::decode_address(0xD50000), MemoryRegion::Vram);
        assert_eq!(Bus::decode_address(0xD657FF), MemoryRegion::Vram);

        // Unmapped between RAM and ports
        assert_eq!(Bus::decode_address(0xD65800), MemoryRegion::Unmapped);
        assert_eq!(Bus::decode_address(0xDFFFFF), MemoryRegion::Unmapped);

        // Port region
        assert_eq!(Bus::decode_address(0xE00000), MemoryRegion::Ports);
        assert_eq!(Bus::decode_address(0xF50000), MemoryRegion::Ports);
        assert_eq!(Bus::decode_address(0xFFFFFF), MemoryRegion::Ports);
    }

    fn run_flash_unlock_sequence(bus: &mut Bus, sequence: &[u8]) {
        // Unlock protected ports (port 0x06 bit 2)
        bus.ports.control.write(0x06, 0x04);

        for (i, &byte) in sequence.iter().enumerate() {
            bus.flash.write_direct(i as u32, byte);
            let _ = bus.fetch_byte(i as u32, i as u32);
        }
    }

    #[test]
    fn test_flash_unlock_sequence_single_di() {
        let mut bus = Bus::new();
        run_flash_unlock_sequence(&mut bus, &FLASH_UNLOCK_SEQUENCE);
        assert!(bus.ports.control.flash_ready());
    }

    #[test]
    fn test_flash_unlock_sequence_double_di() {
        let mut bus = Bus::new();
        run_flash_unlock_sequence(&mut bus, &FLASH_UNLOCK_SEQUENCE_DOUBLE_DI);
        assert!(bus.ports.control.flash_ready());
    }

    #[test]
    fn test_address_masking() {
        // Addresses above 24 bits should be masked
        assert_eq!(Bus::decode_address(0x1000000), MemoryRegion::Flash);
        assert_eq!(Bus::decode_address(0x1D00000), MemoryRegion::Ram);
    }

    #[test]
    fn test_ram_read_write() {
        let mut bus = Bus::new();

        bus.write_byte(0xD00100, 0xAB);
        assert_eq!(bus.read_byte(0xD00100), 0xAB);
    }

    #[test]
    fn test_ram_word_access() {
        let mut bus = Bus::new();

        bus.write_word(0xD00200, 0xBEEF);
        assert_eq!(bus.read_word(0xD00200), 0xBEEF);
    }

    #[test]
    fn test_ram_addr24_access() {
        let mut bus = Bus::new();

        bus.write_addr24(0xD00300, 0xD12345);
        assert_eq!(bus.read_addr24(0xD00300), 0xD12345);
    }

    #[test]
    fn test_flash_read() {
        let mut bus = Bus::new();
        let rom = vec![0x12, 0x34, 0x56, 0x78];
        bus.load_rom(&rom).unwrap();

        assert_eq!(bus.read_byte(0x000000), 0x12);
        assert_eq!(bus.read_byte(0x000001), 0x34);
        assert_eq!(bus.read_byte(0x000002), 0x56);
        assert_eq!(bus.read_byte(0x000003), 0x78);
    }

    #[test]
    fn test_flash_write_ignored() {
        let mut bus = Bus::new();
        let rom = vec![0x12, 0x34];
        bus.load_rom(&rom).unwrap();

        // CPU writes to flash should be ignored
        bus.write_byte(0x000000, 0xFF);
        assert_eq!(bus.read_byte(0x000000), 0x12);
    }

    #[test]
    fn test_port_read_write() {
        let mut bus = Bus::new();

        bus.write_byte(0xE00100, 0x42);
        assert_eq!(bus.read_byte(0xE00100), 0x42);
    }

    #[test]
    fn test_unmapped_returns_pseudorandom() {
        let mut bus = Bus::new();
        bus.seed_rng(0x12, 0x34, 0x56);

        // With deterministic seed, verify we get expected sequence
        let val1 = bus.read_byte(0x500000);
        let val2 = bus.read_byte(0x500000); // Same address, different value (RNG advances)

        // RNG should produce different values on consecutive reads
        assert_ne!(val1, val2, "RNG should produce varying values");

        // Verify the RNG produces the expected first value for this seed
        // seed [0x12, 0x34, 0x56], first output should be 0x12 (returns state[0])
        let mut bus2 = Bus::new();
        bus2.seed_rng(0x12, 0x34, 0x56);
        assert_eq!(bus2.read_byte(0x500000), 0x12);
    }

    #[test]
    fn test_cycle_counting() {
        let mut bus = Bus::new();

        // Verify exact cycle counts match documented wait states
        assert_eq!(bus.cycles(), 0);

        // RAM read: 4 cycles (3 wait states + 1)
        bus.read_byte(0xD00000);
        assert_eq!(bus.cycles(), Bus::RAM_READ_CYCLES);

        bus.reset_cycles();

        // RAM write: 2 cycles (1 wait state + 1)
        bus.write_byte(0xD00000, 0x00);
        assert_eq!(bus.cycles(), Bus::RAM_WRITE_CYCLES);

        bus.reset_cycles();

        // Flash read: 10 cycles (high wait states)
        bus.read_byte(0x000000);
        assert_eq!(bus.cycles(), Bus::FLASH_READ_CYCLES);

        bus.reset_cycles();

        // Memory-mapped I/O read: 3 cycles (MMIO default)
        bus.read_byte(0xE00000);
        assert_eq!(bus.cycles(), Bus::MMIO_READ_CYCLES);

        bus.reset_cycles();

        // Memory-mapped I/O write: 3 cycles (MMIO default)
        bus.write_byte(0xE00000, 0x00);
        assert_eq!(bus.cycles(), Bus::MMIO_WRITE_CYCLES);

        bus.reset_cycles();

        // Unmapped read: 2 cycles
        bus.read_byte(0x500000);
        assert_eq!(bus.cycles(), Bus::UNMAPPED_CYCLES);
    }

    #[test]
    fn test_peek_poke_no_cycles() {
        let mut bus = Bus::new();

        let initial = bus.cycles();
        bus.poke_byte(0xD00000, 0x42);
        bus.peek_byte(0xD00000);
        assert_eq!(bus.cycles(), initial);

        // Verify data was written
        assert_eq!(bus.peek_byte(0xD00000), 0x42);
    }

    #[test]
    fn test_reset() {
        let mut bus = Bus::new();
        let rom = vec![0x12, 0x34];
        bus.load_rom(&rom).unwrap();
        bus.write_byte(0xD00000, 0xFF);
        bus.add_cycles(1000);

        bus.reset();

        // RAM should be cleared
        assert_eq!(bus.peek_byte(0xD00000), 0x00);
        // Cycles should be reset
        assert_eq!(bus.cycles(), 0);
        // Flash should be preserved
        assert_eq!(bus.peek_byte(0x000000), 0x12);
    }

    #[test]
    fn test_hard_reset() {
        let mut bus = Bus::new();
        let rom = vec![0x12, 0x34];
        bus.load_rom(&rom).unwrap();

        bus.hard_reset();

        // Flash should be erased too
        assert_eq!(bus.peek_byte(0x000000), 0xFF);
    }

    #[test]
    fn test_vram_access() {
        let mut bus = Bus::new();

        // Write to VRAM address
        bus.write_byte(0xD40000, 0x42);

        // Access via vram() method should see the data
        assert_eq!(bus.vram()[0], 0x42);
    }

    #[test]
    fn test_dword_access() {
        let mut bus = Bus::new();

        bus.write_dword(0xD00400, 0xDEADBEEF);
        assert_eq!(bus.read_dword(0xD00400), 0xDEADBEEF);

        // Verify little-endian storage
        assert_eq!(bus.peek_byte(0xD00400), 0xEF);
        assert_eq!(bus.peek_byte(0xD00401), 0xBE);
        assert_eq!(bus.peek_byte(0xD00402), 0xAD);
        assert_eq!(bus.peek_byte(0xD00403), 0xDE);
    }

    #[test]
    fn test_boundary_addresses() {
        // Test exact boundary addresses to ensure off-by-one errors are caught

        // Last flash byte vs first unmapped
        assert_eq!(Bus::decode_address(0x3FFFFF), MemoryRegion::Flash);
        assert_eq!(Bus::decode_address(0x400000), MemoryRegion::Unmapped);

        // Last unmapped byte vs first RAM byte
        assert_eq!(Bus::decode_address(0xCFFFFF), MemoryRegion::Unmapped);
        assert_eq!(Bus::decode_address(0xD00000), MemoryRegion::Ram);

        // Last RAM byte vs first VRAM byte
        assert_eq!(Bus::decode_address(0xD3FFFF), MemoryRegion::Ram);
        assert_eq!(Bus::decode_address(0xD40000), MemoryRegion::Vram);

        // Last VRAM byte vs first unmapped byte (between RAM and ports)
        assert_eq!(Bus::decode_address(0xD657FF), MemoryRegion::Vram);
        assert_eq!(Bus::decode_address(0xD65800), MemoryRegion::Unmapped);

        // Last unmapped byte vs first port byte
        assert_eq!(Bus::decode_address(0xDFFFFF), MemoryRegion::Unmapped);
        assert_eq!(Bus::decode_address(0xE00000), MemoryRegion::Ports);

        // Last port byte (wraps in 24-bit space)
        assert_eq!(Bus::decode_address(0xFFFFFF), MemoryRegion::Ports);
    }

    #[test]
    fn test_ram_vram_contiguous() {
        // RAM and VRAM should be contiguous in the underlying storage
        let mut bus = Bus::new();

        // Write to last byte of RAM region
        bus.write_byte(0xD3FFFF, 0xAA);
        // Write to first byte of VRAM region
        bus.write_byte(0xD40000, 0xBB);

        // Both should be readable
        assert_eq!(bus.read_byte(0xD3FFFF), 0xAA);
        assert_eq!(bus.read_byte(0xD40000), 0xBB);

        // VRAM accessor should see the VRAM write
        assert_eq!(bus.vram()[0], 0xBB);
    }

    #[test]
    fn test_flash_write_via_bus_ignored_but_poke_works() {
        let mut bus = Bus::new();

        // Normal write should be ignored
        bus.write_byte(0x000000, 0x12);
        assert_eq!(bus.peek_byte(0x000000), 0xFF); // Still erased

        // But poke (debug write) should work
        bus.poke_byte(0x000000, 0x34);
        assert_eq!(bus.peek_byte(0x000000), 0x34);
    }

    #[test]
    fn test_multi_byte_across_boundary() {
        let mut bus = Bus::new();

        // Write a word that spans RAM/VRAM boundary
        // 0xD3FFFF is last RAM byte, 0xD40000 is first VRAM byte
        bus.write_word(0xD3FFFF, 0xABCD);

        // Low byte at 0xD3FFFF (RAM), high byte at 0xD40000 (VRAM)
        assert_eq!(bus.peek_byte(0xD3FFFF), 0xCD);
        assert_eq!(bus.peek_byte(0xD40000), 0xAB);

        // Read should work too
        assert_eq!(bus.read_word(0xD3FFFF), 0xABCD);
    }
}
