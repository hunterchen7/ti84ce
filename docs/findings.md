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

### IN/OUT (C) Uses Full BC as Port Address

On eZ80, `IN r,(C)` and `OUT (C),r` use the **full BC register pair** as a 16-bit port address:

```
// Standard Z80: port = C only (or 0xFF00 | C in some docs)
// eZ80: port = BC (full 16-bit value)
```

**Impact**: Using only C for port address causes wrong peripheral routing.

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

- RTC: Return fixed time values
- Watchdog: Accept writes, never trigger
- SPI: Return "TX not full" status

The ROM usually just checks that peripherals exist and have sane values.

---

_Last updated: During Milestone 5 boot debugging_
