//! Memory subsystem for TI-84 Plus CE
//!
//! This module implements the memory map based on the eZ80's 24-bit address space:
//! - 0x000000 - 0x3FFFFF: Flash (4MB, read-only from user code)
//! - 0x400000 - 0xCFFFFF: Unmapped (returns pseudo-random values)
//! - 0xD00000 - 0xD657FF: RAM (256KB + VRAM, 0x65800 bytes total)
//! - 0xD65800 - 0xDFFFFF: Unmapped RAM region
//! - 0xE00000 - 0xFFFFFF: Memory-mapped I/O ports
//!
//! Reference: CEmu (https://github.com/CE-Programming/CEmu)
//! Reference: WikiTI (https://wikiti.brandonw.net)

/// Memory region address constants
pub mod addr {
    /// Flash memory start address
    pub const FLASH_START: u32 = 0x000000;
    /// Flash memory end address (exclusive)
    pub const FLASH_END: u32 = 0x400000;
    /// Flash memory size (4MB)
    pub const FLASH_SIZE: usize = 0x400000;

    /// Unmapped region 1 start (between Flash and RAM)
    pub const UNMAPPED1_START: u32 = 0x400000;
    /// Unmapped region 1 end (exclusive)
    pub const UNMAPPED1_END: u32 = 0xD00000;

    /// RAM start address
    pub const RAM_START: u32 = 0xD00000;
    /// RAM end address (exclusive, includes VRAM)
    pub const RAM_END: u32 = 0xD65800;
    /// Total RAM size (256KB user RAM + ~150KB VRAM)
    pub const RAM_SIZE: usize = 0x65800;

    /// VRAM start address (within RAM region)
    pub const VRAM_START: u32 = 0xD40000;
    /// VRAM size (~150KB for 320x240 16-bit display + extra)
    pub const VRAM_SIZE: usize = 0x25800;

    /// Unmapped region 2 start (between RAM and ports)
    pub const UNMAPPED2_START: u32 = 0xD65800;
    /// Unmapped region 2 end (exclusive)
    pub const UNMAPPED2_END: u32 = 0xE00000;

    /// Memory-mapped I/O start address
    pub const PORT_START: u32 = 0xE00000;
    /// Memory-mapped I/O end address (exclusive)
    pub const PORT_END: u32 = 0x1000000;

    /// Maximum address in 24-bit space
    pub const ADDR_MASK: u32 = 0xFFFFFF;
}

