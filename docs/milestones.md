# Milestones

## Milestone 1: End-to-End Plumbing ✓

**Goal:** Prove Rust core + JNI + Compose rendering + keypad input work together.

**Deliverables:**

- [x] Core crate compiles and exports C ABI
- [x] Android app builds and runs
- [x] Animated framebuffer display (moving gradient)
- [x] Key presses visibly affect framebuffer
- [x] Run/Pause functionality
- [x] Reset functionality
- [x] ROM import UI (loads bytes, unused in dummy core)

## Milestone 2: Bus + Memory Skeleton ✓

**Goal:** Implement memory subsystem foundation.

**Deliverables:**

- [x] Bus with address decoding (24-bit eZ80 address space)
- [x] RAM backing store (0x65800 bytes at 0xD00000)
- [x] Flash backing store for ROM (4MB at 0x000000)
- [x] MMIO port stubs (0xE00000 - 0xFFFFFF)
- [x] Memory read/write tests pass (44 tests)
- [x] Cycle counting with accurate wait states
- [x] Pseudo-random values for unmapped reads
- [x] peek/poke debug access without cycle cost

## Milestone 3: eZ80 Interpreter Foundation ✓

**Goal:** Implement CPU instruction execution.

**Deliverables:**

- [x] Register file and flags (A, F, BC, DE, HL, IX, IY + shadows)
- [x] PC/SP and interrupt state (IFF1/IFF2, IM, ADL mode)
- [x] Core instruction subset:
  - LD r,r' / LD r,n / LD rp,nn
  - ADD/ADC/SUB/SBC/AND/OR/XOR/CP
  - INC/DEC (8-bit and 16-bit)
  - JP/JR/DJNZ (conditional and unconditional)
  - CALL/RET/RST
  - PUSH/POP
  - EX AF,AF' / EXX / EX DE,HL
  - DI/EI/HALT
  - Rotate instructions (RLCA, RRCA, RLA, RRA)
- [x] Cycle counting (per-instruction)
- [x] Instruction execution tests (198 total, including Z80/ADL mode coverage)

## Milestone 4: ROM Fetch + Early Boot ✓

**Goal:** Execute real ROM code.

**Deliverables:**

- [x] Initial memory mapping for ROM start
- [x] Crash diagnostics (PC/opcode ring buffer)
- [x] Bus fault reporting (StopReason enum)
- [x] ROM executes until missing hardware (HALT at 0x001414)

## Milestone 5: Minimal Peripherals (In Progress)

**Goal:** Reach visible OS UI.

**Deliverables:**

- [x] Interrupt controller (0xF00000) with source tracking
- [x] CPU interrupt dispatch (Mode 0/1/2, NMI support)
- [x] General purpose timers (3x at 0xF20000)
- [x] LCD controller (0xE30000) with VRAM pointer
- [x] Keypad controller (0xF50000) with 8x8 matrix
- [x] Control ports (0xE00000) - CPU speed, power, flash unlock
- [x] Peripheral tick integration in emulator loop
- [x] render_frame() for RGB565 → ARGB8888 conversion
- [x] ON key wake-from-HALT (special non-maskable wake signal)
- [x] Flash controller (0xE10000) - wait states, status registers
- [ ] Visible OS screen

**Current Status (305 tests passing):**
- ROM executes ~4000 cycles of initialization
- Sets up stack, interrupt mode, CPU speed
- Writes 0x10 to power port then HALTs at 0x001414
- ON key can now wake CPU from HALT even with interrupts disabled
- Flash controller returns ready status for ROM boot

**Next Steps:**
1. Investigate what else is needed for ROM to progress past HALT
2. Investigate CEmu for any other required hardware for early boot

## Milestone 6: Persistence

**Goal:** State survives app restarts.

**Deliverables:**

- [ ] Flash write/erase behavior
- [ ] Save-state buffer APIs
- [ ] Android save/load state
- [ ] State persistence verified

## Milestone 7: Android Polish

**Goal:** Production-ready Android app.

**Deliverables:**

- [ ] Nearest-neighbor scaling
- [ ] Speed toggle (normal/turbo)
- [ ] Debug overlay (optional)
- [ ] Accurate keypad layout
