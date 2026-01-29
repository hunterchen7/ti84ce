# Findings

Interesting discoveries made during TI-84 Plus CE emulator development. These come from comparing execution traces with CEmu, examining ROM behavior, and general implementation work.

## eZ80 CPU

### IM Instruction Encoding Difference from Standard Z80

The eZ80 maps the `y` value from IM instructions directly to the interrupt mode, unlike standard Z80:

| Opcode      | Z80 Behavior | eZ80 Behavior        |
| ----------- | ------------ | -------------------- |
| ED 46 (y=0) | IM 0         | IM 0                 |
| ED 56 (y=2) | **IM 1**     | **IM 2**             |
| ED 5E (y=3) | IM 2         | IM 3 (eZ80-specific) |

**Impact**: The ROM executes `ED 56` expecting to set interrupt mode 2, not mode 1. Getting this wrong causes interrupt handling to fail completely.

**Source**: CEmu's cpu.c sets `cpu.IM = context.y` directly.

### IM 2 Mode Is NOT Standard Vectored Interrupts

On the TI-84 CE (eZ80), IM 2 does **not** behave like standard Z80 IM 2 (vectored interrupts using I register). Instead, it behaves the same as IM 1:

| Mode | Standard Z80 Behavior | TI-84 CE (eZ80) Behavior |
| ---- | --------------------- | ------------------------ |
| IM 0 | Execute instruction on data bus | Not typically used |
| IM 1 | Jump to 0x0038 | Jump to 0x0038 |
| IM 2 | Vector = (I << 8) \| data_bus | **Jump to 0x0038** (same as IM 1!) |
| IM 3 | N/A | Vectored (with asic.im2 flag) |

CEmu's code (cpu.c line ~986):
```c
if (cpu.IM == 2) {
    cpu_interrupt(0x38);  // Fixed address, NOT vectored!
} else {
    if (asic.im2 && cpu.IM == 3) {
        // Only IM 3 with asic.im2 flag does vectored interrupts
        cpu_interrupt(cpu_read_word(r->I << 8 | (bus_rand() & 0xFF)));
    }
}
```

**Impact**: Implementing standard Z80 IM 2 vectored interrupts (reading handler address from I*256) causes the CPU to jump to garbage addresses since the vector table doesn't exist. The ROM expects IM 2 to simply jump to 0x38.

**Source**: CEmu's cpu.c interrupt handling code.

### Block Instructions Execute Atomically

On the eZ80 (and in CEmu), block instructions like LDIR, LDDR, CPIR, CPDR execute **all iterations** in a single instruction step. This differs from some Z80 implementations that process one iteration per step.

**Impact**: Trace comparisons will show different step counts if iterations are counted differently. Also affects cycle counting.

### BIT Preserves F3/F5 (Undocumented) Flags

For CB-prefixed BIT instructions, CEmu preserves F3/F5 from the **previous** F register instead of deriving them from the tested operand or address.

**Impact**: Setting F3/F5 from the operand causes a mismatch in loops that test bits (e.g., SPI status loops around 0x005BB0).

**Source**: CEmu `cpu.c` uses `cpuflag_undef(r->F)` in BIT handling.

### SBC/ADC HL,rr Preserves F3/F5

The 16/24-bit `SBC HL,rr` and `ADC HL,rr` instructions preserve F3/F5 from the previous F rather than using the high byte of the result.

**Impact**: Using result-derived F3/F5 causes flag divergence after ED 4A/52/5A/62 instructions.

**Source**: CEmu `cpu.c` uses `cpuflag_undef(r->F)` for these ops.

### Prefixed z=7 Opcodes Become LD rp3,(IX/IY+d)

With a DD/FD prefix, x=0 z=7 opcodes (normally RLCA/RRCA/RLA/RRA/DAA/CPL/SCF/CCF) become:

- `LD rp3[p],(IX/IY+d)` when q=0
- `LD (IX/IY+d),rp3[p]` when q=1

**Impact**: Treating these as normal rotate/DAA/CPL instructions causes PC and register divergence near 0x024020.

**Source**: CEmu `cpu.c` handles prefixed z=7 in the DD/FD path.

### DD/FD 0x31 Is NOT LD SP,nn

With DD/FD prefix, opcode 0x31 maps to `LD IY/IX,(IX/IY+d)` (uses a displacement) instead of `LD SP,nn`. Other prefixed `LD rr,nn` opcodes are opcode traps in CEmu.

**Impact**: Executing `DD 31` as `LD SP,nn` corrupts SP and PC around 0x024023.

