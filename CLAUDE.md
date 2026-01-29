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

- **Update milestones when completing features** - After implementing a feature from [docs/milestones.md](docs/milestones.md), mark it as complete (`[x]`) and update the test count and status section.

## Architecture

- See [docs/architecture.md](docs/architecture.md) for system design
- See [docs/milestones.md](docs/milestones.md) for implementation roadmap

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

### Implementation Status

| CEmu Component | Our Status     | Notes                                                     |
| -------------- | -------------- | --------------------------------------------------------- |
| `asic.c`       | ✅ Equivalent  | Our `Ports` struct in peripherals/mod.rs serves same role |
| `cpu.c`        | ✅ Implemented | core/src/cpu/ directory                                   |
| `control.c`    | ✅ Implemented | peripherals/control.rs                                    |
| `flash.c`      | ✅ Implemented | peripherals/flash.rs                                      |
| `lcd.c`        | ✅ Implemented | peripherals/lcd.rs                                        |
| `timers.c`     | ✅ Implemented | peripherals/timers.rs                                     |
| `keypad.c`     | ✅ Implemented | peripherals/keypad.rs                                     |
| `interrupt.c`  | ✅ Implemented | peripherals/interrupt.rs                                  |
| `mem.c`        | ⚠️ Partial     | Memory protection checks disabled                         |
| `backlight.c`  | ❌ Stub        | Not needed for boot                                       |
| `misc.c`       | ❌ Missing     | Watchdog timer, power events                              |
| `realclock.c`  | ❌ Missing     | RTC - may need stub for boot                              |
| `sha256.c`     | ❌ Missing     | SHA256 accelerator                                        |
| `spi.c`        | ❌ Missing     | SPI bus                                                   |
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

The ROM boot sequence:

1. Disables interrupts (DI)
2. Configures control ports via OUT0 instructions
3. Sets up memory protection boundaries
4. Configures flash controller
5. Initializes VRAM (screen shows white)
6. Polls LCD status (current blocker at PC=0x5BA9)

### eZ80-Specific Behavior

Key differences from standard Z80 that affect emulation:

- **Memory-mapped I/O**: `IN r,(C)` and `OUT (C),r` access address `0xFF0000 | C`, not traditional I/O ports
- **L/IL mode flags**: Separate flags for data (L) vs instruction (IL) addressing; suffix opcodes (0x40, 0x49, 0x52, 0x5B) temporarily override these per-instruction
- **Block instructions**: LDIR, LDDR, CPIR, CPDR, OTIMR, etc. execute all iterations in a single `step()` call (matches CEmu behavior)
- **ED prefix decoding**: ED z=5 RETN/RETI only valid for y=0,1; other y values are NOP
