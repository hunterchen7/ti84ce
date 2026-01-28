//! Emulator orchestrator
//!
//! Coordinates the CPU, bus, and peripherals to run the TI-84 Plus CE.

use crate::bus::Bus;
use crate::cpu::Cpu;

/// TI-84 Plus CE screen dimensions
pub const SCREEN_WIDTH: usize = 320;
pub const SCREEN_HEIGHT: usize = 240;


/// Number of entries in the PC/opcode history ring buffer
const HISTORY_SIZE: usize = 64;

/// Single entry in the execution history
#[derive(Clone, Copy, Default)]
struct HistoryEntry {
    /// Program counter before instruction
    pc: u32,
    /// Opcode byte(s) - up to 4 bytes for prefixed instructions
    opcode: [u8; 4],
    /// Number of valid opcode bytes
    opcode_len: u8,
}

/// Execution history ring buffer for crash diagnostics
struct ExecutionHistory {
    /// Ring buffer of history entries
    entries: [HistoryEntry; HISTORY_SIZE],
    /// Write index (next position to write)
    write_idx: usize,
    /// Number of entries written (max HISTORY_SIZE)
    count: usize,
}

impl ExecutionHistory {
    fn new() -> Self {
        Self {
            entries: [HistoryEntry::default(); HISTORY_SIZE],
            write_idx: 0,
            count: 0,
        }
    }

    /// Record an instruction execution
    fn record(&mut self, pc: u32, opcode: &[u8]) {
        let mut entry = HistoryEntry {
            pc,
            opcode: [0; 4],
            opcode_len: opcode.len().min(4) as u8,
        };
        for (i, &byte) in opcode.iter().take(4).enumerate() {
            entry.opcode[i] = byte;
        }
        self.entries[self.write_idx] = entry;
        self.write_idx = (self.write_idx + 1) % HISTORY_SIZE;
        if self.count < HISTORY_SIZE {
            self.count += 1;
        }
    }

    /// Get history entries in execution order (oldest to newest)
    fn iter(&self) -> impl Iterator<Item = &HistoryEntry> {
        let start = if self.count < HISTORY_SIZE {
            0
        } else {
            self.write_idx
        };
        (0..self.count).map(move |i| {
            let idx = (start + i) % HISTORY_SIZE;
            &self.entries[idx]
        })
    }

    fn clear(&mut self) {
        self.write_idx = 0;
        self.count = 0;
    }
}

/// Reason for stopping execution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// Completed requested cycles
    CyclesComplete,
    /// CPU halted (HALT instruction)
    Halted,
    // TODO: Wire up UnimplementedOpcode when CPU reports unimplemented instructions (Milestone 5+)
    /// Unimplemented opcode encountered
    UnimplementedOpcode(u8),
    // TODO: Wire up BusFault when Bus reports invalid memory access (Milestone 5+)
    /// Bus fault (invalid memory access)
    BusFault(u32),
}

/// Main emulator state
pub struct Emu {
    /// eZ80 CPU
    cpu: Cpu,
    /// System bus (memory, I/O)
    bus: Bus,

    /// Framebuffer in ARGB8888 format
    framebuffer: Vec<u32>,

    /// ROM loaded flag
    rom_loaded: bool,

    /// Execution history for crash diagnostics
    history: ExecutionHistory,

    /// Last stop reason
    last_stop: StopReason,

    /// Total cycles executed
    total_cycles: u64,
}

impl Emu {
    /// Create a new emulator instance
    pub fn new() -> Self {
        Self {
            cpu: Cpu::new(),
            bus: Bus::new(),
            framebuffer: vec![0xFF000000; SCREEN_WIDTH * SCREEN_HEIGHT],
            rom_loaded: false,
            history: ExecutionHistory::new(),
            last_stop: StopReason::CyclesComplete,
            total_cycles: 0,
        }
    }

    /// Load ROM data into flash
    pub fn load_rom(&mut self, data: &[u8]) -> Result<(), i32> {
        if data.is_empty() {
            return Err(-2); // Empty ROM
        }

        self.bus.load_rom(data).map_err(|_| -3)?; // -3 = ROM too large
        self.rom_loaded = true;
        self.reset();
        Ok(())
    }

