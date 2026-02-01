# CEmu Parity Investigation Findings

This document summarizes the instruction-level parity analysis between the Rust eZ80 emulator and CEmu.

## Investigation Date: 2026-01-31

## Summary

The investigation verified instruction-level parity with CEmu and fixed several F3/F5 flag preservation issues. **All 29 parity tests and 422 total tests now pass.**

## Key Findings and Fixes

### 1. F3/F5 Flag Preservation (FIXED)

**Issue**: CEmu preserves F3 (bit 3) and F5 (bit 5) from the previous F register value for ALL ALU operations using `cpuflag_undef(r->F)`. The Rust implementation was inconsistent.

**CEmu Behavior** (from `registers.h`):
```c
#define cpuflag_undef(a) ((a) & (FLAG_3 | FLAG_5))
```

This macro preserves F3/F5 from the old F register in every ALU operation.

**Fixes Applied**:

1. **alu_add()** in `helpers.rs`:
   - Added `old_f3f5` preservation before clearing F
   - Now matches CEmu's ADD/ADC behavior

2. **alu_sub()** in `helpers.rs`:
   - Fixed to preserve F3/F5 for SUB/SBC/CP (not just CP)
   - Now matches CEmu's SUB behavior

3. **execute_rot()** in `execute.rs`:
   - Added F3/F5 preservation for CB prefix rotate/shift operations
   - Now matches CEmu's RLC/RRC/RL/RR/SLA/SRA/SRL behavior

4. **NEG instruction** in `execute.rs`:
   - Added F3/F5 preservation for ED 44 (NEG)
   - Now matches CEmu's NEG behavior

### 2. Opcode Encoding Verification

**LEA Instructions** (eZ80-specific):
| Instruction | Correct Opcode | Notes |
|-------------|---------------|-------|
| LEA IX,IY+d | ED 54 | p=1, q=0, z=4 |
| LEA BC,IX+d | ED 02 | |
| LEA DE,IX+d | ED 12 | |
| LEA HL,IX+d | ED 22 | |

### 3. Block Instruction Behavior

LDIR/LDDR/CPIR/CPDR execute atomically in a single `step()` call, matching CEmu behavior.

### 4. Suffix Opcode Handling (0x40, 0x49, 0x52, 0x5B)

These opcodes temporarily override L/IL mode for the following instruction. The suffix and following instruction execute atomically.

## Parity Matrix (Post-Fix)

| Instruction Category | Flag Parity | Register Parity | Notes |
|---------------------|-------------|-----------------|-------|
| ADD/ADC A,r | ✅ Yes | ✅ Yes | F3/F5 preserved |
| SUB/SBC A,r | ✅ Yes | ✅ Yes | F3/F5 preserved |
| AND/OR/XOR | ✅ Yes | ✅ Yes | F3/F5 preserved |
| INC/DEC r | ✅ Yes | ✅ Yes | F3/F5 preserved |
| RLCA/RRCA/RLA/RRA | ✅ Yes | ✅ Yes | S, Z, PV preserved |
| CB RLC/RRC/RL/RR | ✅ Yes | ✅ Yes | F3/F5 preserved |
| CB SLA/SRA/SRL | ✅ Yes | ✅ Yes | F3/F5 preserved |
| CB BIT b,r | ✅ Yes | ✅ Yes | |
| LDI/LDD/LDIR/LDDR | ✅ Yes | ✅ Yes | Block completes atomically |
| CPI/CPD/CPIR/CPDR | ✅ Yes | ✅ Yes | |
| NEG | ✅ Yes | ✅ Yes | F3/F5 preserved |
| MLT | N/A | ✅ Yes | No flag changes |
| LEA | N/A | ✅ Yes | No flag changes |

## Test Results

```
test result: ok. 422 passed; 0 failed; 7 ignored; 0 measured; 0 filtered out
```

All parity tests pass:
- 29 parity-specific tests
- 422 total tests

## Files Modified

### Implementation Fixes
1. `core/src/cpu/helpers.rs`:
   - `alu_add()`: Added F3/F5 preservation
   - `alu_sub()`: Fixed F3/F5 preservation for all sub operations

2. `core/src/cpu/execute.rs`:
   - `execute_rot()`: Added F3/F5 preservation for CB prefix
   - NEG instruction: Added F3/F5 preservation

### New Test Files
1. `core/src/cpu/tests/parity.rs`: Comprehensive CEmu parity test suite
2. `core/src/cpu/tests/mod.rs`: Added parity module
3. `docs/parity_findings.md`: This document

## CEmu Reference Implementation

Flag calculation formulas from CEmu (`registers.h`):
```c
// Preserve undefined bits (F3/F5) from old F
#define cpuflag_undef(a) ((a) & (FLAG_3 | FLAG_5))

// Overflow detection
#define cpuflag_overflow_b_add(op1, op2, result) \
    cpuflag_pv(((op1) ^ (result)) & ((op2) ^ (result)) & 0x80)
#define cpuflag_overflow_b_sub(op1, op2, result) \
    cpuflag_pv(((op1) ^ (op2)) & ((op1) ^ (result)) & 0x80)

// Half-carry detection
#define cpuflag_halfcarry_b_add(op1, op2, carry) \
    cpuflag_h((((op1) & 0x0f) + ((op2) & 0x0f) + (carry)) & 0x10)
#define cpuflag_halfcarry_b_sub(op1, op2, carry) \
    cpuflag_h((((op1) & 0x0f) - ((op2) & 0x0f) - (carry)) & 0x10)
```

## Related Documentation

- [findings.md](findings.md) - General emulator findings
- [milestones.md](milestones.md) - Implementation roadmap
- CEmu source: `cemu-ref/core/cpu.c`, `cemu-ref/core/registers.h`