/// Flash memory state
///
/// The TI-84 Plus CE has 4MB of NOR flash for OS and user programs.
/// Flash is read-only from user code; writes require special unlock sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlashCommand {
    None,
    SectorErase { reads_left: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlashWriteState {
    Idle,
    SawAA1,
    Saw55_1,
    Saw80,
    SawAA2,
    Saw55_2,
    SawA0,
}

pub struct Flash {
    /// Flash memory contents
    data: Vec<u8>,
    /// Whether flash has been initialized with ROM data
    initialized: bool,
    /// Active flash command (minimal command emulation)
    command: FlashCommand,
    /// Write sequence state for flash command detection
    write_state: FlashWriteState,
}

impl Flash {
    /// Create a new flash memory instance with lazy allocation
    pub fn new() -> Self {
        // Start with empty vec - will be allocated when ROM is loaded
        Self {
            data: Vec::new(),
            initialized: false,
            command: FlashCommand::None,
            write_state: FlashWriteState::Idle,
        }
    }

    /// Load ROM data into flash
    ///
    /// # Arguments
    /// * `data` - ROM data to load
    ///
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err(FlashError)` if data is too large
    pub fn load_rom(&mut self, data: &[u8]) -> Result<(), FlashError> {
        if data.len() > addr::FLASH_SIZE {
            return Err(FlashError::RomTooLarge);
        }

        // Allocate and copy in one step - start with ROM data
        let mut new_data = data.to_vec();
        // Extend with 0xFF to reach full flash size
        new_data.resize(addr::FLASH_SIZE, 0xFF);
        self.data = new_data;

        self.initialized = true;
        self.command = FlashCommand::None;
        self.write_state = FlashWriteState::Idle;
        Ok(())
    }

    /// Read a byte from flash
    ///
    /// # Arguments
    /// * `addr` - Address relative to flash start (0 to FLASH_SIZE-1)
    pub fn read(&mut self, addr: u32) -> u8 {
        let value = self.peek_status(addr);
        if let FlashCommand::SectorErase { reads_left } = self.command {
            let next_reads = reads_left.saturating_sub(1);
            if next_reads == 0 {
                self.command = FlashCommand::None;
            } else {
                self.command = FlashCommand::SectorErase {
                    reads_left: next_reads,
                };
            }
        }
        value
    }

    /// Peek flash content ignoring command status (debug-style read)
    pub fn peek(&self, addr: u32) -> u8 {
        if self.data.is_empty() {
            return 0xFF; // Uninitialized flash reads as 0xFF
        }
        let offset = (addr & (addr::FLASH_SIZE as u32 - 1)) as usize;
        self.data[offset]
    }

    /// Peek flash content with current command status (no state changes)
    pub fn peek_status(&self, addr: u32) -> u8 {
        match self.command {
            FlashCommand::SectorErase { .. } => 0x80,
            FlashCommand::None => self.peek(addr),
        }
    }

    /// Write a byte to flash (for emulator use, not accessible from CPU normally)
    ///
    /// In real hardware, flash writes require unlock sequences.
    /// This method bypasses that for testing/debugging.
    ///
    /// # Arguments
    /// * `addr` - Address relative to flash start
    /// * `value` - Byte to write
    pub fn write_direct(&mut self, addr: u32, value: u8) {
        // Allocate flash if needed (for testing convenience)
        if self.data.is_empty() {
            self.data = vec![0xFF; addr::FLASH_SIZE];
        }
        let offset = (addr & (addr::FLASH_SIZE as u32 - 1)) as usize;
        self.data[offset] = value;
    }

    /// Handle a CPU write to flash (command detection + optional program/erase)
    pub fn write_cpu(&mut self, addr: u32, value: u8) {
        // Reset command mode on 0xF0 (common flash reset command)
        if value == 0xF0 {
            self.command = FlashCommand::None;
            self.write_state = FlashWriteState::Idle;
            return;
        }

        let addr_masked = addr & 0xFFF;
        self.write_state = match self.write_state {
            FlashWriteState::Idle => {
                if addr_masked == 0xAAA && value == 0xAA {
                    FlashWriteState::SawAA1
                } else {
                    FlashWriteState::Idle
                }
            }
            FlashWriteState::SawAA1 => {
                if addr_masked == 0x555 && value == 0x55 {
                    FlashWriteState::Saw55_1
                } else {
                    FlashWriteState::Idle
                }
            }
            FlashWriteState::Saw55_1 => {
                if addr_masked == 0xAAA && value == 0x80 {
                    FlashWriteState::Saw80
                } else if addr_masked == 0xAAA && value == 0xA0 {
                    FlashWriteState::SawA0
                } else {
                    FlashWriteState::Idle
                }
            }
            FlashWriteState::Saw80 => {
                if addr_masked == 0xAAA && value == 0xAA {
                    FlashWriteState::SawAA2
                } else {
                    FlashWriteState::Idle
                }
            }
            FlashWriteState::SawAA2 => {
                if addr_masked == 0x555 && value == 0x55 {
                    FlashWriteState::Saw55_2
                } else {
                    FlashWriteState::Idle
                }
            }
            FlashWriteState::Saw55_2 => {
                if value == 0x30 {
                    self.erase_sector(addr);
                    self.command = FlashCommand::SectorErase { reads_left: 3 };
                }
                FlashWriteState::Idle
            }
            FlashWriteState::SawA0 => {
                self.program_byte(addr, value);
                FlashWriteState::Idle
            }
        };
    }

    fn erase_sector(&mut self, addr: u32) {
        if self.data.is_empty() {
            return;
        }
        let (start, size) = if addr < 0x10000 {
            let sector_start = (addr / 0x2000) * 0x2000; // 8KB sectors
            (sector_start, 0x2000)
        } else {
            let sector_start = (addr / 0x10000) * 0x10000; // 64KB sectors
            (sector_start, 0x10000)
        };
        let end = (start + size).min(addr::FLASH_SIZE as u32);
        for offset in start..end {
            self.data[offset as usize] = 0xFF;
        }
    }

    fn program_byte(&mut self, addr: u32, value: u8) {
        if self.data.is_empty() {
            return;
        }
        let offset = (addr & (addr::FLASH_SIZE as u32 - 1)) as usize;
        self.data[offset] &= value;
    }

    /// Check if flash is initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get raw flash data for save states
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Load flash data from save state
    pub fn load_data(&mut self, data: &[u8]) {
        let len = data.len().min(addr::FLASH_SIZE);
        self.data[..len].copy_from_slice(&data[..len]);
        self.initialized = true;
        self.command = FlashCommand::None;
        self.write_state = FlashWriteState::Idle;
    }

    /// Reset flash to erased state
    pub fn reset(&mut self) {
        if !self.data.is_empty() {
            self.data.fill(0xFF);
        }
        self.initialized = false;
        self.command = FlashCommand::None;
        self.write_state = FlashWriteState::Idle;
    }
}

impl Default for Flash {
    fn default() -> Self {
        Self::new()
    }
}

/// Flash memory errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashError {
    /// ROM data exceeds flash capacity
    RomTooLarge,
    /// Flash write protection violation
    WriteProtected,
}

/// RAM memory state
///
/// The TI-84 Plus CE has 256KB of user RAM plus ~150KB of VRAM,
/// all in a single contiguous region starting at 0xD00000.
pub struct Ram {
    /// RAM contents
    data: Vec<u8>,
}

impl Ram {
    /// Create a new RAM instance (lazy allocation)
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
        }
    }

    /// Read a byte from RAM
    ///
    /// # Arguments
    /// * `addr` - Address relative to RAM start (0 to RAM_SIZE-1)
    pub fn read(&self, addr: u32) -> u8 {
        if self.data.is_empty() {
            return 0x00;
        }
        let offset = (addr as usize) % addr::RAM_SIZE;
        self.data[offset]
    }

    /// Write a byte to RAM
    ///
    /// # Arguments
    /// * `addr` - Address relative to RAM start
    /// * `value` - Byte to write
    pub fn write(&mut self, addr: u32, value: u8) {
        if self.data.is_empty() {
            self.data = vec![0x00; addr::RAM_SIZE];
        }
        let offset = (addr as usize) % addr::RAM_SIZE;
        self.data[offset] = value;
    }

    /// Read a 16-bit word from RAM (little-endian)
    pub fn read_word(&self, addr: u32) -> u16 {
        let lo = self.read(addr) as u16;
        let hi = self.read(addr.wrapping_add(1)) as u16;
        lo | (hi << 8)
    }

    /// Write a 16-bit word to RAM (little-endian)
    pub fn write_word(&mut self, addr: u32, value: u16) {
        self.write(addr, value as u8);
        self.write(addr.wrapping_add(1), (value >> 8) as u8);
    }

    /// Read a 24-bit address from RAM (little-endian)
    pub fn read_addr24(&self, addr: u32) -> u32 {
        let b0 = self.read(addr) as u32;
        let b1 = self.read(addr.wrapping_add(1)) as u32;
        let b2 = self.read(addr.wrapping_add(2)) as u32;
        b0 | (b1 << 8) | (b2 << 16)
    }

    /// Write a 24-bit address to RAM (little-endian)
    pub fn write_addr24(&mut self, addr: u32, value: u32) {
        self.write(addr, value as u8);
        self.write(addr.wrapping_add(1), (value >> 8) as u8);
        self.write(addr.wrapping_add(2), (value >> 16) as u8);
    }

    /// Get VRAM slice for LCD rendering
    ///
    /// Returns a slice of the VRAM region (0xD40000-0xD657FF relative to RAM start)
    pub fn vram(&self) -> &[u8] {
        if self.data.is_empty() {
            return &[];
        }
        let start = (addr::VRAM_START - addr::RAM_START) as usize;
        let end = start + addr::VRAM_SIZE;
        &self.data[start..end]
    }

    /// Get mutable VRAM slice
    pub fn vram_mut(&mut self) -> &mut [u8] {
        if self.data.is_empty() {
            return &mut [];
        }
        let start = (addr::VRAM_START - addr::RAM_START) as usize;
        let end = start + addr::VRAM_SIZE;
        &mut self.data[start..end]
    }

    /// Get raw RAM data for save states
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Load RAM data from save state
    pub fn load_data(&mut self, data: &[u8]) {
        let len = data.len().min(addr::RAM_SIZE);
        self.data[..len].copy_from_slice(&data[..len]);
    }

    /// Clear RAM to zero
    pub fn reset(&mut self) {
        self.data.fill(0x00);
    }
}

impl Default for Ram {
    fn default() -> Self {
        Self::new()
    }
}

// Re-export Peripherals as Ports for backward compatibility
pub use crate::peripherals::Peripherals as Ports;

#[cfg(test)]
mod tests {
    use super::*;

    mod flash_tests {
        use super::*;

        #[test]
        fn test_new_flash_is_erased() {
            let mut flash = Flash::new();
            assert!(!flash.is_initialized());
            // Flash erased state is 0xFF
            assert_eq!(flash.read(0), 0xFF);
            assert_eq!(flash.read(0x1000), 0xFF);
            assert_eq!(flash.read(0x3FFFFF), 0xFF);
        }

        #[test]
        fn test_load_rom() {
            let mut flash = Flash::new();
            let rom = vec![0x12, 0x34, 0x56, 0x78];
            assert!(flash.load_rom(&rom).is_ok());
            assert!(flash.is_initialized());

            assert_eq!(flash.read(0), 0x12);
            assert_eq!(flash.read(1), 0x34);
            assert_eq!(flash.read(2), 0x56);
            assert_eq!(flash.read(3), 0x78);
            // Rest should still be erased
            assert_eq!(flash.read(4), 0xFF);
        }

        #[test]
        fn test_rom_too_large() {
            let mut flash = Flash::new();
            let rom = vec![0u8; addr::FLASH_SIZE + 1];
            assert_eq!(flash.load_rom(&rom), Err(FlashError::RomTooLarge));
        }

        #[test]
        fn test_write_direct() {
            let mut flash = Flash::new();
            flash.write_direct(0x100, 0xAB);
            assert_eq!(flash.read(0x100), 0xAB);
        }

        #[test]
        fn test_reset() {
            let mut flash = Flash::new();
            let rom = vec![0x12, 0x34];
            flash.load_rom(&rom).unwrap();
            flash.reset();
            assert!(!flash.is_initialized());
            assert_eq!(flash.read(0), 0xFF);
        }
    }

    mod ram_tests {
        use super::*;

        #[test]
        fn test_new_ram_is_zeroed() {
            let ram = Ram::new();
            assert_eq!(ram.read(0), 0x00);
            assert_eq!(ram.read(0x1000), 0x00);
        }

        #[test]
        fn test_read_write_byte() {
            let mut ram = Ram::new();
            ram.write(0x100, 0xAB);
            assert_eq!(ram.read(0x100), 0xAB);
        }

        #[test]
        fn test_read_write_word() {
            let mut ram = Ram::new();
            ram.write_word(0x200, 0xBEEF);
            assert_eq!(ram.read_word(0x200), 0xBEEF);
            // Check little-endian storage
            assert_eq!(ram.read(0x200), 0xEF);
            assert_eq!(ram.read(0x201), 0xBE);
        }

        #[test]
        fn test_read_write_addr24() {
            let mut ram = Ram::new();
            ram.write_addr24(0x300, 0xD12345);
            assert_eq!(ram.read_addr24(0x300), 0xD12345);
            // Check little-endian storage
            assert_eq!(ram.read(0x300), 0x45);
            assert_eq!(ram.read(0x301), 0x23);
            assert_eq!(ram.read(0x302), 0xD1);
        }

        #[test]
        fn test_vram_access() {
            let mut ram = Ram::new();
            let vram_offset = (addr::VRAM_START - addr::RAM_START) as usize;

            // Write via normal RAM access
            ram.write(vram_offset as u32, 0x42);

            // Read via VRAM accessor
            assert_eq!(ram.vram()[0], 0x42);
        }

        #[test]
        fn test_vram_size() {
            let mut ram = Ram::new();
            // Force RAM allocation by writing to it
            ram.write(0, 0);
            assert_eq!(ram.vram().len(), addr::VRAM_SIZE);
        }

        #[test]
        fn test_reset() {
            let mut ram = Ram::new();
            ram.write(0x100, 0xFF);
            ram.reset();
            assert_eq!(ram.read(0x100), 0x00);
        }

        #[test]
        fn test_address_wrapping() {
            let mut ram = Ram::new();
            // Address beyond RAM_SIZE should wrap
            let wrapped_addr = addr::RAM_SIZE as u32 + 0x100;
            ram.write(wrapped_addr, 0x99);
            assert_eq!(ram.read(0x100), 0x99);
        }
    }

    mod port_tests {
        use super::*;
        use crate::peripherals::{KEYPAD_COLS, KEYPAD_ROWS};

        fn empty_keys() -> [[bool; KEYPAD_COLS]; KEYPAD_ROWS] {
            [[false; KEYPAD_COLS]; KEYPAD_ROWS]
        }

        #[test]
        fn test_read_write() {
            let mut ports = Ports::new();
            let keys = empty_keys();
            ports.write(0x1000, 0xAB, 0);
            assert_eq!(ports.read(0x1000, &keys, 0), 0xAB);
        }

        #[test]
        fn test_reset() {
            let mut ports = Ports::new();
            let keys = empty_keys();
            ports.write(0x100, 0xFF, 0);
            ports.reset();
            assert_eq!(ports.read(0x100, &keys, 0), 0x00);
        }
    }
}