**Source**: CEmu `cpu.c` prefixed LD rr,nn handling.

### ED 22/23 Are LEA (Load Effective Address)

ED 22/23 are eZ80-specific LEA instructions:

- `ED 22`: `LEA rp3[p],IX+d`
- `ED 23`: `LEA rp3[p],IY+d`

They consume a displacement byte and write the masked effective address (no MBASE) to rp3.

**Impact**: Treating ED 23 as NOP leaves HL unchanged and desynchronizes PC by one byte.

**Source**: CEmu `cpu.c` ED-prefix x=0 z=2/3 handling.

### IN/OUT (C) Uses Full BC as Port Address

On eZ80, `IN r,(C)` and `OUT (C),r` use the **full BC register pair** as a 16-bit port address:

```
// Standard Z80: port = C only (or 0xFF00 | C in some docs)
// eZ80: port = BC (full 16-bit value)
```

**Impact**: Using only C for port address causes wrong peripheral routing.

### DD/FD 3E d Is LD (IX/IY+d),IY/IX

With DD/FD prefix, opcode 0x3E (z=6, y=7) becomes `LD (IX+d),IY` or `LD (IY+d),IX`, not `LD A,n`:

- DD 3E d: `LD (IX+d),IY` (stores IY at IX+displacement)
- FD 3E d: `LD (IY+d),IX` (stores IX at IY+displacement)

**Impact**: Treating this as `LD A,n` loads the wrong value into A and misses the memory write, causing register divergence around step 1,187,224.

**Source**: CEmu `cpu.c` prefixed LD handling for y=7 case.

### ED z=4 Distinguishes NEG from MLT

For ED prefix with z=4, the instruction depends on q:

- q=0: Various instructions based on p (NEG for p=0, LEA IX,IY+d for p=1, etc.)
- q=1: MLT rp[p] (multiply high*low bytes, store 16-bit result)

| Opcode | q | p | Instruction |
| ------ | - | - | ----------- |
| ED 44  | 0 | 0 | NEG         |
| ED 4C  | 1 | 0 | MLT BC      |
| ED 54  | 0 | 1 | LEA IX,IY+d |
| ED 5C  | 1 | 1 | MLT DE      |
| ED 64  | 0 | 2 | TST A,n     |
| ED 6C  | 1 | 2 | MLT HL      |
| ED 74  | 0 | 3 | TSTIO n     |
| ED 7C  | 1 | 3 | MLT SP      |

**Impact**: Treating all z=4 as NEG breaks MLT instructions, causing register divergence around step 1,188,549.

**Source**: CEmu `cpu.c` ED x=1 z=4 handling.

### ED 6E (LD A,MB) - Critical Boot Instruction

The `LD A,MB` instruction (ED 6E) loads MBASE into the A register. This instruction is critical for boot:

1. ROM sets MBASE to 0xD0 during initialization
2. Interrupt handler executes `LD A,MB` then `CP 0xD0`
3. If MBASE check fails, handler enters infinite loop

Initially this was treated as NOP which caused boot to stall in the interrupt handler.

**Source**: Discovered via trace comparison when boot stalled after interrupt.

## Timers

### OS Timer (32KHz Crystal Timer)

The TI-84 Plus CE has **four** timer sources for interrupts, not three:

1. **Timer 1** (GPT) - General purpose, bit 1
2. **Timer 2** (GPT) - General purpose, bit 2
3. **Timer 3** (GPT) - General purpose, bit 3
4. **OS Timer** - 32KHz crystal-based, bit 4

The OS Timer is separate from the three memory-mapped GPTs. It:

- Runs at 32768 Hz (crystal oscillator frequency)
- Uses CPU speed bits to determine tick interval
- Toggles state and fires interrupt when state becomes true

**Tick intervals** (in 32KHz ticks) based on CPU speed (port 0x01 bits 0-1):

| Speed | Clock  | 32K Ticks | Approximate Hz |
| ----- | ------ | --------- | -------------- |
| 0     | 6 MHz  | 73        | ~449 Hz        |
| 1     | 12 MHz | 153       | ~214 Hz        |
| 2     | 24 MHz | 217       | ~151 Hz        |
| 3     | 48 MHz | 313       | ~105 Hz        |

**Impact**: The ROM enables OS Timer interrupt (bit 4) and enters a delay loop waiting for it. Without OS Timer implementation, boot stalls indefinitely.

**Source**: CEmu's timers.c `ost_ticks[4] = { 73, 153, 217, 313 }` and `ost_event()`.

## LCD Controller

### Register Offset Mapping