    /// Reset emulator to initial state
    pub fn reset(&mut self) {
        self.cpu.reset();
        self.bus.reset();
        self.history.clear();
        self.last_stop = StopReason::CyclesComplete;
        self.total_cycles = 0;

        // Clear framebuffer to black
        for pixel in &mut self.framebuffer {
            *pixel = 0xFF000000;
        }
    }

    /// Run for specified cycles, returns cycles actually executed
    pub fn run_cycles(&mut self, cycles: u32) -> u32 {
        if !self.rom_loaded {
            return 0;
        }

        let mut cycles_remaining = cycles as i32;
        let start_cycles = self.total_cycles;

        while cycles_remaining > 0 {
            // Record PC and peek at opcode before execution
            let pc = self.cpu.pc;
            let (opcode, opcode_len) = self.peek_opcode(pc);

            // Execute one instruction
            let cycles_used = self.cpu.step(&mut self.bus);

            // Record in history
            self.history.record(pc, &opcode[..opcode_len]);

            // Tick peripherals and check for interrupts
            if self.bus.ports.tick(cycles_used) {
                self.cpu.irq_pending = true;
            }

            cycles_remaining -= cycles_used as i32;
            self.total_cycles += cycles_used as u64;

            // Check for halt
            if self.cpu.halted {
                self.last_stop = StopReason::Halted;
                return (self.total_cycles - start_cycles) as u32;
            }
        }

        self.last_stop = StopReason::CyclesComplete;
        (self.total_cycles - start_cycles) as u32
    }

    /// Peek at opcode bytes at address without affecting state
    /// Returns (bytes, length) to avoid heap allocation in hot loop
    fn peek_opcode(&self, addr: u32) -> ([u8; 4], usize) {
        let mut bytes = [0u8; 4];
        let first = self.bus.peek_byte(addr);
        bytes[0] = first;

        // Check for prefix bytes
        let len = match first {
            0xCB | 0xED => {
                bytes[1] = self.bus.peek_byte(addr.wrapping_add(1));
                2
            }
            0xDD | 0xFD => {
                let second = self.bus.peek_byte(addr.wrapping_add(1));
                bytes[1] = second;
                if second == 0xCB {
                    bytes[2] = self.bus.peek_byte(addr.wrapping_add(2));
                    bytes[3] = self.bus.peek_byte(addr.wrapping_add(3));
                    4
                } else {
                    2
                }
            }
            _ => 1,
        };

        (bytes, len)
    }

    /// Get framebuffer dimensions
    pub fn framebuffer_size(&self) -> (usize, usize) {
        (SCREEN_WIDTH, SCREEN_HEIGHT)
    }

    /// Get raw pointer to framebuffer
    pub fn framebuffer_ptr(&self) -> *const u32 {
        self.framebuffer.as_ptr()
    }

    /// Set key state
    pub fn set_key(&mut self, row: usize, col: usize, down: bool) {
        self.bus.set_key(row, col, down);
    }

    /// Render the current VRAM contents to the framebuffer
    /// Converts RGB565 to ARGB8888
    pub fn render_frame(&mut self) {
        let upbase = self.bus.ports.lcd.upbase();

        // Read VRAM and convert to ARGB8888
        for y in 0..SCREEN_HEIGHT {
            for x in 0..SCREEN_WIDTH {
                let pixel_offset = (y * SCREEN_WIDTH + x) * 2;
                let vram_addr = upbase + pixel_offset as u32;

                // Read RGB565 pixel (little-endian)
                let lo = self.bus.peek_byte(vram_addr) as u16;
                let hi = self.bus.peek_byte(vram_addr + 1) as u16;
                let rgb565 = lo | (hi << 8);

                // Convert RGB565 to ARGB8888
                let r = ((rgb565 >> 11) & 0x1F) as u8;
                let g = ((rgb565 >> 5) & 0x3F) as u8;
                let b = (rgb565 & 0x1F) as u8;

                // Expand to 8-bit (replicate high bits into low bits)
                let r8 = (r << 3) | (r >> 2);
                let g8 = (g << 2) | (g >> 4);
                let b8 = (b << 3) | (b >> 2);

                let argb = 0xFF000000 | ((r8 as u32) << 16) | ((g8 as u32) << 8) | (b8 as u32);
                self.framebuffer[y * SCREEN_WIDTH + x] = argb;
            }
        }
    }

