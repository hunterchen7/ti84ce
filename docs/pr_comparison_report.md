# Comprehensive CEmu vs Rust Emulator Comparison Report

*Generated 2026-02-05 — 8 parallel analysis agents compared every Rust source file in `core/src/` against the CEmu reference in `cemu-ref/core/`.*

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [CPU (execute.rs, mod.rs, helpers.rs, flags.rs)](#2-cpu)
3. [Bus & Memory (bus.rs, memory.rs)](#3-bus--memory)
4. [LCD / Backlight / SPI](#4-lcd--backlight--spi)
5. [Interrupt / Timer / Scheduler](#5-interrupt--timer--scheduler)
6. [Control Ports / Flash Controller](#6-control-ports--flash-controller)
7. [Keypad / RTC / Watchdog](#7-keypad--rtc--watchdog)
8. [SHA256 / Peripheral Hub (mod.rs)](#8-sha256--peripheral-hub)
9. [Emu Loop / Lib / Disasm / WASM](#9-emu-loop--lib--disasm--wasm)
10. [Full Issue Tracker](#10-full-issue-tracker)

---

## 1. Executive Summary

The Rust emulator successfully boots the TI-84 CE ROM to the home screen and achieves good high-level parity with CEmu. However, deep comparison reveals **~150 specific discrepancies** across all subsystems. The issues fall into three severity tiers:

### Critical (breaks correctness or blocks features)
- **CPU**: Missing instructions (block I/O, LEA IY,IX+d, LD I,HL, LD HL,I), RETI doesn't restore IFF1, mixed-mode CALL/RET/RST unimplemented, single SP instead of SPS/SPL
- **Timers**: Register layout completely wrong (CEmu uses shared control word, Rust uses per-timer control bytes at different offsets)
- **Keypad**: Control register packing wrong (rowWait/scanWait embedded in CEmu's 32-bit control, separate registers at wrong offsets in Rust)
- **Watchdog**: Nearly every register offset is wrong
- **RTC**: No time counting, no interrupts, no load operation — completely stubbed
- **SHA256**: No hash computation (`process_block()` not implemented)

### High (affects accuracy/timing)
- **Bus**: Flash region 0x400000-0xBFFFFF routing wrong, memory protection entirely missing, MMIO address mapping has unmapped holes not honored
- **LCD**: Only RGB565 mode supported (no palette modes 0-5, no 12/24bpp), no DMA engine, timing is hardcoded instead of register-driven
- **SPI**: No device connection (no panel communication), FIFO is counter-only (no data), no threshold interrupts
- **Scheduler**: Missing SCHED_SECOND overflow prevention, no event timestamp conversion on CPU speed change, no DMA scheduling
- **OS Timer**: Interrupt fires at wrong phase (inverted from CEmu)

### Medium/Low (cosmetic or unlikely to affect boot)
- Missing peripherals: USB, UART, panel controller, protected ports, Cxxx/Fxxx ports
- R register increment scheme differs (1 per fetch_byte vs 2 per instruction with rotation)
- Various missing write masks, side effects, and reset value differences

---

## 2. CPU

**Files compared**: `cpu/mod.rs`, `cpu/execute.rs`, `cpu/flags.rs`, `cpu/helpers.rs` vs `cpu.c`, `cpu.h`, `registers.c`, `registers.h`

### 2.1 Register Model

| Aspect | CEmu | Rust | Status |
|--------|------|------|--------|
| Shadow registers (AF', BC', DE', HL') | Full set | Full set | Match |
| Separate SPS/SPL | Two 24-bit stack pointers | Single `sp: u32` | **MISSING** |
| R register storage | Rotated: `(A<<1)\|(A>>7)`, +2/insn | Direct storage, +1/fetch_byte | **DIFFERENT** |
| I register | 16-bit | 16-bit | Match |
| MBASE | 8-bit | 8-bit | Match |
| MADL flag | Implemented | Not implemented | **MISSING** |
| IEF_wait / eiDelay | Cycle-based delay | Step-count-based delay | Different |

### 2.2 Missing Instructions

| Instruction | Encoding | Impact |
|-------------|----------|--------|
| INI/IND/INIR/INDR/OUTI/OUTD/OTIR/OTDR | ED (block I/O z=2,3) | **Stubbed** — 16 cycles, no side effects |
| INI2/IND2/OUTI2/OUTD2/INI2R/IND2R/OTI2R/OTD2R | ED (block I/O z=4) | **Missing** |
| INIRX/INDRX/OTIRX/OTDRX | ED x=3 (C2/C3/CA/CB) | **Missing** |
| LD I,HL | ED C7 | **Missing** (ED x=3 all NOP) |
| LD HL,I | ED D7 | **Missing** |
| LEA IY,IX+d | ED 55 | **Missing** (treated as NOP) |

### 2.3 Instruction Bugs

| Issue | Detail |
|-------|--------|
| **RETI doesn't restore IFF1** | CEmu: `IEF1 = IEF2` for both RETN and RETI. Rust: only RETN restores. |
| **Mixed-mode CALL/RET/RST** | CEmu pushes/pops MADL\|ADL flag byte for cross-mode transitions. Rust: not implemented at all. |
| **ED x=0 z=7 p=3: rp3 mapping** | CEmu sets PREFIX=2, making rp3[3]=IX. Rust maps p=3 to IY. **Wrong register.** |
| **EX DE,HL: no L-mode masking** | CEmu masks both registers in Z80 mode. Rust does simple swap. |
| **Block BC decrement** | CEmu uses partial mode write (preserves BCU). Rust masks entire register (`& 0xFFFF`), clearing upper byte in Z80 mode. |
| **JP nn: ADL source** | CEmu: `ADL = L`. Rust: `ADL = IL`. Only matters with suffix opcodes. |
| **LD A,I / LD A,R: PV flag** | CEmu uses IEF1. Rust uses IFF2. Differs after NMI. |
| **DAA half-carry** | Different computation methods; edge case results may differ. |
| **SBC HL,rp carry** | Rust uses `hl < rp + c` which can overflow when `rp + c` wraps. |

### 2.4 Cycle Counting

| Issue | Detail |
|-------|--------|
| **HALT** | CEmu consumes all remaining cycles until next event. Rust adds 4 per step. |
| **RET cc taken: R += 2** | CEmu increments R by 2. Rust does not. |
| **PUSH rp2: R adjustment** | CEmu adds R += 2 when !PREFIX && L. Rust omits. |
| **Interrupt prefetch_discard** | CEmu does prefetch_discard (adds memory timing) before taking interrupt when not halted. Rust omits. |
| **Interrupt L/IL setup** | CEmu sets `L = IL = ADL \|\| MADL` before dispatch. Rust does not. |

---

## 3. Bus & Memory

**Files compared**: `bus.rs`, `memory.rs` vs `bus.c`, `bus.h`, `mem.c`, `mem.h`, `port.c`, `port.h`

### 3.1 Address Decoding

| Range | CEmu | Rust | Issue |
|-------|------|------|-------|
| 0x400000-0xBFFFFF | Routes through `mem_read_flash()` (cases 0x4-0xB) | `MemoryRegion::Unmapped` | **Wrong** — misses serial flash cache touch on writes |
| 0xC00000-0xCFFFFF | `mem_read_unmapped_other()` with 258 cycles | Part of Unmapped | Missing specific timing |
| 0xE40000-0xEFFFFF | Unmapped MMIO (not routed to ports) | Routed to port handlers | **Wrong** |
| 0xFB0000-0xFEFFFF | Unmapped MMIO, 3-cycle penalty | Routed to port handlers | **Wrong** |
| RAM masking | `addr & 0x7FFFF` (512KB window) | `(addr - RAM_START) % RAM_SIZE` | Functionally equivalent |

### 3.2 Memory Protection — Entirely Missing

| Feature | CEmu | Rust |
|---------|------|------|
| Stack limit NMI | `addr == control.stackLimit` → NMI | Not implemented |
| Protected memory region | `protectedStart..protectedEnd` → NMI for unprivileged writes, returns 0 for reads | Not implemented |
| Flash write from unprivileged code | Triggers NMI | Not checked |
| I/O from unprivileged code | IN returns 0, OUT triggers NMI | Not checked |

### 3.3 Flash Memory

| Feature | CEmu | Rust | Status |
|---------|------|------|--------|
| Program byte (AA/55/A0/data) | Implemented | Implemented | Match |
| Sector erase (AA/55/80/AA/55/30) | Implemented | Implemented | Match |
| Chip erase | Implemented | **Missing** | |
| CFI read (0x98) | Implemented | **Missing** | |
| Sector protection verify (0x90) | Implemented | **Missing** | |
| Deep power down (0xB9) | Implemented | **Missing** | |
| IPB/DPB modes | Implemented | **Missing** | |
| Per-sector protection (IPB/DPB) | Full sector protection checking | No checks | **Missing** |
| Flash bounds checking (flash.mask) | Checks against mappedBytes | Always reads | **Missing** |
| `flashDelayCycles` tracking | Retroactive adjustment on wait state change | Not tracked | **Missing** |

### 3.4 Other Bus Issues

- **RNG algorithm**: CEmu uses 4-variable XOR/add. Rust uses 3-byte LFSR. Different sequences for unmapped reads.
- **Port mirror masks**: Several ranges differ (timers 0xFF vs 0x7F, RTC 0x7F vs 0xFF, flash in serial mode 0xFFF vs 0xFF).
- **No `sched_process_pending_events()` before port access**: Stale peripheral state can be returned.
- **Separate `cycles`/`mem_cycles`** counters vs CEmu's single `cpu.cycles`.
- **Dead code**: `UNMAPPED_MMIO_PROTECTED_CYCLES` and `UNMAPPED_MMIO_OTHER_CYCLES` constants are defined but never used.
- **DMA system completely missing**: No `sched_process_pending_dma()`, LCD/USB DMA cannot steal CPU cycles.

---

## 4. LCD / Backlight / SPI

**Files compared**: `peripherals/lcd.rs`, `peripherals/backlight.rs`, `peripherals/spi.rs` vs `lcd.c/h`, `backlight.c/h`, `spi.c/h`, `panel.c/h`

### 4.1 LCD Controller

| Feature | CEmu | Rust | Severity |
|---------|------|------|----------|
| Color modes 0-5, 7 (1/2/4/8/24/12bpp) | All 8 modes | Only mode 6 (RGB565) | **High** |
| 256-entry color palette | Full, at 0x200-0x3FF | Not implemented | **High** |
| BGR/RGB swap (control bit 8) | Implemented | Hardcoded RGB | Medium |
| BEBO/BEPO byte ordering | Implemented | Not implemented | Low |
| LCD DMA engine | Watermark-based FIFO, CLOCK_48M | Not implemented | Medium |
| Timing state machine | 5-state (SYNC/BACK_PORCH/ACTIVE/FRONT_PORCH/LNBU) | Fixed 800K cycles/frame | Medium |
| Write delays (speed-dependent) | 8-21 cycles for control, 7-14 for cursor | None | Low |
| Hardware cursor | 32x32/64x64, compositing, palette | Not implemented | Medium |
| MIS register (0x24) | Masked interrupt status | **Missing** (offset used as PALBASE) | Medium |
| ICR register (0x28) | Interrupt clear | **Wrong** — treated as PALBASE | Medium |
| Interrupt sources | 4 (LNBU, compare, vblank, cursor) | Only VBLANK | Medium |
| UPBASE alignment | Forces 8-byte alignment | No enforcement | Low |
| Peripheral ID registers | Returns hardware ID bytes | Returns 0 | Low |

### 4.2 Panel Controller — Entirely Missing

CEmu has 1375 lines emulating the ST7789V display driver IC with:
- 80+ SPI commands, frame memory (320×240×3), gamma correction, color modes, scan direction
- The Rust emulator reads VRAM directly via `render_frame()`, bypassing all panel processing.

### 4.3 Backlight

| Feature | CEmu | Rust | Status |
|---------|------|------|--------|
| Read address indexing | `(pio >> 2) & 0xFF` (brightness at index 0x09) | Raw offset (brightness at 0x24) | Different but functionally OK |
| Reset values | 5 non-zero defaults (0x64, 0x64, 0x61, 0x4C, 0xFF) | Only brightness=0xFF | Missing |
| Gamma integration | Float factor, panel dirty flag | None | Missing |
| Port backing storage | 256-byte array | No storage | Missing |

### 4.4 SPI Controller

| Feature | CEmu | Rust | Severity |
|---------|------|------|----------|
| Device connection (panel/ARM) | `panel_spi_transfer()`, 9-bit frames | No device — RX always 0 | **High** |
| TX/RX FIFO data | 16×32-bit arrays with actual data | Counter only, no data | **High** |
| Bit-level transfer sim | Full frame/device bit shifting | Timer-based | **High** |
| CR0 read masking | Mode-dependent 16-bit masks | No masking | Low |
| INTSTATUS read side-effect | Clears bits 0-1 | No side effect | Medium |
| Threshold interrupts | TX/RX threshold → status bits 2-3 | Not implemented | Medium |
| Interrupt controller integration | `intrpt_set(INT_SPI, ...)` | Not connected | Medium |
| Loopback mode | CR0 bit 7 | Not implemented | Low |
| TX underflow detection | Status bit 1 | Not detected | Low |

---

## 5. Interrupt / Timer / Scheduler

**Files compared**: `peripherals/interrupt.rs`, `peripherals/timer.rs`, `scheduler.rs` vs `interrupt.c/h`, `timers.c/h`, `schedule.c/h`

### 5.1 Timer Register Layout — Completely Wrong

**CEmu** uses a shared register layout:
```
0x00-0x0F: Timer 0 (counter, reset, match0, match1)
0x10-0x1F: Timer 1
0x20-0x2F: Timer 2
0x30: Control (shared 32-bit, 3 bits per timer + 3 direction bits)
0x34: Status (shared, 3 bits per timer)
0x38: Mask
0x3C: Revision (0x00010801)
```

**Rust** uses per-timer control bytes at wrong addresses:
```
0x30: Timer 1 control (individual byte)
0x34: Timer 2 control (individual byte)
0x38: Timer 3 control (individual byte)
```

Missing: shared status register, mask register, revision register.

### 5.2 Timer Architecture

| Feature | CEmu | Rust | Status |
|---------|------|------|--------|
| Execution model | Scheduler-driven events, lazy counter reads | Per-cycle ticking via `tick()` | **Different** |
| Counter read accuracy | Computed from `sched_ticks_remaining()` | Returns last-updated value (potentially stale) | **Different** |
| Clock selection | CPU or 32kHz via control bit | CPU with prescaler divider | **Different** (no 32kHz option) |
| 2-cycle interrupt delay | `SCHED_TIMER_DELAY` event | Immediate | **Missing** |
| Counter write timing | +1 cycle delay | No delay | Missing |
| Timer status register | Readable/clearable at 0x34 | Not implemented | **Missing** |

### 5.3 OS Timer

| Issue | Detail |
|-------|--------|
| **Interrupt phase inverted** | CEmu: sets interrupt when transitioning true→false. Rust: raises when transitioning false→true. |
| **Never clears interrupt** | CEmu calls `intrpt_set(false)`. Rust only raises, never clears raw status. |
| **Speed index register** | CEmu uses `control.ports[0]` (port 0x00). Rust uses port 0x01 (cpu speed). Potentially wrong register. |
| **One state change per tick** | Rust processes max 1 transition per `tick()` call. Can fall behind with large cycle deltas. |

### 5.4 Scheduler

| Feature | CEmu | Rust | Status |
|---------|------|------|--------|
| Event types | 18 (including DMA) | 7 | Missing 11 event types |
| SCHED_SECOND (overflow prevention) | Normalizes timestamps every second | No normalization | **Missing** |
| DMA events | Separate DMA scheduling | Not implemented | **Missing** |
| CPU speed change event conversion | Converts all affected timestamps | Only converts `cpu_cycles` counter | **BUG** |
| Clock rates | Panel=10MHz, CPU default=48MHz | Panel=60Hz, CPU default=6MHz | **Different** |
| Callback architecture | Function pointers per event | Poll-and-match in emu loop | Different |
| Optimized division | Precomputed reciprocals | Simple u64 division | Slower |

---

## 6. Control Ports / Flash Controller

**Files compared**: `peripherals/control.rs`, `peripherals/flash.rs` vs `control.c/h`, `flash.c/h`

### 6.1 Control Port Bugs

| Port | Issue | Severity |
|------|-------|----------|
| **0x01** (CPU speed) | Write mask 0x03 vs CEmu's 0x13 (bit 4 lost) | Bug |
| **0x29** (general) | No write mask vs CEmu's `& 1` | Bug |
| **0x02** (battery read) | Hardcoded 0 vs CEmu's FSM state | Missing |
| **0x0D** (LCD enable) | No side effects (`lcd_disable()`, VRAM corruption, `lcd_update()`) | Missing |
| **0x09** (panel) | No panel HW reset, SPI select, sleep detection | Missing |
| **0x3D/0x3E** (protection) | Not implemented | Missing |
| Default ports | Reads return 0 vs CEmu's 128-byte backing array | Different |

### 6.2 Battery FSM — Completely Missing

CEmu has a multi-state FSM across ports 0x00, 0x07, 0x09, 0x0A, 0x0C that probes battery voltage. The Rust implementation stores register values but has no FSM transitions. Port 0x02 always returns 0.

### 6.3 Flash Controller

| Feature | CEmu | Rust | Status |
|---------|------|------|--------|
| Parallel flash registers | Full | Implemented | Mostly matches |
| Serial flash (command-based) | Full (20+ commands, erase/program/status/IDs) | **Not implemented** | Missing |
| Flash write protection | `flash_unlocked()` check | Not checked | Missing |
| `size_config` reset value | 0x00 (memset) | 0x07 | **Different** |
| Wait states type | u32 (up to 261) | u8 with saturation (caps 255) | Different |
| `flash.mask` override at 0 wait states | Implemented | Missing | Missing |

---

## 7. Keypad / RTC / Watchdog

**Files compared**: `peripherals/keypad.rs`, `peripherals/rtc.rs`, `peripherals/watchdog.rs` vs `keypad.c/h`, `realclock.c/h`, `misc.c/h`

### 7.1 Keypad — Control Register Packing (Critical)

CEmu packs mode (2 bits) + rowWait (14 bits) + scanWait (16 bits) into a single 32-bit register at offset 0x00. Rust treats these as separate registers at offsets 0x00, 0x30, 0x34. **The ROM writes these as part of the 32-bit control register, so Rust never receives correct scan timing values.**

Other keypad issues:
- Data register count: 16 (CEmu) vs 8 (Rust)
- GPIO enable register: missing
- Key ghosting simulation: missing
- ON key wake/interrupt handling: missing
- Scan clock: 6MHz fixed (CEmu) vs CPU-speed-dependent (Rust)
- Reset mask: 0xFFFF (CEmu) vs 0x00FF (Rust)
- Enable mask: `& 0x07` (CEmu) vs no masking (Rust)

### 7.2 RTC — Completely Stubbed (Critical)

| Feature | CEmu | Rust |
|---------|------|------|
| Time counting (sec→min→hour→day) | Full with rollover | **`tick()` is no-op** |
| State machine | 3 states (TICK/LATCH/LOAD_LATCH) | 2 states (Latch/LoadTick) |
| Load operation | Bit-level transfer from load→counter | `advance_load()` transfers no data |
| Interrupts (6 types) | Sec/min/hour/day/alarm/load-latch | None generated |
| Alarm | Full match checking | Stub (ignored) |
| Latch | Copies counter→latched on event | No latching |
| Load registers | Readable/writable | Writes ignored, reads 0 |

### 7.3 Watchdog — Wrong Register Offsets (Critical)

| Function | CEmu Offset | Rust Offset | Match? |
|----------|-------------|-------------|--------|
| Current counter | 0x00-0x03 | 0x04-0x07 | **NO** |
| Load value | 0x04-0x07 | 0x00-0x03 | **NO** |
| Restart/Feed (write 0xB9) | 0x08 | Not implemented | **MISSING** |
| Control | 0x0C | 0x08 | **NO** |
| Status clear | 0x14-0x17 | 0x0C | **NO** |
| Revision | 0x1C (value 0x00010602) | 0xFC (value 0x00000500) | **NO** |
| Reset load value | 0x03EF1480 | 0xFFFFFFFF | **WRONG** |

Additionally: no countdown, no NMI/reset generation, no pulse counter, no clock source selection. Rust has a lock register (0xC0) that doesn't exist in CEmu.

---

## 8. SHA256 / Peripheral Hub

**Files compared**: `peripherals/sha256.rs`, `peripherals/mod.rs` vs `sha256.c/h`, `asic.c/h`

### 8.1 SHA256 — No Hash Computation

| Issue | Detail |
|-------|--------|
| `process_block()` | Not implemented — comment says "just accept the writes" |
| Control register decode | Uses `else if` instead of independent `if`s — 0x0A only initializes, never processes |
| Protected port gating | Missing — reads/writes bypass `protected_ports_unlocked()` check |
| Flash unlock check | Missing — control writes don't check `flash_unlocked()` |
| Reset state | Rust initializes to SHA-256 IV. CEmu zeros everything. |

### 8.2 Peripheral Hub Issues

| Issue | Detail |
|-------|--------|
| **Port range 0xF mapped to Control** | Should be fxxx handler (debug port). Control is only via MMIO at 0xFF0000. |
| **SPI not in Peripherals::read/write** | Memory-mapped SPI access (0xED0000) broken — only IN/OUT works. |
| **Backlight not routed in mod.rs** | Constants marked `#[allow(dead_code)]`; falls to fallback. |
| **Missing INT_PWR on reset** | CEmu sets `intrpt_set(INT_PWR, true)` after interrupt reset. |
| Port mirror masks | 6+ ranges differ between `bus.rs` and CEmu's `port_mirrors` table. |

### 8.3 Missing Peripherals

| Peripheral | CEmu | Rust |
|------------|------|------|
| USB controller | Full (`usb/`) | Missing |
| UART | `uart.c/h` | Missing |
| Panel controller | `panel.c/h` (1375 lines) | Missing |
| Protected ports (0x9xxx) | `misc.c` with lock gating | Missing |
| Cxxx ports | `misc.c` byte storage | Missing |
| Fxxx ports | `misc.c` debug output | Missing |

---

## 9. Emu Loop / Lib / Disasm / WASM

**Files compared**: `emu.rs`, `lib.rs`, `disasm.rs`, `wasm.rs` vs `emu.c/h`, `asic.c/h`, `defines.h`, `cemu.h`

### 9.1 Emulation Loop

| Aspect | CEmu | Rust | Status |
|--------|------|------|--------|
| Frame timing | `SCHED_RUN` event on `CLOCK_RUN` (60Hz) | Raw cycle budget from caller | Different |
| Signal system | Bitmask: EXIT, RESET, ON_KEY, ANY_KEY | Individual booleans | Different |
| Event processing order | Signals → scheduler → cpu_execute | cpu.step → scheduler → peripheral tick | **Inverted** |
| Per-instruction peripheral tick | Not done (scheduler-driven) | `bus.ports.tick(cycles)` after every instruction | Extra work |
| Frame rendering | Push-based (LCD DMA → framebuffer) | Pull-based (read VRAM on demand) | Different |

### 9.2 Initialization

| Feature | CEmu | Rust |
|---------|------|------|
| Device type detection from ROM certificate | Full (TI-84 PCE / TI-83 PCE / TI-82 AEP) | None |
| ASIC revision (Pre-A / Rev I / Rev M) | Auto-detected from boot version | None |
| Python edition support | Detected from certificate | None |
| Boot version parsing | Full | None |
| Random bus initialization | `bus_init_rand(rand(), rand(), rand())` | Deterministic |
| Reset order | 17-step ordered sequence | Different order |

### 9.3 Disassembler Issues

- **ADD IX/IY,BC/DE/SP** (DD 09/19/39, FD 09/19/39): Not disassembled, show as raw bytes.
- **FD 37** (`LD IY,(IY+d)`): Missing (DD 37 exists but FD 37 doesn't).
- ALU mnemonic formatting inconsistency (`ADD A, B` with extra space vs `SUB B`).

### 9.4 C FFI Safety

`emu_framebuffer()` in `lib.rs` returns a raw pointer to the framebuffer, but the MutexGuard is dropped when the function returns — **use-after-free risk**.

### 9.5 WASM

- ARGB→RGBA conversion allocates a new Vec each frame (307KB at 60fps).
- No zero-copy rendering (CEmu WASM build exposes heap pointers).
- No `send_key()` / `send_letter_key()` high-level APIs exposed.

---

## 10. Full Issue Tracker

### Critical (20 issues)

| # | Component | Issue |
|---|-----------|-------|
| 1 | CPU | Block I/O (INI/IND/OUTI/OUTD/INIR/INDR/OTIR/OTDR) stubbed — no side effects |
| 2 | CPU | Missing LD I,HL (ED C7), LD HL,I (ED D7) |
| 3 | CPU | Missing LEA IY,IX+d (ED 55) |
| 4 | CPU | Missing block I/O: INI2/IND2/OUTI2/OUTD2 + repeat variants, INIRX/INDRX/OTIRX/OTDRX |
| 5 | CPU | RETI does not restore IFF1 from IFF2 |
| 6 | CPU | Mixed-mode CALL/RET/RST not implemented (no MADL\|ADL flag byte) |
| 7 | CPU | Single SP instead of separate SPS/SPL |
| 8 | CPU | ED x=0 z=7 p=3: rp3[3] maps to IY instead of IX |
| 9 | Timer | Register layout wrong: shared control/status/mask/revision vs per-timer bytes |
| 10 | Timer | Missing shared status register (0x34), mask (0x38), revision (0x3C) |
| 11 | Keypad | Control register packing wrong — rowWait/scanWait at wrong offsets |
| 12 | Watchdog | Nearly every register offset incorrect |
| 13 | RTC | tick() is no-op — no time counting, no interrupts |
| 14 | RTC | Load operation transfers no data |
| 15 | SHA256 | No `process_block()` — hash computation never happens |
| 16 | Bus | Flash region 0x400000-0xBFFFFF treated as Unmapped instead of flash |
| 17 | Bus | Memory protection entirely missing (stack limit NMI, protected region, unprivileged I/O) |
| 18 | Bus | MMIO unmapped holes (0xE40000-0xEFFFFF, 0xFB0000-0xFEFFFF) routed to ports |
| 19 | LCD | Only RGB565 supported — no palette modes, no 12/24bpp |
| 20 | SPI | No device connection — panel communication impossible |

### High (19 issues)

| # | Component | Issue |
|---|-----------|-------|
| 21 | CPU | EX DE,HL no L-mode masking |
| 22 | CPU | Block instruction BC decrement clears upper byte in Z80 mode |
| 23 | Scheduler | No SCHED_SECOND overflow prevention |
| 24 | Scheduler | CPU speed change doesn't convert event timestamps |
| 25 | Scheduler | Panel clock 60Hz vs CEmu's 10MHz |
| 26 | Scheduler | No DMA scheduling |
| 27 | OS Timer | Interrupt phase inverted from CEmu |
| 28 | OS Timer | Never clears interrupt (no `intrpt_set(false)`) |
| 29 | Timer | No 32kHz clock source option |
| 30 | Timer | 2-cycle interrupt delay missing |
| 31 | Control | Port 0x01 write mask 0x03 vs 0x13 (bit 4 lost) |
| 32 | Control | Port 0x29 no write mask |
| 33 | Flash | Serial flash controller entirely missing (20+ commands) |
| 34 | Flash | size_config reset value 0x07 vs 0x00 |
| 35 | LCD | ICR register (0x28) treated as PALBASE instead of interrupt clear |
| 36 | LCD | DMA engine not implemented |
| 37 | SHA256 | Control decode logic uses `else if` instead of independent `if` |
| 38 | Hub | Port range 0xF mapped to Control instead of fxxx handler |
| 39 | Hub | SPI not routed in memory-mapped path (mod.rs) |

### Medium (25+ issues)

Keypad ghosting, ON key wake, scan clock base, various missing write side effects (port 0x0D LCD disable/VRAM corruption, port 0x09 panel HW reset), battery FSM, SPI threshold interrupts/INTSTATUS read-clear/interrupt controller integration, LCD hardware cursor/timing state machine/write delays, backlight gamma integration, RTC alarm, multiple missing port mirror masks, R register scheme, various reset value differences, disassembler gaps, WASM allocation overhead, C FFI framebuffer safety.