The LCD register layout differs from some ARM PrimeCell PL111 documentation:

| Offset | Register                    |
| ------ | --------------------------- |
| 0x00   | Timing 0 (horizontal)       |
| 0x04   | Timing 1 (vertical)         |
| 0x08   | Timing 2                    |
| 0x0C   | Timing 3                    |
| 0x10   | **Upper Panel Base (VRAM)** |
| 0x14   | Lower Panel Base            |
| 0x18   | **Control**                 |
| 0x1C   | Interrupt Mask              |
| 0x20   | Interrupt Status            |

**Impact**: Incorrect register mapping causes VRAM address to appear in control register reads, making LCD appear misconfigured.

**Source**: CEmu's lcd.c and cross-referencing with TI-84 CE ROM behavior.

## Control Ports

### CPU Speed Port (0x01) Nibble Behavior

Writing to port 0x01 duplicates the low nibble to the high nibble:

- Write 0x03 → stored as 0x33
- Read returns 0x33

**Source**: CEmu's control.c behavior.

### LCD Enable Port (0x0D) Nibble Duplication

Same nibble duplication behavior as CPU speed port.

### USB Control Port (0x0F) Masking

Writes are masked with 0x03 (only bits 0-1 are writable).

### USB Status Bits in Port 0x0F Reads

Port 0x0F reads OR in USB status bits from the OTGCSR register. At reset this yields **0xC0** (bits 7 and 6 set), so the ROM expects `IN0 A,(0x0F)` to return at least 0xC0 | control.ports[0x0F].

**Impact**: Returning only 0x02 causes a branch at ROM 0x000F69 to go the wrong way, diverging around step ~702,600.

**Source**: CEmu `core/control.c` uses `control.ports[index] | usb_status()`, and `usb_status()` sets 0x80/0x40 based on OTGCSR bits (see `core/usb/usb.c`).

### Control Flags Port (0x05) Masks to 0x1F

Writes to port 0x05 are masked with 0x1F; bits 5-7 are cleared on write in CEmu.

**Impact**: Leaving bit 5 set makes `IN0 A,(0x05)` return 0x20 too high and diverges after ED 38.

**Source**: CEmu `control.c` (`control.ports[index] = byte & 0x1F`).

## Interrupt Controller

### PWR Interrupt at Reset

The Power interrupt (bit 15) is set during hardware reset. This is part of the power-on sequence signaling.

**Source**: CEmu's interrupt.c initialization.

### Interrupt Source Mapping

| Bit | Source             |
| --- | ------------------ |
| 0   | ON Key             |
| 1   | Timer 1            |
| 2   | Timer 2            |
| 3   | Timer 3            |
| 4   | OS Timer           |
| 10  | Keypad (scan mode) |
| 11  | LCD (VBLANK)       |
| 15  | Power              |
| 19  | Wake               |

## Flash Controller

### Default Values at Reset

CEmu-compatible defaults for boot:

- Enable: 0x01 (enabled)
- Wait states: 0x04
- Map select: 0x06

Using incorrect defaults causes flash access timing issues.

## Flash Memory

### Sector Erase Command Status (DQ7)

The ROM issues the AMD-style erase sequence:
`AA 55 80 AA 55 30` (to 0x0AAA / 0x0555 / target). During sector erase, CEmu returns **0x80** for the first few reads from flash (DQ7 ready), then clears command state.

**Impact**: Ignoring flash writes causes reads to return ROM data (0x00) instead of 0x80, breaking the erase-poll loop around 0xD18C50.

**Source**: CEmu `mem.c` `FLASH_SECTOR_ERASE` read path returns 0x80 for 3 reads.

## SPI Controller

### FIFO Depth Is 16 (Not 4)

The SPI TX/RX FIFO depth is **16 entries**, matching CEmu's `SPI_TXFIFO_DEPTH`/`SPI_RXFIFO_DEPTH`.

**Impact**: Using depth 4 caps tfve too early and causes the ROM SPI polling loop to exit prematurely (PC diverges at ~699,900 steps).

**Source**: CEmu `core/spi.h` (`#define SPI_TXFIFO_DEPTH 16`, `SPI_RXFIFO_DEPTH 16`).

### CR0[11] (FLASH) Enables RX-Only Transfers

When RX is enabled (`CR2` bit 7) and `CR0` bit 11 is set, CEmu allows SPI transfers to continue **even with an empty TX FIFO**. This fills the RX FIFO and keeps the STATUS transfer-active bit set until RX FIFO nears full.

