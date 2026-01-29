# SPI Timing Parity Issue - Comprehensive Report

## Executive Summary

The TI-84 Plus CE emulator achieves **699,900 instruction-level PC matches** with CEmu (the reference emulator) before diverging. The divergence is caused by **incorrect SPI (Serial Peripheral Interface) transfer completion timing**.

At step 699,900, our emulator completes SPI transfers too fast, causing the ROM's SPI polling loop to exit prematurely with different A register values.

## Current State

| Metric | Value |
|--------|-------|
| Instructions matched | 699,900 out of 1,000,000 |
| First PC divergence | Step 699,900 |
| Root cause | SPI STATUS register returns different tfve (TX FIFO valid entries) |
| CEmu behavior at divergence | tfve=2 (2 pending transfers) |
| Our behavior at divergence | tfve=0 (all transfers complete) |

## The SPI Polling Loop (ROM Code at 0x005BA4-0x005BB7)

The ROM polls SPI STATUS to wait for transfers to complete:

```
005BA4: C5        PUSH BC
005BA5: 40        LD B,B          ; Copy B to itself (NOP-like)
005BA6: 01 xx xx  LD BC,xxxxD0    ; BC = 0xD0xxxx (SPI port address)
005BA9: ED 78     IN A,(C)        ; Read SPI STATUS byte 1 (contains tfve)
005BAB: E6 F0     AND 0xF0        ; Mask to get tfve bits
005BAD: 20 FA     JR NZ, 005BA9   ; Loop if tfve != 0 (transfers pending)
005BAF: 0D        DEC C           ; Move to next port byte
005BB0: ED 78     IN A,(C)        ; Read next status byte
005BB2: CB 57     BIT 2,A         ; Check transfer active bit
005BB4: 20 FA     JR NZ, 005BB0   ; Loop if transfer still active
005BB6: C1        POP BC
005BB7: C9        RET
```

The loop at 005BA9-005BAD polls the high nibble of STATUS byte 1 (tfve field) until it's zero.

## Divergence Analysis

### At Step 699,897 (Before SPI Read)
Both emulators at PC=005BA9, about to execute `IN A,(C)`:
- CEmu cycles: 16,004,469
- Our cycles: 13,139,205
- Both: A=0x08, registers match

### At Step 699,898 (After SPI Read)
Both emulators at PC=005BAB, just read SPI STATUS:
- **CEmu: A=0x20** → tfve=2 (2 transfers still pending)
- **Ours: A=0x00** → tfve=0 (all transfers complete)

### At Step 699,900 (Divergence)
- **CEmu: PC=005BA9** (loops back, still polling)
- **Ours: PC=005BAF** (exits loop, moves on)

## CEmu SPI Implementation (Reference)

CEmu uses a **scheduler-based event system** for SPI timing:

### Key Components (from `cemu-ref/core/spi.c`):

1. **Transfer scheduling**: Uses `sched_set(SCHED_SPI, ticks)` and `sched_repeat(SCHED_SPI, ticks)`
2. **Clock**: CLOCK_24M (24 MHz = 24,000,000 Hz)
3. **Transfer duration**: `bitCount * ((cr1 & 0xFFFF) + 1)` ticks at 24MHz

### STATUS Register (lines 182-185):
```c
case 0x0C >> 2: // STATUS
    value = spi.tfve << 12 | spi.rfve << 4 |
        (spi.transferBits != 0) << 2 |
        (spi.tfve != SPI_TXFIFO_DEPTH) << 1 | (spi.rfve == SPI_RXFIFO_DEPTH) << 0;
    break;
```

STATUS just reports current state - **no automatic completion**.

### Transfer Completion Flow:
1. `spi_write()` to DATA register → increments tfve, calls `spi_update()`
2. `spi_update()` → schedules `spi_event` after delay
3. `spi_event()` → decrements tfve via `spi_next_transfer()`, reschedules
4. Repeat until tfve=0

## Our Current Implementation (Broken)

File: `core/src/peripherals/spi.rs`

Our current approach **completes ALL transfers on first STATUS read**:

```rust
pub fn read(&mut self, addr: u32, _current_cycles: u64) -> u8 {
    // ...

    // STATUS register reads: complete ALL pending transfers
    // This gets us past the first SPI polling loop (step 418K)
    // and to step 699K where a different issue occurs
    if reg_idx == 3 && self.tfve > 0 {
        self.tfve = 0;           // ← WRONG: Instantly completes everything
        self.transfer_bits = 0;
    }

    // ...
}
```

This approach was a bandaid fix for step 418K but fails at step 699K.

## Why This Approach Fails

### Key Finding: Different Number of Queued Transfers

The two scenarios differ in how many DATA writes (transfers) are queued before polling:

| Step | DATA writes before poll | CEmu tfve on first poll | Required behavior |
|------|------------------------|------------------------|-------------------|
| 418K | **3 writes** | 0 | All 3 complete before poll |
| 699K | **6 writes** | 2 | Only 4 of 6 complete before poll |

At step 418K, 3 transfers are queued and all complete before the first STATUS poll.
At step 699K, 6 transfers are queued but only 4 complete - leaving 2 pending (tfve=2).

### Why the Difference Matters

CEmu's timing formula: `bitCount * ((cr1 & 0xFFFF) + 1)` ticks at 24MHz per transfer.

With the same bit rate, 6 transfers take twice as long as 3 transfers. The ROM doesn't wait longer before polling, so some transfers are still pending.

Our "complete all on first STATUS read" approach:
- Works at 418K because CEmu also shows 0 pending
- Fails at 699K because CEmu still has 2 pending

