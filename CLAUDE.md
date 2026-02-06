# Claude Code Instructions

Project-specific guidelines for Claude Code when working on this TI-84 Plus CE emulator.

## Code Style

- When leaving functionality for future implementation, always add a `TODO:` comment explaining what needs to be done and which milestone it's planned for
  - Example: `// TODO: Wire up BusFault when Bus reports invalid memory access (Milestone 5+)`
- Keep TODO comments concise but include enough context to understand the task

## Workflow

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

### Exact Scheduler Parity Required

Exact cycle timing parity with CEmu is **required**. Every instruction should execute with identical cycle counts. Key areas to verify:

- **Register parity**: AF, BC, DE, HL, IX, IY, SP must match at every PC
- **Cycle parity**: Total cycles must match CEmu at each instruction
- **Memory timing**: Flash wait states, RAM cycles, port write delays must match

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
