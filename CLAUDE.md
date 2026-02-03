# Claude Code Instructions

Project-specific guidelines for Claude Code when working on this TI-84 Plus CE emulator.

## Code Style

- When leaving functionality for future implementation, always add a `TODO:` comment explaining what needs to be done and which milestone it's planned for
  - Example: `// TODO: Wire up BusFault when Bus reports invalid memory access (Milestone 5+)`
- Keep TODO comments concise but include enough context to understand the task

## Testing

- In Z80 mode tests, remember to set `cpu.mbase = 0x00` when poking bytes at address 0, since the default MBASE (0xD0) causes fetches from 0xD00000
- Use minimal ROM buffers in tests - flash defaults to 0xFF, so only include the bytes actually needed
- **Always assert exact expected values** - When testing arithmetic, counters, or state transitions, calculate and assert the specific expected result, not just a range or property. Weak assertions like `assert!(x < 1000)` can pass for both correct and buggy implementations. Instead, trace through the expected behavior step-by-step and use `assert_eq!(x, 995)`.
- **Test boundary conditions** - When a function involves arithmetic or type limits, test edge cases like maximum values (e.g., `0xFF` for u8), overflow/underflow scenarios, and boundary transitions. These catch issues that typical values won't reveal.

## Workflow

- **Check findings.md before peripheral fixes** - Before attempting to fix or modify any peripheral emulation (keypad, LCD, timers, interrupts, etc.), ALWAYS read [docs/findings.md](docs/findings.md) first. Look for:
  - "What we tried that didn't work" sections documenting failed approaches
  - Critical behavior notes that explain CEmu's implementation details
  - Hardware quirks that affect the specific peripheral

  This prevents wasting time retreading failed approaches. Many peripheral bugs have subtle causes that aren't obvious from reading code alone.

- **Use the debug tool for testing** - From `core/` directory, use cargo aliases:
  ```bash
  cd core
  cargo boot      # Test boot progress
  cargo screen    # Render screen to PNG
  cargo vram      # Analyze VRAM colors
  cargo trace     # Generate trace (100k steps)
  cargo dbg       # Show help
  cargo t         # Run tests
  ```
  For more options: `cargo run --release --example debug -- <command>`

- **Full trace comparison with CEmu** - For detailed parity debugging:
  ```bash
  # Generate our trace (JSON with I/O ops)
  cd core
  cargo run --release --example debug -- fulltrace 1000

  # Generate CEmu trace (requires patched CEmu in cemu-ref/)
  cd ../cemu-ref
  ./test/fulltrace "../TI-84 CE.rom" 1000 /tmp/cemu_trace.json

  # Compare traces and report divergences
  cd ../core
  cargo run --release --example debug -- fullcompare ../traces/fulltrace_*.json /tmp/cemu_trace.json
  ```
  The comparison shows: PC/opcode mismatches, cycle differences, register state divergences, and I/O operation differences.
- **Verify parity after every change** - After making any change to CPU, bus, peripherals, or timing code:
  1. Run boot test: `cargo run --release --example debug -- boot`
  2. Generate trace: `cargo run --release --example debug -- trace 100000`
  3. Compare with CEmu trace to verify no regressions

  If divergence is found, investigate immediately before continuing other work.

- **Minimize Grep tool usage** - Prefer Read tool with specific line ranges when possible, as Grep requires manual approval. Use Read to examine specific file sections rather than searching.
- **Update milestones when completing features** - After implementing a feature from [docs/milestones.md](docs/milestones.md), mark it as complete (`[x]`) and update the test count and status section.
- **Document interesting findings** - When discovering esoteric behavior or surprising implementation details, add them to [docs/findings.md](docs/findings.md). This includes:
  - Hardware quirks (timing, undocumented registers, differences from standard Z80)
  - Boot sequence discoveries (what the ROM expects, why it stalls)
  - CPU instruction encoding surprises (eZ80 vs Z80 differences)
  - Peripheral initialization requirements
  - Any "aha!" moments that took significant debugging to uncover

  For each finding, document:
  - What the behavior is
  - Why it matters (what breaks without it)
  - Where the information came from (CEmu source file, ROM trace analysis, etc.)

## Architecture

- See [docs/architecture.md](docs/architecture.md) for system design
- See [docs/milestones.md](docs/milestones.md) for implementation roadmap
- See [docs/findings.md](docs/findings.md) for interesting discoveries

