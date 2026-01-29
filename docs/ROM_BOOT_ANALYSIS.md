# ROM Boot Analysis: Code Copy to RAM

This document analyzes how the TI-84 Plus CE ROM copies code to RAM during boot, what peripherals are involved, and what our emulator might be missing for Milestone 5e (visible OS screen).

## Executive Summary

The ROM boot sequence relies on several key mechanisms to initialize RAM:

1. **Block copy instructions** (LDIR/LDDR) - Already working, 3.2M+ instruction parity achieved
2. **Flash memory access** - Working with command emulation
3. **Memory protection** - Partially implemented (disabled during boot)
4. **SHA256 accelerator** - Not wired up in port_read/port_write (potential issue)
5. **DMA scheduling** - Not implemented (CEmu uses scheduler-based DMA)

The current bottleneck is likely **scheduler-dependent timing** (RTC load status causing divergence at 3.2M steps) rather than a missing code copy mechanism.

## CEmu Memory Architecture

### Address Space (from CEmu mem.c)

```
0x000000 - 0x3FFFFF : Flash (4MB)
0x400000 - 0xCFFFFF : Unmapped
0xD00000 - 0xD657FF : RAM (256KB + VRAM = 0x65800 bytes)
0xD65800 - 0xDFFFFF : Unmapped RAM region
0xE00000 - 0xFFFFFF : Memory-mapped I/O
```

### RAM Size

CEmu defines `SIZE_RAM = 0x65800` (415,744 bytes), which includes:
- User RAM: ~256KB (0x40000)
- VRAM: ~150KB (0x25800) starting at 0xD40000

Our implementation matches this exactly.

## ROM Code Copy Mechanism

### How Code Gets to RAM

The TI-84 CE ROM uses standard eZ80 block transfer instructions to copy code from flash to RAM:

1. **LDIR (Load, Increment, Repeat)** - Primary mechanism for bulk copies
2. **LDDR (Load, Decrement, Repeat)** - Reverse direction copies
3. **Direct LD instructions** - For small/scattered initialization

These instructions work correctly in our emulator (verified via 3.2M+ step trace comparison).

### Boot Sequence Flow

1. **Initial Hardware Setup** (PC 0x000000 - ~0x001000)
   - Configure control ports via OUT0 instructions
   - Set CPU speed, flash wait states
   - Initialize memory protection boundaries

2. **Memory Protection Configuration** (control.c ports 0x1D-0x25)
   - Set privileged boundary (0x1D-0x1F) - controls which code can access flash
   - Set protected memory range (0x20-0x25) - prevents user code from modifying OS

3. **Flash Controller Setup** (flash.c ports 0xE10000)
   - Enable flash (port 0x00)
   - Configure wait states (port 0x05)
   - Set memory mapping (port 0x02)

4. **RAM Initialization** (using LDIR/block instructions)
   - Zero out critical RAM regions
   - Copy OS code from flash to RAM
   - Set up interrupt vectors

5. **VRAM Initialization**
   - Clear VRAM to background color
   - ROM currently fills with white (0xFF)

6. **LCD Enable**
   - Configure LCD controller at 0xE30000
   - Set VRAM base address
   - Enable display

## CEmu Flash Memory Implementation

### Parallel Flash (Standard Mode)

From CEmu's `mem.c`, flash access follows AMD/Spansion NOR flash command sequences:

```c
// Write sequence for program operation
0xAAA <- 0xAA
0x555 <- 0x55
0xAAA <- 0xA0
addr <- data  // Actual byte program

// Write sequence for sector erase
0xAAA <- 0xAA
0x555 <- 0x55
0xAAA <- 0x80
0xAAA <- 0xAA
0x555 <- 0x55
addr <- 0x30  // Sector erase at addr
```

Our implementation handles these sequences correctly.

### Flash Unlock Detection

CEmu detects a specific instruction sequence in fetched bytes to unlock flash writes:

```c
static const uint8_t flash_unlock_sequence[] = {
    0xF3, 0x18, 0x00,       // DI; JR 0
    0xF3, 0xF3,             // DI, DI (double)
    0xED, 0x7E,             // IM 2
    0xED, 0x56,             // IM 1
    0xED, 0x39, 0x28,       // OUT0 (0x28), A
    0xED, 0x38, 0x28,       // IN0 A, (0x28)
    0xCB, 0x57              // BIT 2, A - triggers unlock
};
```

Our implementation handles both single-DI and double-DI variants.

## CEmu Memory Protection

### Privilege Model (control.c)

CEmu implements a simple privilege model:

1. **Privileged Boundary** (ports 0x1D-0x1F): 24-bit address
   - Code with `PC > privileged` is considered unprivileged
   - Default: 0xFFFFFF (all code privileged at boot)

2. **Protected Range** (ports 0x20-0x25): Start and end addresses
   - Unprivileged code cannot write to this range
   - Default: 0xD1887C-0xD1887C (empty range at boot)

3. **Stack Limit** (ports 0x3A-0x3C): NMI triggered on write to this address

### Protection Enforcement

```c
// From CEmu mem.c mem_write_cpu()
if (addr >= control.protectedStart && addr <= control.protectedEnd
    && unprivileged_code()) {
    control.protectionStatus |= 2;
    cpu_nmi();  // Trigger NMI on protection violation
}
```

**Our Status**: We have the protection registers but don't enforce them during writes. This is intentional for early boot testing but might need to be enabled later.

## SHA256 Accelerator

### CEmu Implementation (sha256.c)

The SHA256 accelerator at port 0x2xxx provides hardware-accelerated hashing:

