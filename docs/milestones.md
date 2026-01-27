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

## Milestone 2: Bus + Memory Skeleton
**Goal:** Implement memory subsystem foundation.

**Deliverables:**
- [ ] Bus with address decoding
- [ ] RAM backing store
- [ ] Flash backing store for ROM
- [ ] MMIO stubs with debug logging
- [ ] Memory read/write tests pass

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
