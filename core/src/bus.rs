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
use crate::peripherals::{KEYPAD_COLS, KEYPAD_ROWS};

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

/// System bus connecting CPU to memory subsystems
pub struct Bus {
    /// Flash memory
    pub flash: Flash,
    /// RAM (including VRAM)
    pub ram: Ram,
    /// Memory-mapped I/O peripherals
    pub ports: Ports,
    /// RNG for unmapped region reads
    rng: BusRng,
    /// Cycle counter for timing
    cycles: u64,
    /// Keypad state for peripheral reads
    key_state: [[bool; KEYPAD_COLS]; KEYPAD_ROWS],
}

impl Bus {
    /// Wait states for different memory regions
    /// These affect CPU timing for accurate emulation
    pub const FLASH_READ_CYCLES: u64 = 10;  // ~4 wait states + fetch
    pub const RAM_READ_CYCLES: u64 = 4;     // 3 wait states + 1
    pub const RAM_WRITE_CYCLES: u64 = 2;    // 1 wait state + 1
    pub const PORT_READ_CYCLES: u64 = 4;
    pub const PORT_WRITE_CYCLES: u64 = 3;
    pub const UNMAPPED_CYCLES: u64 = 2;

    /// Create a new bus with fresh memory
    pub fn new() -> Self {
        Self {
            flash: Flash::new(),
            ram: Ram::new(),
            ports: Ports::new(),
            rng: BusRng::new(),
            cycles: 0,
            key_state: [[false; KEYPAD_COLS]; KEYPAD_ROWS],
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
                self.cycles += Self::PORT_READ_CYCLES;
                self.ports.read(addr - addr::PORT_START, &self.key_state)
            }
            MemoryRegion::Unmapped => {
                self.cycles += Self::UNMAPPED_CYCLES;
                self.rng.next()
            }
        }
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
                // Flash writes are ignored from CPU
                // Real implementation would check for unlock sequences
                self.cycles += Self::UNMAPPED_CYCLES;
            }
            MemoryRegion::Ram | MemoryRegion::Vram => {
                self.cycles += Self::RAM_WRITE_CYCLES;
                self.ram.write(addr - addr::RAM_START, value);
            }
            MemoryRegion::Ports => {
                self.cycles += Self::PORT_WRITE_CYCLES;
                self.ports.write(addr - addr::PORT_START, value);
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
    pub fn peek_byte(&self, addr: u32) -> u8 {
        let addr = addr & addr::ADDR_MASK;

        match Self::decode_address(addr) {
            MemoryRegion::Flash => self.flash.read(addr),
            MemoryRegion::Ram | MemoryRegion::Vram => {
                self.ram.read(addr - addr::RAM_START)
            }
            MemoryRegion::Ports => self.ports.read(addr - addr::PORT_START, &self.key_state),
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
                self.ports.write(addr - addr::PORT_START, value);
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

    /// Reset bus and all memory to initial state
    pub fn reset(&mut self) {
        self.ram.reset();
        self.ports.reset();
        self.cycles = 0;
        self.rng = BusRng::new();
        self.key_state = [[false; KEYPAD_COLS]; KEYPAD_ROWS];
        // Note: Flash is NOT reset - ROM data is preserved
    }

    /// Set key state for peripheral reads
    pub fn set_key(&mut self, row: usize, col: usize, pressed: bool) {
        if row < KEYPAD_ROWS && col < KEYPAD_COLS {
            self.key_state[row][col] = pressed;
            // Also update peripherals' internal key_state
            self.ports.set_key(row, col, pressed);
        }
    }

    /// Get key state reference
    pub fn key_state(&self) -> &[[bool; KEYPAD_COLS]; KEYPAD_ROWS] {
        &self.key_state
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

        // Port read: 4 cycles
        bus.read_byte(0xE00000);
        assert_eq!(bus.cycles(), Bus::PORT_READ_CYCLES);

        bus.reset_cycles();

        // Port write: 3 cycles
        bus.write_byte(0xE00000, 0x00);
        assert_eq!(bus.cycles(), Bus::PORT_WRITE_CYCLES);

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
