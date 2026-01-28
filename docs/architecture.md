# Architecture

## Overview

The TI-84 Plus CE emulator is designed with a clean separation between the platform-agnostic emulator core and platform-specific shells.

```
┌─────────────────────────────────────────────────────────┐
│                    Platform Shell                        │
│  (Android: Kotlin/Compose, iOS: Swift/SwiftUI)          │
├─────────────────────────────────────────────────────────┤
│                    C ABI Bridge                          │
│  (Android: JNI/C++, iOS: Swift C interop)               │
├─────────────────────────────────────────────────────────┤
│                    Rust Core                             │
│  (Platform-agnostic, no OS APIs)                        │
└─────────────────────────────────────────────────────────┘
```

## Core Design Principles

### 1. No Platform Dependencies
The core contains no file I/O, no platform logging, and no threading. All data is passed in/out via byte buffers.

### 2. Stable C ABI
The core exposes a stable C ABI that doesn't leak Rust types. This allows any platform to integrate with minimal effort.

### 3. Single-Threaded Core
The core is single-threaded and deterministic. Threading is owned by the platform shell.

### 4. Buffer-Based I/O
- ROM is passed in as bytes
- Framebuffer is exposed as a pointer to ARGB8888 data
- Save states are serialized to/from byte buffers

## C ABI Exports

```c
// Lifecycle
Emu* emu_create(void);
void emu_destroy(Emu* emu);
void emu_reset(Emu* emu);

// ROM loading
int32_t emu_load_rom(Emu* emu, const uint8_t* data, size_t len);

// Execution
int32_t emu_run_cycles(Emu* emu, int32_t cycles);

// Display
const uint32_t* emu_framebuffer(const Emu* emu, int32_t* w, int32_t* h);

// Input
void emu_set_key(Emu* emu, int32_t row, int32_t col, int32_t down);

// Save states
size_t emu_save_state_size(const Emu* emu);
int32_t emu_save_state(const Emu* emu, uint8_t* out, size_t cap);
int32_t emu_load_state(Emu* emu, const uint8_t* data, size_t len);
```

## Core Components

### Module Structure
```
core/src/
├── lib.rs      # C ABI exports and public interface
├── emu.rs      # Main emulator orchestrator
├── cpu/        # eZ80 CPU implementation
│   ├── mod.rs      # Cpu struct, step(), module exports
│   ├── flags.rs    # Flag bit constants
│   ├── helpers.rs  # Register access, fetch, push/pop, ALU
│   ├── execute.rs  # Instruction execution functions
│   └── tests/      # CPU test suite
├── bus.rs      # System bus with address decoding
└── memory.rs   # Flash, RAM, and Port implementations
```

### Emu (Orchestrator)
The main emulator struct that owns all subsystems and coordinates execution.

### CPU (eZ80)
The eZ80 processor implementation running at 48 MHz in ADL mode (24-bit addressing).

**Registers:**
- Main: A, F, BC, DE, HL (24-bit in ADL mode)
- Shadow: A', F', BC', DE', HL' (for EX AF,AF' and EXX)
- Index: IX, IY (24-bit in ADL mode)
- Special: PC, SP (SPL in ADL), I, R, MBASE

**Flags (F register):**
- S (Sign), Z (Zero), H (Half-carry)
- PV (Parity/Overflow), N (Subtract), C (Carry)
- F5, F3 (undocumented, copies of result bits)

**Instruction Set (implemented):**
- Load: LD r,r' / LD r,n / LD rp,nn / LD (rp),A / LD A,(rp)
- Arithmetic: ADD, ADC, SUB, SBC, INC, DEC, NEG, DAA
- Logic: AND, OR, XOR, CP, CPL
- Rotate (basic): RLCA, RRCA, RLA, RRA
- Control: JP, JR, DJNZ, CALL, RET, RETI, RETN, RST, HALT
- Stack: PUSH, POP
- Exchange: EX AF,AF' / EXX / EX DE,HL / EX (SP),HL
- Misc: NOP, DI, EI, SCF, CCF, IM 0/1/2

**CB Prefix (bit operations):**
- Rotate/Shift: RLC, RRC, RL, RR, SLA, SRA, SRL
- Bit test: BIT n,r
- Bit manipulation: SET n,r / RES n,r

**ED Prefix (extended operations):**
- 16-bit arithmetic: ADC HL,rp / SBC HL,rp
- Block transfer: LDI, LDIR, LDD, LDDR
- Block compare: CPI, CPIR, CPD, CPDR
- Rotate decimal: RRD, RLD
- Register: LD I,A / LD A,I / LD R,A / LD A,R
- I/O: IN r,(C) / OUT (C),r (blocked on TI-84 CE)

**DD/FD Prefix (indexed operations):**
- All HL instructions work with IX (DD) or IY (FD)
- Indexed addressing: (IX+d), (IY+d)
- Half-register access: IXH, IXL, IYH, IYL
- DDCB/FDCB: Bit ops on indexed memory

**Opcode decoding:**
Uses x-y-z-p-q decomposition (standard Z80 decode scheme)

### Bus
Memory bus with 24-bit address decoding:

| Address Range       | Region              | Size    |
|---------------------|---------------------|---------|
| 0x000000 - 0x3FFFFF | Flash               | 4MB     |
| 0x400000 - 0xCFFFFF | Unmapped            | -       |
| 0xD00000 - 0xD657FF | RAM + VRAM          | 415KB   |
| 0xD65800 - 0xDFFFFF | Unmapped            | -       |
| 0xE00000 - 0xFFFFFF | Memory-mapped I/O   | 2MB     |

Wait states for accurate timing:
- RAM read: 4 cycles (3 wait states)
- RAM write: 2 cycles (1 wait state)
- Flash read: 10 cycles
- Port access: 3-4 cycles

### Memory
RAM and Flash backing stores:
- Flash: 4MB, erased state is 0xFF, read-only from CPU
- RAM: 415KB contiguous region including VRAM
- VRAM: Last 150KB of RAM (0xD40000 - 0xD657FF)
- Unmapped reads return pseudo-random values (LFSR-based RNG)

### Hardware
Peripheral implementations:
- LCD controller
- Keypad matrix
- Timers
- Interrupt controller

## Android Shell

### JNI Bridge
Thin C++ layer that:
- Holds `Emu*` as `jlong` handle
- Converts between Java types and C types
- Copies framebuffer data

### Compose UI
- Single activity with Compose-based UI
- Emulation runs on background coroutine
- Framebuffer rendered as Bitmap
- Touch events mapped to keypad matrix

## Data Flow

### ROM Loading
```
User picks file → ContentResolver reads bytes → JNI passes to core → Core stores in flash
```

### Frame Rendering
```
Core updates framebuffer → JNI copies to IntArray → Kotlin creates Bitmap → Compose renders
```

### Key Input
```
Touch event → Compose callback → JNI call → Core updates key matrix
```