    /// Get save state size (stub)
    pub fn save_state_size(&self) -> usize {
        1024 // Placeholder
    }

    /// Save state to buffer (stub)
    pub fn save_state(&self, _buffer: &mut [u8]) -> Result<usize, i32> {
        Err(-100) // Not implemented
    }

    /// Load state from buffer (stub)
    pub fn load_state(&mut self, _buffer: &[u8]) -> Result<(), i32> {
        Err(-100) // Not implemented
    }

    /// Get the last stop reason
    pub fn last_stop_reason(&self) -> StopReason {
        self.last_stop
    }

    /// Get current PC
    pub fn pc(&self) -> u32 {
        self.cpu.pc
    }

    /// Get total cycles executed
    pub fn total_cycles(&self) -> u64 {
        self.total_cycles
    }

    /// Peek at a memory byte without affecting emulation state
    pub fn peek_byte(&self, addr: u32) -> u8 {
        self.bus.peek_byte(addr)
    }

    /// Dump execution history for debugging
    /// Returns a string with the last N instructions executed
    pub fn dump_history(&self) -> String {
        let mut output = String::new();
        output.push_str("Execution history (oldest to newest):\n");

        for entry in self.history.iter() {
            let opcode_str: String = entry.opcode[..entry.opcode_len as usize]
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ");

            output.push_str(&format!(
                "  PC={:06X}  {:12}  {}\n",
                entry.pc,
                opcode_str,
                Self::disassemble_opcode(&entry.opcode[..entry.opcode_len as usize])
            ));
        }

        output.push_str(&format!("\nCurrent PC: {:06X}\n", self.cpu.pc));
        output.push_str(&format!("Total cycles: {}\n", self.total_cycles));
        output.push_str(&format!("Stop reason: {:?}\n", self.last_stop));

        output
    }

    /// Simple disassembler for common opcodes
    fn disassemble_opcode(opcode: &[u8]) -> &'static str {
        if opcode.is_empty() {
            return "???";
        }

        match opcode[0] {
            0x00 => "NOP",
            0x01 => "LD BC,nn",
            0x02 => "LD (BC),A",
            0x03 => "INC BC",
            0x04 => "INC B",
            0x05 => "DEC B",
            0x06 => "LD B,n",
            0x07 => "RLCA",
            0x08 => "EX AF,AF'",
            0x09 => "ADD HL,BC",
            0x0A => "LD A,(BC)",
            0x0B => "DEC BC",
            0x0C => "INC C",
            0x0D => "DEC C",
            0x0E => "LD C,n",
            0x0F => "RRCA",
            0x10 => "DJNZ d",
            0x11 => "LD DE,nn",
            0x12 => "LD (DE),A",
            0x18 => "JR d",
            0x20 => "JR NZ,d",
            0x21 => "LD HL,nn",
            0x22 => "LD (nn),HL",
            0x23 => "INC HL",
            0x28 => "JR Z,d",
            0x2A => "LD HL,(nn)",
            0x30 => "JR NC,d",
            0x31 => "LD SP,nn",
            0x32 => "LD (nn),A",
            0x38 => "JR C,d",
            0x3A => "LD A,(nn)",
            0x3E => "LD A,n",
            0x76 => "HALT",
            0xC0 => "RET NZ",
            0xC1 => "POP BC",
            0xC2 => "JP NZ,nn",
            0xC3 => "JP nn",
            0xC4 => "CALL NZ,nn",
            0xC5 => "PUSH BC",
            0xC6 => "ADD A,n",
            0xC7 => "RST 00H",
            0xC8 => "RET Z",
            0xC9 => "RET",
            0xCA => "JP Z,nn",
            0xCB => "CB prefix",
            0xCD => "CALL nn",
            0xD0 => "RET NC",
            0xD1 => "POP DE",
            0xD5 => "PUSH DE",
            0xD8 => "RET C",
            0xD9 => "EXX",
            0xDD => "DD prefix (IX)",
            0xE1 => "POP HL",
            0xE5 => "PUSH HL",
            0xE9 => "JP (HL)",
            0xEB => "EX DE,HL",
            0xED => "ED prefix",
            0xF1 => "POP AF",
            0xF3 => "DI",
            0xF5 => "PUSH AF",
            0xFB => "EI",
            0xFD => "FD prefix (IY)",
            0xFE => "CP n",
            0xFF => "RST 38H",
            _ => "...",
        }
    }

    /// Get CPU register dump for debugging
    pub fn dump_registers(&self) -> String {
        format!(
            "AF={:02X}{:02X} BC={:06X} DE={:06X} HL={:06X}\n\
             IX={:06X} IY={:06X} SP={:06X} PC={:06X}\n\
             Flags: S={} Z={} H={} PV={} N={} C={}\n\
             ADL={} IFF1={} IFF2={} IM={:?} MBASE={:02X}",
            self.cpu.a,
            self.cpu.f,
            self.cpu.bc,
            self.cpu.de,
            self.cpu.hl,
            self.cpu.ix,
            self.cpu.iy,
            self.cpu.sp,
            self.cpu.pc,
            (self.cpu.f >> 7) & 1,
            (self.cpu.f >> 6) & 1,
            (self.cpu.f >> 4) & 1,
            (self.cpu.f >> 2) & 1,
            (self.cpu.f >> 1) & 1,
            self.cpu.f & 1,
            self.cpu.adl,
            self.cpu.iff1,
            self.cpu.iff2,
            self.cpu.im,
            self.cpu.mbase,
        )
    }
}