**Impact**: Without RX-only transfers, the ROM's second SPI polling loop (BIT 2 on STATUS byte 0) exits too early, diverging around step ~699,910.

**Source**: CEmu `core/spi.c` logic in `spi_next_transfer()` and ROM trace comparison.

## Boot Sequence

### HALT with DI Pattern

The ROM frequently uses HALT with interrupts disabled (DI). This is a power-saving pattern where:

1. CPU executes DI (disable interrupts)
2. CPU executes HALT
3. ON key press generates a wake signal (not a regular interrupt)
4. CPU resumes execution

The ON key has a special "wake" capability that can break out of HALT even with IFF1=0.

### MBASE Initialization

The ROM sets MBASE to 0xD0 early in boot. This means:

- PC values in Z80 mode are prefixed with 0xD0
- Address 0x0000 in Z80 mode maps to 0xD00000 (RAM base)

### Boot Progress Checkpoints

Approximate cycle counts at key boot stages (at 48MHz):

- ~10M cycles: Initial hardware configuration complete
- ~15M cycles: First HALT (waiting for ON key wake)
- ~20M cycles: After first wake, delay loops
- ~30M cycles: Second HALT in RAM code

### OS Timer Delay Loop Pattern

The ROM uses a delay loop at 0x5C45 that is a simple countdown, not an interrupt-driven wait:

```asm
5C4D: 11 xxxx  LD DE,xxxx    ; Load decrement value (small, e.g., 1)
5C51: 21 xxxx  LD HL,xxxx    ; Load starting count (large 24-bit value)
5C55: B7       OR A          ; Clear carry flag
5C56: ED 52    SBC HL,DE     ; HL = HL - DE - carry
5C58: 20 FB    JR NZ,$5C55   ; Loop until HL == 0
```

**Key Finding**: This is NOT waiting for an interrupt to break out - it's a pure countdown loop. With 24-bit registers in ADL mode, HL could start at millions, requiring millions of loop iterations. Each iteration is 3 instructions (~31 cycles), so a full countdown could take 50+ million cycles.

**Trace Comparison**: Both CEmu and our emulator correctly enter this loop around step 540. CEmu's per-instruction trace ran 200,000 steps still in the loop; its sparse snapshot trace shows the loop eventually exits after many more iterations. Our trace (40,000 steps) is simply not running long enough.

**Resolution**: The implementation is correct. The loop just needs to run to completion. The OS Timer interrupt being pending (IRQ_PEND=true) is normal - it was enabled earlier and is latched. The interrupt will be serviced later once the ROM executes EI after the delay loop completes.

## I/O Port Address Space

The eZ80 has a separate 16-bit I/O port address space. Port addresses are routed based on bits 15:12:

| Range  | Peripheral           |
| ------ | -------------------- |
| 0x0xxx | Control Ports        |
| 0x1xxx | Flash Controller     |
| 0x2xxx | SHA256               |
| 0x3xxx | USB                  |
| 0x4xxx | LCD Controller       |
| 0x5xxx | Interrupt Controller |
| 0x6xxx | Watchdog             |
| 0x7xxx | Timers               |
| 0x8xxx | RTC                  |
| 0x9xxx | Protected            |
| 0xAxxx | Keypad               |
| 0xBxxx | Backlight            |
| 0xCxxx | Reserved             |
| 0xDxxx | SPI                  |
| 0xExxx | UART                 |
| 0xFxxx | Control Ports (alt)  |

**Source**: CEmu's port.c port_map array.

## Implementation Notes

### Trace Comparison is Essential

The most effective debugging technique was capturing execution traces from both our emulator and CEmu, then diffing them to find the first divergence point. This revealed:

- IM instruction encoding issue (diverged at step 393)
- LCD register offset issue (control register showed VRAM address)
- Missing ED 6E instruction (LD A,MB)

### Peripheral Stubs are Often Sufficient

Many peripherals can be stubbed with safe return values during early boot:

- RTC: Return zero time values (CEmu initializes to 0)
- Watchdog: Accept writes, never trigger
- SPI: Return "TX not full" status

The ROM usually just checks that peripherals exist and have sane values.

## RTC (Real-Time Clock)

### Initialization Matches CEmu memset

CEmu initializes the RTC with `memset(&rtc, 0, sizeof rtc)`, meaning:

- Control register: 0 (not 0x81)
- All time registers: 0 (not arbitrary values)
- Load status: LOAD_TOTAL_TICKS (51, meaning complete)

**Impact**: Initializing control to 0x81 or time to non-zero causes IN A,(C) to read wrong values, diverging around step 1,188,774.

