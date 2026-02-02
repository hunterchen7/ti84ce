# CEmu Test Tools

Test tools for comparing CEmu (reference emulator) behavior with our Rust TI-84 Plus CE emulator. These tools help verify parity between the two implementations.

## Prerequisites

### 1. Clone CEmu

Clone the CEmu repository into `cemu-ref/` at the project root:

```bash
cd /path/to/calc
git clone https://github.com/CE-Programming/CEmu.git cemu-ref
```

### 2. Build CEmu Core Library

Build the CEmu core library (headless, no Qt GUI):

```bash
cd cemu-ref/core
make
```

This creates `libcemucore.a` which our test tools link against.

### 3. Obtain a ROM

You need a TI-84 Plus CE ROM file. This is copyrighted and not included in the repository.
Place it at the project root as `TI-84 CE.rom` or specify the path when running tools.

## Building the Test Tools

```bash
cd tools/cemu-test
make
```

This builds:
- `parity_check` - RTC timing and MathPrint flag checker
- `trace_gen` - CPU instruction trace generator

## Tools

### parity_check

Checks RTC timing, MathPrint flag, and key emulator state at cycle milestones.
Useful for verifying our emulator matches CEmu's behavior during boot.

```bash
# Run with defaults (60M cycles, ROM at ../../TI-84 CE.rom)
./parity_check

# Specify ROM path and max cycles
./parity_check /path/to/rom.rom -m 100000000

# Verbose mode
./parity_check -v
```

**Output example:**
```
=== CEmu Parity Check ===

Cycle(M)  | RTC Ctrl | RTC Status | loadTicks | mode | MathPrint | PC
----------|----------|------------|-----------|------|-----------|--------
       27 | 0x40     | 0xF8       |       255 |    1 | 0x00 Classic   | 0x0101A1
       28 | 0x40     | 0xF8       |       255 |    1 | 0x00 Classic   | 0x00730C
```

**Key addresses monitored:**
- `0xD000C4` - MathPrint flag (bit 5: 1=MathPrint, 0=Classic)
- `0xF80020` - RTC control register (bit 6: load in progress)
- `0xF80040` - RTC load status (0x00=complete, 0xF8=all pending)

### trace_gen

Generates CPU instruction traces in the same format as our Rust emulator for direct comparison.

```bash
# Generate 1M step trace to stdout
./trace_gen ../../TI-84\ CE.rom

# Generate 100K step trace to file
./trace_gen ../../TI-84\ CE.rom -n 100000 -o cemu_trace.txt
```

**Output format (space-separated):**
```
step cycles PC SP AF BC DE HL IX IY ADL IFF1 IFF2 IM HALT opcode
```

### Comparing Traces

To compare CEmu and Rust emulator traces:

```bash
# Generate CEmu trace
./trace_gen ../../TI-84\ CE.rom -n 10000 -o cemu_trace.txt

# Generate Rust trace (from core/ directory)
cargo run --release --example debug -- trace -n 10000 > rust_trace.txt

# Compare
diff cemu_trace.txt rust_trace.txt | head -50
```

## Wrapper Library

For external integration (JNI, FFI), build the wrapper library:

```bash
make libcemu_wrapper.a
```

See `cemu_wrapper.h` for the API.

## Troubleshooting

### "CEmu core library not found"

Make sure you've built the CEmu core library:
```bash
cd ../../cemu-ref/core
make
```

### "ROM not found"

Specify the correct path to your ROM file:
```bash
./parity_check /path/to/your/TI-84\ CE.rom
```

### Compilation errors about missing headers

Ensure CEmu is cloned to the correct location:
```bash
ls ../../cemu-ref/core/emu.h  # Should exist
```

## Files

| File | Purpose |
|------|---------|
| `parity_check.c` | RTC/MathPrint parity checker |
| `trace_gen.c` | CPU trace generator |
| `cemu_wrapper.c/h` | Wrapper library for external use |
| `test_cemu.c` | Basic CEmu test |
| `test_wrapper.c` | Wrapper API test |
| `Makefile` | Build system |

## Related Documentation

- [docs/findings.md](../../docs/findings.md) - Investigation findings including RTC timing
- [CLAUDE.md](../../CLAUDE.md) - CEmu reference section with memory map and port details
