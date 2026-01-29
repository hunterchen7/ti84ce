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

### 5a: Core Peripherals ✓
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

### 5b: Control Port Initialization Fixes ✓
- [x] Fix CPU speed default: 6 MHz (0x00) instead of 48 MHz (0x03)
- [x] Set PWR interrupt (bit 15) during reset
- [x] Fix flash map_select default: 0x06 instead of 0x00
- [x] Fix flash wait_states default: 0x04 instead of 0x00
- [x] Set protected memory defaults to 0xD1887C (start=end)
- [x] Add privileged region ports (0x1D-0x1F) with is_unprivileged() check
- [x] Fix battery_status default: 0x00 instead of 0x0B
- [x] Fix port 0x0D LCD enable to duplicate nibble on write (CEmu behavior)
- [x] Fix port 0x0F USB control to mask with 0x03 on write

### 5c: Missing Peripheral Stubs
- [ ] Watchdog timer (port 0x6) - basic stub
- [ ] RTC (port 0x8) - read-only stub returning safe values
- [ ] SHA256 accelerator (port 0x2) - stub or full implementation
- [ ] SPI controller (port 0xD) - basic stub

### 5d: Boot Debugging ✓
- [x] Compare execution trace with CEmu at divergence point
- [x] Verify LDIR/block instructions execute during boot
- [x] Trace why RAM isn't initialized (107 writes = stack only)
- [x] Test with corrected control port defaults

### 5e: Visible OS Screen
- [ ] ROM successfully copies code to RAM
- [ ] Execution continues past RAM initialization
- [ ] LCD shows boot screen or OS UI

**Current Status (322 tests passing):**
- Control port defaults now match CEmu (CPU speed, flash, PWR interrupt, protection)
- Added privileged boundary register (ports 0x1D-0x1F) and is_unprivileged() check
- Fixed battery_status, LCD enable nibble duplication, USB control masking
- **Boot trace matches CEmu for 40,000+ steps** (full trace comparison)
- ROM boots to initialization loop, VRAM filled with white pixels
- CPU reaches main initialization code at ~50M cycles

### 5f: CPU/Bus Fixes (Completed)
- [x] Fixed L/IL suffix mode handling (eZ80 suffix opcodes)
- [x] Fixed block instruction internal looping (LDIR, LDDR, CPIR, CPDR)
- [x] Fixed eZ80 block I/O instructions (OTIMR, OTDMR, INIMR, INDMR)
- [x] Fixed ED z=5 RETN/RETI (only y=0,1 valid, others are NOP)
- [x] Fixed IN r,(C) and OUT (C),r to use memory-mapped I/O at 0xFF00xx
- [x] Fixed ED x=0 z=7 instructions (LD rp3,(HL), LD (HL),rp3, LD IY,(HL), LD (HL),IY)
- [x] Updated LDIR test for internal looping behavior

**Key Progress:**
- Execution trace matches CEmu for 40,001+ steps
- VRAM is being written (screen shows all white pixels)
- ROM is now waiting on port 0x0D (LCD enable) status

**Trace Comparison Commands:**
```bash
# Capture emu-core trace
cargo run --example trace_boot --manifest-path core/Cargo.toml > trace_ours.log

# Capture CEmu trace (requires cemu-ref/ clone with trace_cli)
./cemu-ref/trace_cli > trace_cemu.log 2>&1
```

**Current Blocker:**
ROM is stuck in a loop at PC=0x5BA9 polling port 0x0D (LCD enable).
The loop reads port 0x0D, ANDs with a mask, and loops while non-zero.
This may require proper LCD enable/status bit simulation.

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
