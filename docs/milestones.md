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

## Milestone 5: Minimal Peripherals ✓

**Goal:** Reach visible OS UI. **COMPLETE** - OS boots to home screen with "RAM Cleared" message.

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
- [x] Watchdog timer (port 0x6) - basic stub
- [x] RTC (port 0x8) - read-only stub returning safe values
- [ ] SHA256 accelerator (port 0x2) - stub or full implementation
- [x] SPI controller (port 0xD) - status stub returning reset values

### 5d: Boot Debugging ✓
- [x] Compare execution trace with CEmu at divergence point
- [x] Verify LDIR/block instructions execute during boot
- [x] Trace why RAM isn't initialized (107 writes = stack only)
- [x] Test with corrected control port defaults

### 5e: Visible OS Screen ✓
- [x] ROM successfully copies code to RAM
- [x] Execution continues past RAM initialization
- [x] LCD shows boot screen or OS UI
- [x] "RAM Cleared" message displayed on screen
- [x] Status bar shows "NORMAL FLOAT AUTO REAL RADIAN CL"
- [x] CPU reaches OS idle loop (EI + HALT at 0x085B7F)

**Current Status (358 tests passing):**
- **🎉 BOOT COMPLETE** - TI-84 CE OS boots to home screen with "RAM Cleared" message
- Emulator runs to **3,609,969 steps** (~61.6M cycles) before normal OS HALT (idle wait)
- LCD shows full OS UI: status bar "NORMAL FLOAT AUTO REAL RADIAN CL" + battery indicator
- VRAM contains 4 distinct colors: 88% white background, 11% dark green UI, 1% black text, 0.2% red
- CPU reaches OS idle loop at PC=0x085B7F (EI + NOP + HALT sequence)
- **Scheduler implemented** - 7.68 GHz base clock with proper event timing
- Control port defaults match CEmu (CPU speed, flash, PWR interrupt, protection)
- Fixed all critical CPU instructions: IM mapping, MLT, LEA, LD A,MB, indexed loads
- SPI timing matches CEmu (24MHz tick conversion, FIFO depth 16, RX-only transfers)
- Flash command emulation for sector erase (80h sequence returns 80h status)

### 5f: CPU/Bus Fixes (Completed)
- [x] Fixed L/IL suffix mode handling (eZ80 suffix opcodes)
- [x] Fixed block instruction internal looping (LDIR, LDDR, CPIR, CPDR)
- [x] Fixed eZ80 block I/O instructions (OTIMR, OTDMR, INIMR, INDMR)
- [x] Fixed ED z=5 RETN/RETI (only y=0,1 valid, others are NOP)
- [x] Fixed IN r,(C) and OUT (C),r to use full BC as 16-bit port address (not 0xFF00|C)
- [x] Fixed ED x=0 z=7 instructions (LD rp3,(HL), LD (HL),rp3, LD IY,(HL), LD (HL),IY)
- [x] Updated LDIR test for internal looping behavior
- [x] Added Bus::port_read/port_write with 16-bit port routing (bits 15:12 select peripheral)
- [x] Fixed ED 6E (LD A,MB) - was being treated as NOP, now loads MBASE into A
- [x] Added ED 6D (LD MB,A), ED 7D (STMIX), ED 7E (RSMIX) eZ80 instructions
- [x] Added ED 65 (PEA IX+d) and ED 66 (PEA IY+d) instructions
- [x] Added madl field to CPU struct for mixed memory mode
- [x] Fixed IM instruction mapping: eZ80 maps y value directly to IM mode (ED 56 = IM 2, not IM 1)

**Key Progress:**
- **🎉 MILESTONE 5 COMPLETE** - OS boots to visible home screen
- Screen displays "RAM Cleared" message with full status bar
- Execution trace matches CEmu for **1,000,000+ steps** (all available CEmu trace data)
- Scheduler parity not required for correct boot (timing differences acceptable)
- CPU reaches OS idle loop at 0x085B7F after 3.6M steps
- VRAM filled with actual UI content (not just white pixels)

**Debug Tool:**
```bash
# All-in-one debug tool for testing and tracing
cargo run --release --example debug -- help

# Quick commands:
cargo run --release --example debug -- boot      # Run boot test
cargo run --release --example debug -- screen    # Render screen to PNG
cargo run --release --example debug -- vram      # Analyze VRAM colors
cargo run --release --example debug -- trace 1M  # Generate trace
cargo run --release --example debug -- compare <cemu_trace>  # Compare traces
```

**Boot Success Verified:**
- ROM boots in 3,609,969 steps (~61.6M cycles at 48MHz)
- LCD control: 0x0000092D (16bpp RGB565, power on)
- VRAM base: 0xD40000
- OS reaches idle state with interrupts enabled (EI + HALT)

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
