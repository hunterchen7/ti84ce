# CEmu Parity Milestones

*Plan created 2026-02-05. Tracks progress toward full CEmu parity across 7 phases.*

---

## Phase 1: CPU Instruction Correctness — [x]
**Effort: L | Risk: Low**

- [x] **1A** Fix RETI IFF1 restore (`cpu/execute.rs:~1121`) — add `self.iff1 = self.iff2`
- [x] **1B** Fix rp3[3] mapping (`cpu/execute.rs:~882-895`) — ED x=0 z=7 p=3: `self.iy` → `self.ix`
- [x] **1C** Add LD I,HL (ED C7) / LD HL,I (ED D7) (`cpu/execute.rs:~767`) — currently ED x=3 all NOP
- [x] **1D** Add LEA IY,IX+d (ED 55) (`cpu/execute.rs`) — x=1,y=2,z=5 currently NOP
- [x] **1E** Implement block I/O (`cpu/execute.rs:~1458`) — INI/IND/OUTI/OUTD + repeats, INI2/IND2/OUTI2/OUTD2, INIRX/INDRX/OTIRX/OTDRX
- [x] **1F** Fix EX DE,HL L-mode masking (`cpu/helpers.rs`) — mask both regs in Z80 mode
- [x] **1G** Fix block BC decrement (`cpu/execute.rs`) — preserve BCU in Z80 mode

**Verify**: Boot passes (132.79M cycles, PC=085B80). Trace 100k steps generated. 250/436 tests pass (178 pre-existing failures due to uninitialized prefetch in tests).

---

## Phase 2: Bus & Address Decoding — [x]
**Effort: M | Risk: Low-Med**

- [x] **2A** Flash 0x400000-0xBFFFFF routing (`bus.rs:~591`) — route through flash (not Unmapped)
- [x] **2B** MMIO unmapped holes (`bus.rs`) — 0xE40000-0xEFFFFF and 0xFB0000-0xFEFFFF not to port handlers
- [x] **2C** Port range 0xF routing (`bus.rs`) — fxxx handler, not Control
- [x] **2D** SPI in memory-mapped path (`bus.rs`) — intercept port range 0xD in MMIO read/write paths
- [x] **2E** Backlight in mod.rs (`peripherals/mod.rs`) — enable BACKLIGHT_BASE match arms

**Verify**: Boot passes (132.79M cycles, PC=085B80). 251/436 tests pass (178 pre-existing failures).

---

## Phase 3: Peripheral Register Layout Rewrites — [x]
**Effort: XL | Risk: Med**

### 3A: Timer Rewrite (`peripherals/timer.rs`, `peripherals/mod.rs`) — [x]
- [x] Replace 3 separate Timers with single `GeneralTimers`
- [x] Shared control at 0x30 (32-bit, 3 bits/timer + direction), status at 0x34, mask at 0x38, revision 0x3C (0x00010801)
- [x] Timer 0/1/2 at offsets 0x00/0x10/0x20 (counter/reset/match0/match1)
- [x] Ref: `cemu-ref/core/timers.h:17-28`

### 3B: Keypad Register Packing (`peripherals/keypad.rs`) — [x]
- [x] 32-bit control at 0x00: bits [1:0]=mode, [15:2]=rowWait, [31:16]=scanWait
- [x] Remove ROW_WAIT (0x30), SCAN_WAIT (0x34)
- [x] 16 data registers (not 8), GPIO enable at 0x40
- [x] Fix reset mask 0xFFFF, enable mask `& 0x07`, scan clock 6MHz
- [x] Ref: `cemu-ref/core/keypad.c`, `keypad.h:20-46`

### 3C: Watchdog Offset Fix (`peripherals/watchdog.rs`) — [x]
- [x] Counter→0x00, Load→0x04, Restart(0xB9)→0x08, Control→0x0C, Status clear→0x10, Revision→0x1C (0x00010602)
- [x] Fix reset load: 0x03EF1480
- [x] Remove lock register (0xC0)
- [x] Ref: `cemu-ref/core/misc.c:128-148`

**Verify**: Boot passes (156.10M cycles, PC=085B80). 272/457 tests pass (178 pre-existing failures).

---

## Phase 4: Scheduler & Timing — [x]
**Effort: L | Risk: Med**