impl Default for Emu {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_emu() {
        let emu = Emu::new();
        assert_eq!(emu.framebuffer.len(), SCREEN_WIDTH * SCREEN_HEIGHT);
        assert!(!emu.rom_loaded);
    }

    #[test]
    fn test_load_rom() {
        let mut emu = Emu::new();
        // Minimal ROM - flash defaults to 0xFF so we only need the bytes we use
        let rom = vec![0x00, 0x00, 0x76]; // NOP, NOP, HALT
        assert!(emu.load_rom(&rom).is_ok());
        assert!(emu.rom_loaded);
    }

    #[test]
    fn test_empty_rom_fails() {
        let mut emu = Emu::new();
        let rom: Vec<u8> = vec![];
        assert!(emu.load_rom(&rom).is_err());
    }

    #[test]
    fn test_key_state() {
        let mut emu = Emu::new();
        emu.set_key(0, 0, true);
        assert!(emu.bus.key_state()[0][0]);
        emu.set_key(0, 0, false);
        assert!(!emu.bus.key_state()[0][0]);
    }

    #[test]
    fn test_run_cycles() {
        let mut emu = Emu::new();
        // Without ROM loaded, should return 0
        let executed = emu.run_cycles(1000);
        assert_eq!(executed, 0);
    }

    #[test]
    fn test_run_with_rom() {
        let mut emu = Emu::new();
        // Minimal ROM - flash defaults to 0xFF so we only need the bytes we use
        let rom = vec![0x00, 0x00, 0x00, 0x76]; // NOP, NOP, NOP, HALT
        emu.load_rom(&rom).unwrap();
        let executed = emu.run_cycles(1000);

        // Should have executed some cycles and halted
        assert!(executed > 0);
        assert_eq!(emu.last_stop_reason(), StopReason::Halted);
        assert!(emu.cpu.halted);
    }

    #[test]
    fn test_reset() {
        let mut emu = Emu::new();
        // Minimal ROM - flash defaults to 0xFF so we only need the bytes we use
        let rom = vec![0x00, 0x76]; // NOP, HALT
        emu.load_rom(&rom).unwrap();
        emu.run_cycles(100);
        emu.set_key(1, 1, true);
        emu.reset();

        assert_eq!(emu.cpu.pc, 0);
        assert!(!emu.bus.key_state()[1][1]);
        assert_eq!(emu.total_cycles, 0);
    }

    #[test]
    fn test_history() {
        let mut emu = Emu::new();
        // Minimal ROM - flash defaults to 0xFF so we only need the bytes we use
        let rom = vec![0x00, 0x00, 0x00, 0x76]; // NOP, NOP, NOP, HALT
        emu.load_rom(&rom).unwrap();
        emu.run_cycles(100);

        let history = emu.dump_history();
        assert!(history.contains("NOP"));
        assert!(history.contains("HALT"));
    }
}
