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

## Phase 2: Bus & Address Decoding — [ ]
**Effort: M | Risk: Low-Med**

- [ ] **2A** Flash 0x400000-0xBFFFFF routing (`bus.rs:~591`) — route through flash (not Unmapped)
- [ ] **2B** MMIO unmapped holes (`bus.rs`) — 0xE40000-0xEFFFFF and 0xFB0000-0xFEFFFF not to port handlers
- [ ] **2C** Port range 0xF routing (`bus.rs`) — fxxx handler, not Control
- [ ] **2D** SPI in memory-mapped path (`peripherals/mod.rs`) — add 0x0D0000 range
- [ ] **2E** Backlight in mod.rs (`peripherals/mod.rs`) — enable BACKLIGHT_BASE match arms

**Verify**: `cargo t` → `cargo boot` → `cargo trace 100000` + fullcompare

---

## Phase 3: Peripheral Register Layout Rewrites — [ ]
**Effort: XL | Risk: Med**

### 3A: Timer Rewrite (`peripherals/timer.rs`, `peripherals/mod.rs`)
- [ ] Replace 3 separate Timers with single `GeneralTimers`
- [ ] Shared control at 0x30 (32-bit, 3 bits/timer + direction), status at 0x34, mask at 0x38, revision 0x3C (0x00010801)
- [ ] Timer 0/1/2 at offsets 0x00/0x10/0x20 (counter/reset/match0/match1)
- [ ] Ref: `cemu-ref/core/timers.h:17-28`

### 3B: Keypad Register Packing (`peripherals/keypad.rs`)
- [ ] 32-bit control at 0x00: bits [1:0]=mode, [15:2]=rowWait, [31:16]=scanWait
- [ ] Remove ROW_WAIT (0x30), SCAN_WAIT (0x34)
- [ ] 16 data registers (not 8), GPIO enable at 0x40
- [ ] Fix reset mask 0xFFFF, enable mask `& 0x07`, scan clock 6MHz
- [ ] Ref: `cemu-ref/core/keypad.c`, `keypad.h:20-46`

### 3C: Watchdog Offset Fix (`peripherals/watchdog.rs`)
- [ ] Counter→0x00, Load→0x04, Restart(0xB9)→0x08, Control→0x0C, Status clear→0x14, Revision→0x1C (0x00010602)
- [ ] Fix reset load: 0x03EF1480
- [ ] Remove lock register (0xC0)
- [ ] Ref: `cemu-ref/core/misc.c:128-148`

**Verify**: `cargo t` → `cargo boot` → `cargo trace 100000` + fullcompare

---

## Phase 4: Scheduler & Timing — [ ]
**Effort: L | Risk: Med**

- [ ] **4A** SCHED_SECOND overflow prevention (`scheduler.rs`) — subtract base_clock_rate from all timestamps every second
- [ ] **4B** CPU speed change event conversion (`scheduler.rs`) — convert all ClockId::Cpu event timestamps on speed change
- [ ] **4C** Panel clock rate (`scheduler.rs:~50`) — 60 Hz → 10,000,000 Hz
- [ ] **4D** OS Timer interrupt phase (`peripherals/mod.rs`) — set interrupt to OLD state before toggle; add clear_raw on false
- [ ] **4E** Timer 32kHz clock source (`peripherals/timer.rs`) — control bit selects CLOCK_32K vs CLOCK_CPU
- [ ] **4F** Timer 2-cycle interrupt delay — SCHED_TIMER_DELAY pipeline

**Verify**: `cargo t` → `cargo boot` → `cargo trace 100000` + fullcompare

---

## Phase 5: RTC, SHA256, Control Ports — [ ]
**Effort: M | Risk: Low**

- [ ] **5A** RTC time counting (`peripherals/rtc.rs`) — 3-state machine, sec→min→hour→day rollover, 6 interrupt types
- [ ] **5B** RTC load data transfer (`peripherals/rtc.rs`) — bit-level transfer from load→counter
- [ ] **5C** SHA256 process_block (`peripherals/sha256.rs`) — 64-round compression, fix control decode (independent ifs), fix reset (zero not IV)
- [ ] **5D** Control port masks (`peripherals/control.rs`) — port 0x01: `& 0x13`, port 0x29: `& 1`
- [ ] **5E** Flash size_config reset (`peripherals/flash.rs`) — 0x07 → 0x00
- [ ] **5F** INT_PWR on reset (`peripherals/mod.rs`) — raise after interrupt.reset()

**Verify**: `cargo t` → `cargo boot` → `cargo trace 100000` + fullcompare

---

## Phase 6: LCD & SPI Enhancements — [ ]
**Effort: L | Risk: Low**

- [ ] **6A** Fix ICR register (`peripherals/lcd.rs`) — offset 0x28 = interrupt clear, not PALBASE
- [ ] **6B** Add palette color modes — 256 entries at 0x200-0x3FF, modes 0-7, BGR/RGB swap
- [ ] **6C** Basic LCD DMA engine — scheduler event on CLOCK_48M, UPCURR advancement
- [ ] **6D** SPI panel stub (`peripherals/spi.rs`, new `peripherals/panel.rs`) — minimal ST7789V

**Verify**: `cargo t` → `cargo boot` → `cargo screen` → `cargo trace 100000` + fullcompare

---

## Phase 7: CPU Advanced & Bus Protection — [ ]
**Effort: XL | Risk: High**

- [ ] **7A** Separate SPS/SPL (`cpu/mod.rs`, `cpu/helpers.rs`) — replace single sp with sps+spl
- [ ] **7B** Mixed-mode CALL/RET/RST (`cpu/execute.rs`, `cpu/helpers.rs`) — MADL|ADL flag byte
- [ ] **7C** Memory protection (`bus.rs`, `peripherals/control.rs`) — stack limit NMI, protected region, unprivileged I/O check
- [ ] **7D** DMA scheduling (`scheduler.rs`, `emu.rs`) — DMA events steal CPU cycles
- [ ] **7E** CPU cycle parity — HALT cycles, interrupt prefetch_discard, R register rotation, LD A,I PV=IFF1

**Verify**: `cargo t` → `cargo boot` → full trace comparison 1M+ steps

---

## Summary

| Phase | Focus | Critical | High | Effort |
|-------|-------|:--------:|:----:|:------:|
| 1 | CPU Instructions | 5 | 2 | L |
| 2 | Bus/Address Decoding | 3 | — | M |
| 3 | Peripheral Registers | 4 | — | XL |
| 4 | Scheduler & Timing | — | 8 | L |
| 5 | RTC/SHA256/Control | 3 | 3 | M |
| 6 | LCD & SPI | 2 | 2 | L |
| 7 | CPU Advanced & Bus | 3 | 4 | XL |
| **Total** | | **20** | **19** | |
