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

### 5g: Trace Parity Improvements (In Progress)

- [x] Fixed suffix opcode handling (0x40, 0x49, 0x52, 0x5B) to execute atomically with following instruction
  - Suffix + instruction now count as single step, matching CEmu trace behavior
  - Reduced boot step count from 4.2M to 4.19M (8,856 fewer steps)
- [ ] Investigate early boot trace divergence at PC=0x000E50 (OUT0 instruction)
  - CEmu trace generator may have issues with fine-grained emu_run(1) tick stepping

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

## Milestone 6: Android Display Integration ✓

**Goal:** Display emulator screen on Android device.

**Deliverables:**

- [x] VRAM render_frame() FFI function exposed
- [x] JNI bridge calls render_frame() after emu_run_cycles()
- [x] Framebuffer correctly converted RGB565 → ARGB8888
- [x] Android Bitmap displays boot screen
- [x] ON key powers on calculator and wakes from HALT
- [x] Screen shows "RAM Cleared" message after boot

**Current Status:**

- Android app displays the TI-84 CE boot screen correctly
- ON key works to power on and wake from HALT
- Regular keypad input still in progress (OS polling mechanism)

### 6a: Keypad Integration ✓

- [x] ON key (row 2, col 0) raises ON_KEY interrupt and wakes CPU
- [x] any_key_wake signal for regular keys to wake from HALT
- [x] Keypad data registers return live key state when polled
- [x] Regular keys register in TI-OS (keys display on screen)
- [x] Edge detection mechanism for fast key presses
- [x] Port I/O path (via IN/OUT instructions) properly triggers any_key_check
- [x] TI-OS expression parser initialization on first key press
- [x] Boot screen visible with OS version info before first interaction
- [x] Integration tests for basic calculations (5, 6+7, 6*7, 1/2, etc.)

**Key Findings:**

- TI-OS polls keypad data registers (0xF50010-0xF5002F) rather than using interrupts
- ON key uses dedicated ON_KEY interrupt (bit 0) which IS enabled
- KEYPAD interrupt (bit 10) is NOT enabled by TI-OS
- CPU wake from HALT is handled via signals, not interrupts for regular keys
- **CRITICAL**: TI-OS uses port I/O (port 0xA via IN/OUT) not memory-mapped writes, requiring both I/O paths to call any_key_check
- **Expression parser initialization**: First key press after boot auto-injects ENTER to dismiss boot screen and initialize TI-OS parser state (see findings.md)

## Milestone 7: iOS App ✓

**Goal:** Port emulator to iOS platform.

**Deliverables:**

- [x] iOS app builds and runs with Swift/SwiftUI
- [x] CEmu backend integration
- [x] Rust backend integration
- [x] Runtime backend switching between Rust and CEmu
- [x] Swipe-to-open gesture for menu (replaced menu button)
- [x] State persistence for Rust emulator backend

## Milestone 8: Android Backend Switching ✓

**Goal:** Support multiple emulator backends on Android.

**Deliverables:**

- [x] CEmu backend integration
- [x] Runtime backend switching between Rust and CEmu
- [x] Backend selection UI

## Milestone 9: Persistence (In Progress)

**Goal:** State survives app restarts.

**Deliverables:**

- [ ] Flash write/erase behavior
- [x] Rust backend: Save flash in state (now includes 4MB flash)
- [x] Save-state buffer APIs (Rust: ~4.5MB with flash, CEmu: ~4.5MB)
- [x] iOS save/load state (Rust backend)
- [x] iOS save/load state (CEmu backend)
- [x] Android save/load state
- [ ] State persistence verified on both platforms

## Milestone 10: Web App (WASM)

**Goal:** Run emulator in web browser via WebAssembly.

**Deliverables:**

- [ ] Rust core compiles to WASM target
- [ ] CEmu backend compiles to WASM (via Emscripten)
- [ ] JavaScript/TypeScript bindings for emulator API
- [ ] Web UI with canvas rendering
- [ ] Keypad input handling (keyboard + touch)
- [ ] Runtime backend switching in browser
- [ ] ROM file loading via file picker
- [ ] State persistence via IndexedDB/localStorage

## Milestone 11: Polish

**Goal:** Production-ready apps on all platforms.

**Deliverables:**

- [ ] Nearest-neighbor scaling
- [ ] Speed toggle (normal/turbo)
- [ ] Debug overlay (optional)
- [ ] Accurate keypad layout

## Milestone 12: CEmu Parity (Research Complete)

**Goal:** Achieve closer logical parity with CEmu reference emulator.

**Research Status:** Complete (2026-02-02)

8 parallel research agents analyzed all difference categories in [cemu_core_comparison.md](cemu_core_comparison.md).

### Priority Assessment

| Area | Gap | Priority | Boot Impact |
|------|-----|----------|-------------|
| CPU Cycle Accounting | Internal cycles not applied | MODERATE | None |
| CPU Protection Enforcement | No unprivileged checks | LOW (boot) / HIGH (security) | None |
| Flash Cache/Serial Mode | Not implemented | MODERATE | None (parallel works) |
| Scheduler Events | Missing TIMER_DELAY, KEYPAD, WATCHDOG | MODERATE | None |
| Timer Global Registers | Missing 0x34/0x38 registers | MODERATE | None |
| RTC Time Ticking | Counter never advances | MODERATE | Clock shows 00:00 |
| Keypad Control Packing | Different register layout | LOW | None |
| LCD Timing Registers | Stored but ignored (fixed 60Hz) | LOW | None |
| SPI Device Abstraction | No FIFO data, no devices | MODERATE | None |
| SHA256 | No hash computation | LOW | None |
| Watchdog | No countdown/state machine | LOW | None |

### Key Finding: Boot and TI-OS Work Correctly

All research agents confirmed that **boot and basic TI-OS operation work correctly** with current implementation. The gaps are refinements for:
- Advanced features (time display, indexed color modes, newer OS versions)
- Edge cases (security enforcement, cycle-exact timing)
- Unused peripherals (SHA256, full watchdog)

### Recommended Implementation Order (if needed)

1. **Timer Global Registers** - Easy add, enables app compatibility
2. **RTC Time Ticking** - Enables clock display in status bar
3. **CPU Cycle Accounting** - Improves timing accuracy
4. **SPI Device Abstraction** - Needed for OS 5.7.0+ coprocessor

### Research Reports

Full research reports from 8 subagents cover:
- CPU execution model (prefetch, cycles, protection, signals, IM3)
- Bus/memory/flash (cache, wait states, flash commands, MMIO)
- Scheduler (event coverage, CPU integration, base clock)
- Interrupt/timer (global registers, delayed delivery, OS timer)
- RTC (time ticking, latch mechanism, alarm)
- Keypad (control packing, scan modes, ghosting, GPIO)
- LCD (timing registers, palette/cursor, DMA, panel)
- SPI/SHA256/Backlight/Watchdog (FIFO, hash, port state)