### Load Status Register (0x40) Behavior

The load status register at offset 0x40 reflects the state of an RTC load operation:

- **LOAD_PENDING (0xFF as i8 = -1)**: Returns 0xF8 (bits 3-7 set)
- **In progress (ticks 0-50)**: Returns bitmask based on which fields finished
- **Complete (ticks >= 51)**: Returns 0

When bit 6 of control is written, a load operation is triggered:
1. `loadTicksProcessed` is set to LOAD_PENDING (255)
2. On first scheduler tick, becomes 0 (actively loading)
3. Advances 1 tick per 32kHz cycle until reaching LOAD_TOTAL_TICKS

**Impact**: Without scheduler-based timing, the load status cannot match CEmu exactly. ~~For parity during early boot (first 3M+ instructions), keeping load status pending (0xF8) matches CEmu behavior.~~ **Update**: Scheduler now implemented - see below.

## Scheduler Architecture

### CEmu's 7.68 GHz Base Clock

CEmu uses a scheduler with a base clock rate of **7,680,000,000 Hz (7.68 GHz)**. This is the Least Common Multiple (LCM) of all hardware clocks, allowing efficient integer-only timing calculations.

| Clock | Rate | Base Ticks per Clock Tick |
|-------|------|---------------------------|
| CPU (48 MHz) | 48,000,000 Hz | 160 |
| SPI (24 MHz) | 24,000,000 Hz | 320 |
| CPU (6 MHz) | 6,000,000 Hz | 1,280 |
| RTC (32 kHz) | 32,768 Hz | 234,375 |

The formula: `base_ticks_per_tick = 7,680,000,000 / clock_rate`

**Why this matters**: Using a common base clock avoids floating-point arithmetic and rounding errors in timing calculations. All timing conversions are exact integer divisions.

### RTC Load Timing

The RTC load operation takes **~51 ticks at 32.768 kHz** to complete:
- Seconds: 9 ticks
- Minutes: 8 more ticks (17 total)
- Hours: 8 more ticks (25 total)
- Day: 16 more ticks (41 total)
- Finalization: 10 more ticks (51 total)

With the scheduler, events are scheduled in base ticks:
- 1 RTC tick = 234,375 base ticks
- Full load = 51 * 234,375 = 11,953,125 base ticks
- At 48 MHz: 11,953,125 / 160 = ~74,707 CPU cycles

**Impact**: Proper scheduler timing allows RTC loads to complete at the same CPU cycle as CEmu, extending trace parity beyond the previous 3.2M limit.

## Boot Success

### OS Boots to Home Screen

The emulator successfully boots the TI-84 CE OS to the home screen:

- **Steps**: 3,609,969 instructions
- **Cycles**: ~61.6M cycles at 48MHz
- **Final PC**: 0x085B7F (OS idle loop)
- **Screen**: "RAM Cleared" message with status bar

**VRAM Analysis:**
| Color | RGB565 | Percentage | Purpose |
|-------|--------|------------|---------|
| White | 0xFFFF | 88.1% | Background |
| Dark Green | 0x52AA | 10.8% | Status bar, UI |
| Black | 0x0000 | 0.9% | Text |
| Red | 0xF800 | 0.2% | Battery indicator |

**LCD State at Boot Complete:**
- Control: 0x0000092D (16bpp RGB565, power on, enabled)
- VRAM base: 0xD40000

### Scheduler Parity Not Required

A key discovery: **exact scheduler timing parity with CEmu is not required** for correct emulation.

The scheduler is an internal implementation detail. What matters for correctness:
1. Peripheral reads/writes return correct values at appropriate times
2. Interrupts fire when expected
3. Polling loops eventually complete

**Evidence**: Our RTC timing differs from CEmu (our seconds complete loading earlier), but the ROM handles this gracefully. The polling loop simply iterates a different number of times before proceeding. Both emulators reach the same end state.

**Implication**: Focus debugging efforts on functional correctness, not cycle-exact timing. Timing differences in polling loops don't indicate bugs if the loop eventually completes correctly.

### Boot Requires ON Key Wake

The ROM uses multiple DI + HALT sequences during boot, expecting the ON key to wake the CPU:

1. First HALT at ~PC 0x001414 (very early, power-on sequence)
2. Second HALT after RAM initialization
3. Final HALT at 0x085B7F (OS idle loop)

Without the ON key wake mechanism (separate from regular interrupts), boot stalls at the first HALT since IFF1=0.

---

_Last updated: 2026-01-29 - Boot complete! OS reaches home screen with "RAM Cleared" message_