## CEmu Reference

CEmu is the primary reference emulator for TI-84 Plus CE hardware behavior.

- Repository: https://github.com/CE-Programming/CEmu
- **Local clone**: `cemu-ref/` directory (added to .gitignore, not committed)

### CEmu Core Directory Structure (core/)

**Hardware Emulation:**

- `asic.c/h` - Main ASIC orchestrator, initializes port_map array with 16 device handlers
- `cpu.c/h` - eZ80 CPU implementation
- `control.c/h` - Control ports (0xFF00xx via OUT0/IN0 instructions)
- `flash.c/h` - Flash memory controller and status registers
- `lcd.c/h` - LCD controller at 0xE30000
- `timers.c/h` - General purpose timers at 0xF20000
- `keypad.c/h` - Keypad controller at 0xF50000
- `interrupt.c/h` - Interrupt controller at 0xF00000
- `backlight.c/h` - LCD backlight control
- `spi.c/h` - SPI bus for hardware communication
- `uart.c/h` - Serial port emulation

**Memory:**

- `mem.c/h` - Memory bus routing (flash/RAM/ports)
- `bus.c/h` - Bus operations

**Debug:**

- `debug/` - Debugger and disassembler utilities

**Trace Integration (our additions):**

- `trace.c/h` - Full trace generation with JSON output (added for parity testing)
- `../test/fulltrace.c` - Standalone trace generator tool

### Implementation Status

| CEmu Component | Our Status     | Notes                                                     |
| -------------- | -------------- | --------------------------------------------------------- |
| `asic.c`       | ✅ Equivalent  | Our `Ports` struct in peripherals/mod.rs serves same role |
| `cpu.c`        | ✅ Implemented | core/src/cpu/ directory                                   |
| `control.c`    | ✅ Implemented | peripherals/control.rs                                    |
| `flash.c`      | ✅ Implemented | peripherals/flash.rs                                      |
| `lcd.c`        | ✅ Implemented | peripherals/lcd.rs                                        |
| `timers.c`     | ✅ Implemented | peripherals/timers.rs + OS Timer in mod.rs                |
| `keypad.c`     | ✅ Implemented | peripherals/keypad.rs                                     |
| `interrupt.c`  | ✅ Implemented | peripherals/interrupt.rs                                  |
| `mem.c`        | ⚠️ Partial     | Memory protection checks disabled                         |
| `backlight.c`  | ❌ Stub        | Not needed for boot                                       |
| `misc.c`       | ✅ Stub        | peripherals/watchdog.rs                                   |
| `realclock.c`  | ✅ Implemented | peripherals/rtc.rs + scheduler integration                |
| `schedule.c`   | ✅ Implemented | scheduler.rs (7.68 GHz base clock)                        |
| `sha256.c`     | ❌ Missing     | SHA256 accelerator                                        |
| `spi.c`        | ✅ Implemented | peripherals/spi.rs (FIFO, timing, RX-only transfers)      |
| `uart.c`       | ❌ Missing     | Serial port                                               |
| `usb/`         | ❌ Missing     | USB controller                                            |

### TI-84 CE Memory Map

| Address Range     | Device               | CEmu File   |
| ----------------- | -------------------- | ----------- |
| 0x000000-0x3FFFFF | Flash (4MB)          | flash.c     |
| 0xD00000-0xD657FF | RAM (256KB+VRAM)     | mem.c       |
| 0xE00000-0xE0FFFF | Control ports        | control.c   |
| 0xE10000-0xE1FFFF | Flash controller     | flash.c     |
| 0xE30000-0xE300FF | LCD controller       | lcd.c       |
| 0xF00000-0xF0001F | Interrupt controller | interrupt.c |
| 0xF20000-0xF2003F | Timers (3x GPT)      | timers.c    |
| 0xF50000-0xF5003F | Keypad               | keypad.c    |
| 0xFF0000-0xFF00FF | Control ports (OUT0) | control.c   |

### Control Ports (0xFF00xx / 0xE000xx)

Accessed via OUT0/IN0 instructions or direct memory access:

| Port | Function                   |
| ---- | -------------------------- |
| 0x00 | Power control, battery     |
| 0x01 | CPU speed (6/12/24/48 MHz) |
| 0x02 | Battery status readout     |
| 0x03 | Device type, serial flash  |
| 0x05 | Control flags              |
| 0x06 | Protected ports unlock     |
| 0x08 | Fixed value (0x7F)         |
| 0x0D | LCD enable/disable         |
| 0x0F | USB status                 |
| 0x1C | Fixed value (0x80)         |
| 0x28 | Flash unlock status        |

### Flash Controller Ports (0xE10000)

| Offset | Function            |
| ------ | ------------------- |
| 0x00   | Flash enable        |
| 0x01   | Flash size config   |
| 0x02   | Flash map selection |
| 0x05   | Wait states         |

### Boot Sequence Notes

**Boot is complete!** The emulator successfully boots to the TI-84 CE home screen.

The ROM boot sequence:

1. Disables interrupts (DI)
2. Configures control ports via OUT0 instructions
3. Sets up memory protection boundaries
4. Configures flash controller
5. Initializes VRAM, copies code to RAM
6. Multiple ON key wake cycles for power management
7. LCD initialized with control value 0x92D (16bpp RGB565)
8. **OS reaches idle loop** at PC=0x085B7F (EI + NOP + HALT)
9. Screen displays "RAM Cleared" with full status bar

Boot completes in ~3.6M steps (~61.6M cycles at 48MHz).

### eZ80-Specific Behavior

Key differences from standard Z80 that affect emulation:

- **Memory-mapped I/O**: `IN r,(C)` and `OUT (C),r` access address `0xFF0000 | C`, not traditional I/O ports
- **L/IL mode flags**: Separate flags for data (L) vs instruction (IL) addressing; suffix opcodes (0x40, 0x49, 0x52, 0x5B) temporarily override these per-instruction
- **Block instructions**: LDIR, LDDR, CPIR, CPDR, OTIMR, etc. execute all iterations in a single `step()` call (matches CEmu behavior)
- **ED prefix decoding**: ED z=5 RETN/RETI only valid for y=0,1; other y values are NOP

## Key Lessons Learned

### Exact Scheduler Parity Required

Exact cycle timing parity with CEmu is **required**. Every instruction should execute with identical cycle counts. Key areas to verify:

- **Register parity**: AF, BC, DE, HL, IX, IY, SP must match at every PC
- **Cycle parity**: Total cycles must match CEmu at each instruction
- **Memory timing**: Flash wait states, RAM cycles, port write delays must match

Known cycle timing differences to fix:
- **ED39 (OUT0)**: CPU speed port writes show ~28K cycle difference due to clock conversion
- **Branch instructions**: DJNZ costs 23-29 cycles (should be 37-43 like CEmu)
- **JR NZ/JR Z**: ~188/107 cycle difference per call

Use `cargo trace` and compare against CEmu traces to verify parity.

### Trace Comparison Strategy

Use `fulltrace` and `fullcompare` commands for detailed JSON traces with I/O operations:

1. **Check cycle deltas** - Compare per-instruction cycle costs, not just totals
2. **Check AF (flags)** - Flag differences often reveal CPU bugs
3. **Account for suffix opcodes** - CEmu logs 40/49/52/5B as separate steps, we combine them
4. **Run longer traces** - Many bugs only appear after 100K+ steps
5. **Analyze by opcode** - Group cycle drift by instruction type to find systematic issues

**JSON Trace Format** (both emulators produce):
```json
{
  "step": 0, "cycle": 0, "pc": "0x000000",
  "opcode": {"bytes": "F3", "mnemonic": "DI"},
  "regs_before": {"A": "0x00", "F": "0x00", "BC": "0x000000", ...},
  "io_ops": [{"type": "write", "target": "ram", "addr": "0xD00000", "new": "0xFF"}]
}
```

**CEmu Trace Tool** (`cemu-ref/test/fulltrace`):
- Built from patched CEmu with trace hooks in cpu.c and mem.c
- Rebuild: `cd cemu-ref/core && make && cd .. && gcc -I core -o test/fulltrace test/fulltrace.c -L core -lcemucore -lm`

### Critical Instructions for Boot

These eZ80 instructions were essential for boot success:
- `LD A,MB` (ED 6E) - Load MBASE into A, used for RAM address validation
- `MLT` (ED 4C/5C/6C/7C) - Multiply high/low bytes of register pair
- `LEA` (ED 22/23) - Load effective address into register pair
- Indexed loads (DD/FD 31/3E) - Special eZ80 indexed memory operations