- **0x00**: Control register (triggers operations)
- **0x0C**: Quick read of state[7]
- **0x10-0x4F**: Input block (64 bytes)
- **0x60-0x7F**: Output state (32 bytes)

### Protection Requirement

```c
// SHA256 reads/writes require protected ports unlocked
if (!poke) {
    if (protected_ports_unlocked()) {
        sha256.last = index;
    } else {
        index = sha256.last;  // Use last accessed index
    }
}
```

### Current Issue in Our Emulator

**SHA256 is NOT wired up in `bus.rs` port_read/port_write!**

Looking at `bus.rs` lines 758-760:
```rust
// Unimplemented: SHA256(2), USB(3), Protected(9),
// Backlight(B), Cxxx(C), UART(E)
_ => 0x00,
```

The SHA256 controller exists in `peripherals/sha256.rs` and is routed via memory-mapped I/O in `peripherals/mod.rs`, but I/O port access (via IN/OUT instructions to port 0x2xxx) returns 0x00 instead of calling the SHA256 controller.

**Recommendation**: Wire up SHA256 in port_read/port_write:
```rust
0x2 => {
    let offset = (port & 0xFF) as u32;
    self.ports.sha256.read(offset)
}
```

## DMA and Scheduler

### CEmu Scheduler (schedule.c)

CEmu uses a sophisticated timestamp-based scheduler for:
- Periodic events (LCD, timers, RTC)
- DMA transfers
- USB activity

### DMA Processing

```c
void sched_process_pending_dma(uint8_t duration) {
    // Called during memory accesses
    // Processes any pending DMA transfers
    // Adds DMA cycles to cpu.dmaCycles
}
```

**Our Status**: We don't have a scheduler-based DMA system. Memory accesses are direct.

### Impact on Boot

The RTC load status (port 0x8 offset 0x40) depends on scheduler ticks:
- CEmu: Returns 0xF8 (pending), then 0x00 after ~51 scheduler ticks
- Our emulator: Returns 0xF8 (pending) indefinitely

This causes divergence at step 3,216,456 when the ROM checks RTC load status.

## What's Working

1. **Memory Map**: Flash, RAM, VRAM at correct addresses ✓
2. **Block Instructions**: LDIR, LDDR execute all iterations atomically ✓
3. **Flash Commands**: Sector erase returns 0x80 status correctly ✓
4. **Control Ports**: All boot-critical ports implemented ✓
5. **Flash Unlock**: Sequence detection works ✓
6. **LCD Controller**: VRAM base, control registers working ✓
7. **3.2M+ Instruction Parity**: Trace matches CEmu exactly ✓

## What's Missing/Broken

### High Priority (Likely Boot Blockers)

1. **SHA256 Port I/O Not Wired** (port 0x2xxx via IN/OUT)
   - Memory-mapped access works
   - I/O port access returns 0x00
   - ROM may check SHA256 presence via ports

2. **RTC Scheduler Timing**
   - Load status stuck at 0xF8
   - Causes divergence at 3.2M steps
   - Would require full scheduler implementation

### Medium Priority (May Affect Later Boot)

3. **USB Controller** (port 0x3xxx)
   - Returns 0x00 on port reads
   - USB status bits in control port 0x0F are stubbed

4. **Protected Memory Enforcement**
   - Registers exist but writes not enforced
   - May cause issues when OS runs user code

### Low Priority (Probably Not Needed for Boot Screen)

5. **Full DMA System**
   - LCD DMA for framebuffer
   - SHA256 DMA for block operations

6. **Backlight Controller** (port 0xBxxx)
7. **UART Controller** (port 0xExxx)

## Recommended Implementation Steps

### Step 1: Wire SHA256 to Port I/O (Quick Fix)

Add SHA256 routing in `bus.rs` port_read/port_write:

```rust
0x2 => {
    let offset = (port & 0xFF) as u32;
    self.ports.sha256.read(offset)
}
```

This is a trivial fix that could unblock boot.

### Step 2: Investigate Post-3.2M Divergence

Run longer traces to see what happens after the RTC divergence:
- Does boot eventually continue?
- Is there a timeout/retry path?
- What specifically blocks progress?

### Step 3: Consider Scheduler Implementation (If Needed)

If RTC timing proves critical:
- Implement basic event scheduler
- Add RTC load status progression
- This is significant work but may be required

### Step 4: Verify VRAM Content

Add debugging to check:
- What's being written to VRAM?
- Is the framebuffer actually filled with OS graphics?
- Is LCD control properly configured?

## Debugging Commands

### Trace Comparison
```bash
# Our trace
cargo run --example trace_boot --manifest-path core/Cargo.toml > trace_ours.log

# CEmu trace (requires cemu-ref/ with trace_cli)
./cemu-ref/trace_cli > trace_cemu.log 2>&1
```

### RAM Write Analysis
```bash
# Enable write tracer in test code
bus.write_tracer.enable();
// ... run boot ...
println!("{}", bus.write_tracer.summary());
```

## References

- CEmu Source: `cemu-ref/core/`
  - `mem.c` - Memory access and flash commands
  - `flash.c` - Flash controller ports
  - `control.c` - Control ports and privilege
  - `sha256.c` - SHA256 accelerator
  - `schedule.c` - Event scheduler and DMA
- Our Implementation: `core/src/`
  - `bus.rs` - Memory bus and port routing
  - `memory.rs` - Flash and RAM
  - `peripherals/` - All peripheral controllers

---

*Analysis Date: 2025-01-29*
*Current Parity: 3,216,456+ instructions*
*Next Milestone: 5e - Visible OS Screen*