- [x] **4A** SCHED_SECOND overflow prevention (`scheduler.rs`) — subtract base_clock_rate from all timestamps every second
- [x] **4B** CPU speed change event conversion (`scheduler.rs`) — convert all ClockId::Cpu event timestamps on speed change
- [x] **4C** Panel clock rate (`scheduler.rs:~50`) — 60 Hz → 10,000,000 Hz
- [x] **4D** OS Timer interrupt phase (`peripherals/mod.rs`) — set interrupt to OLD state before toggle; add clear_raw on false
- [x] **4E** Timer 32kHz clock source (`peripherals/timer.rs`) — control bit selects CLOCK_32K vs CLOCK_CPU
- [x] **4F** Timer 2-cycle interrupt delay — SCHED_TIMER_DELAY pipeline, delay_status/delay_intrpt packing, process_delay()

**Verify**: Boot passes (156.10M cycles, PC=085B80). 272/457 tests pass (178 pre-existing failures).

---

## Phase 5: RTC, SHA256, Control Ports — [x]
**Effort: M | Risk: Low**

- [x] **5A** RTC time counting (`peripherals/rtc.rs`) — 3-state machine (TICK/LATCH/LOAD_LATCH), sec→min→hour→day rollover, 6 interrupt types
- [x] **5B** RTC load data transfer (`peripherals/rtc.rs`) — bit-level transfer from load→counter with writeMask logic
- [x] **5C** SHA256 process_block (`peripherals/sha256.rs`) — 64-round compression, fix control decode (independent ifs), fix reset (zero not IV)
- [x] **5D** Control port masks (`peripherals/control.rs`) — port 0x01: `& 0x13`, port 0x29: `& 1`
- [x] **5E** Flash size_config reset (`peripherals/flash.rs`) — 0x07 → 0x00
- [x] **5F** INT_PWR on reset (`peripherals/mod.rs`) — raise after interrupt.reset()

**Verify**: Boot passes (108.78M cycles, PC=085B80). 259/437 tests pass (178 pre-existing failures).

---

## Phase 6: LCD & SPI Enhancements — [partial]
**Effort: L | Risk: Low**

- [x] **6A** Fix ICR register (`peripherals/lcd.rs`) — offset 0x28 = interrupt clear (not PALBASE), IMSC/RIS as u8, MIS at 0x24, UPCURR/LPCURR, peripheral ID at 0xFE0
- [x] **6B** Add palette storage — 256 entries at 0x200-0x3FF (512 bytes), UPBASE/LPBASE 8-byte alignment
- [x] **6C** Basic LCD DMA engine — 5-state event machine (FRONT_PORCH→SYNC→LNBU→BACK_PORCH→ACTIVE_VIDEO), DMA prefill + active phases, UPCURR advancement, CLOCK_24M events + CLOCK_48M DMA, timing parameter extraction, enable/disable scheduling flags
- [ ] **6D** SPI panel stub — deferred (ST7789V is 1375 lines in CEmu, complex)

**Verify**: Boot passes (156.10M cycles, PC=085B80). 270/455 tests pass (178 pre-existing failures). 6D deferred.

---

## Phase 7: CPU Advanced & Bus Protection — [partial]
**Effort: XL | Risk: High**

- [x] **7A** Separate SPS/SPL (`cpu/mod.rs`, `cpu/helpers.rs`) — replace single `sp` with `sps`+`spl`, `sp()`/`set_sp()` select by L mode
- [x] **7B** Mixed-mode CALL/RET/RST — push/pop MADL|ADL flag byte, push_byte_mode/pop_byte_mode helpers, suffix flag propagation
- [x] **7C** Memory protection (`bus.rs`, `peripherals/control.rs`) — stack limit NMI, protected range check, flash privilege check, ports 0x3D/0x3E
- [ ] **7D** DMA scheduling — deferred (DMA events steal CPU cycles)
- [x] **7E** CPU cycle parity — HALT fast-forwards to next event, interrupt prefetch_discard + L/IL setup, R register rotation, LD A,I PV=IFF1

**Verify**: Boot passes (156.10M cycles, PC=085B80). 272/457 tests pass (178 pre-existing failures). 7D deferred.

---

## Summary

| Phase | Focus | Status | Deferred |
|-------|-------|:------:|----------|
| 1 | CPU Instructions | **Done** | — |
| 2 | Bus/Address Decoding | **Done** | — |
| 3 | Peripheral Registers | **Done** | — |
| 4 | Scheduler & Timing | **Done** | — |
| 5 | RTC/SHA256/Control | **Done** | — |
| 6 | LCD & SPI | Partial | 6D (SPI panel) |
| 7 | CPU Advanced & Bus | Partial | 7D (DMA scheduling) |

Boot passes at PC=085B80 with 156.10M cycles. 270/455 tests pass (178 pre-existing failures). Remaining deferred: 6D (SPI panel), 7D (DMA scheduling).