### The Real Solution

We need to track **when** each transfer was queued and complete them based on elapsed time, not instantly. The timing depends on:
1. Number of transfers queued (tfve)
2. Clock divider setting (CR1 bits 0-15)
3. Transfer bit count (CR1 bits 16-20)
4. Cycles elapsed since queueing

## Cycle Count Difference

At divergence point:
- CEmu cycles: 16,004,469
- Our cycles: 13,139,205
- Ratio: 1.218x (CEmu has more cycles)

This 22% difference exists because:
1. Our bus cycle counting was recently fixed but may still differ
2. Different memory access timing for flash vs RAM
3. Port I/O timing differences

## Required Fix

To match CEmu, we need **scheduler-based SPI timing**:

### Option 1: Cycle-Based Completion (Simpler)
Track cycles when transfers were queued and complete based on elapsed cycles:

```rust
pub struct SpiController {
    // ...
    transfer_queue_cycle: u64,  // When last transfer was queued
    cycles_per_transfer: u64,   // Calculated from CR1
}

pub fn read(&mut self, addr: u32, current_cycles: u64) -> u8 {
    // On STATUS read, complete transfers based on elapsed cycles
    if reg_idx == 3 && self.tfve > 0 {
        let elapsed = current_cycles - self.transfer_queue_cycle;
        let completed = (elapsed / self.cycles_per_transfer) as u8;
        self.tfve = self.tfve.saturating_sub(completed);
    }
    // ...
}
```

**Challenge**: Cycle ratio (1.218x) means our cycles don't map 1:1 to CEmu cycles.

### Option 2: Event/Scheduler System (More Accurate)
Implement a minimal scheduler like CEmu:

```rust
pub struct SpiController {
    // ...
    event_cycles: Option<u64>,  // When next transfer completes
}

pub fn update(&mut self, current_cycles: u64) {
    if let Some(event_time) = self.event_cycles {
        if current_cycles >= event_time {
            // Complete one transfer
            if self.tfve > 0 {
                self.tfve -= 1;
            }
            // Schedule next or clear
            if self.tfve > 0 {
                self.event_cycles = Some(current_cycles + self.cycles_per_transfer());
            } else {
                self.event_cycles = None;
            }
        }
    }
}
```

**Challenge**: Need to call `update()` before every SPI access and potentially from the main loop.

### Option 3: Access-Count Based (User Preferred)
The user requested non-cycle-dependent timing. Track SPI accesses instead:

```rust
pub fn read(&mut self, addr: u32, _current_cycles: u64) -> u8 {
    self.access_count += 1;

    if reg_idx == 3 && self.tfve > 0 {
        // Complete N transfers per M accesses
        // Need to tune N and M to match both step 418K and 699K
    }
    // ...
}
```

**Challenge**: Finding N and M values that work for both scenarios may be impossible since they have opposite requirements.

## Key Files

| File | Purpose |
|------|---------|
| `core/src/peripherals/spi.rs` | Our SPI implementation (needs fixing) |
| `core/src/peripherals/mod.rs` | Peripheral routing, calls SPI read/write |
| `core/src/bus.rs` | Bus cycle counting, port I/O routing |
| `cemu-ref/core/spi.c` | CEmu reference implementation |
| `cemu-ref/core/schedule.c` | CEmu scheduler (events/timing) |

## Test Commands

Generate new trace:
```bash
cd core && cargo run --release --example compare_trace 1000000
```

Compare traces:
```bash
python3 -c "
cemu = [line.split() for line in open('traces/cemu_20260129_100334.log')]
ours = [line.split() for line in open('traces/ours_YYYYMMDD_HHMMSS.log')]
for i in range(min(len(cemu), len(ours))):
    if cemu[i][2] != ours[i][2]:  # Compare PC
        print(f'Divergence at step {i}: CEmu PC={cemu[i][2]}, Ours PC={ours[i][2]}')
        break
"
```

## Constraints

1. **User preference**: Avoid cycle-dependent timing if possible, or keep it localized
2. **No magic numbers**: Solutions should be justified, not tuned empirically
3. **Must pass both scenarios**: Step 418K (fast completion) AND step 699K (gradual completion)

## Questions for Implementation

1. What determines the difference between step 418K (immediate completion) and step 699K (gradual completion)? Is it the number of queued transfers? The CR1 clock divider setting?

2. Can we infer timing from register values (CR1) rather than tracking actual cycles?

3. Should we implement a minimal event scheduler, or can we approximate with access counting?

## Appendix: SPI Register Map

| Offset | Name | Description |
|--------|------|-------------|
| 0x00 | CR0 | Control register 0 |
| 0x04 | CR1 | Control register 1 (clock divider in bits 0-15) |
| 0x08 | CR2 | Control register 2 (enable, FIFO reset) |
| 0x0C | STATUS | Status (tfve<<12, rfve<<4, active<<2, tx_not_full<<1, rx_full<<0) |
| 0x10 | INTCTRL | Interrupt control |
| 0x14 | INTSTATUS | Interrupt status |
| 0x18 | DATA | TX/RX FIFO data |
| 0x1C | FEATURE | Feature flags |
| 0x60 | REVISION | Revision ID |
| 0x64 | FEATURE2 | Feature flags 2 |

## Appendix: Trace Format

```
step cycles PC SP AF BC DE HL IX IY ADL IFF1 IFF2 IM HALT opcode
```

Example:
```
699897 13139205 005BA9 D1A86C 0842 00D00D D65800 D657FF 000000 D00080 1 0 0 Mode2 0 ED78
```
