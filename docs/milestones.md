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

## Milestone 3: eZ80 Interpreter Foundation
**Goal:** Implement CPU instruction execution.

**Deliverables:**
- [ ] Register file and flags
- [ ] PC/SP and interrupt state
- [ ] Core instruction subset (loads, arithmetic, jumps, calls, stack)
- [ ] Cycle counting (approximate)
- [ ] Toy program execution tests

## Milestone 4: ROM Fetch + Early Boot
**Goal:** Execute real ROM code.

**Deliverables:**
- [ ] Initial memory mapping for ROM start
- [ ] Crash diagnostics (PC/opcode ring buffer)
- [ ] Bus fault reporting
- [ ] ROM executes until missing hardware

## Milestone 5: Minimal Peripherals
**Goal:** Reach visible OS UI.

**Deliverables:**
- [ ] Timers and interrupts
- [ ] Keypad matrix MMIO
- [ ] LCD update mechanism
- [ ] Visible OS screen and partial keypad

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
