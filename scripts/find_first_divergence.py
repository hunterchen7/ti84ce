#!/usr/bin/env python3
"""Find the first divergence point between two emulator traces.

Compares:
- PC (program counter)
- Registers (A, F, BC, DE, HL, IX, IY, SP)
- I/O operations (memory reads/writes, port access)
"""
import json
import sys

def format_io_op(op):
    """Format an io_op for display."""
    if op.get("type") == "write":
        return f"{op['type']} {op['target']} {op['addr']}: {op.get('old', '?')} -> {op.get('new', '?')}"
    else:
        return f"{op['type']} {op['target']} {op['addr']}: {op.get('value', '?')}"

def compare_io_ops(ours_ops, cemu_ops, writes_only=False, ignore_old=False):
    """Compare two lists of io_ops. Returns (match, diff_description).

    If writes_only=True, only compare write operations (CEmu doesn't trace reads).
    If ignore_old=True, skip old_value comparison (can differ due to register model).
    """
    if writes_only:
        ours_ops = [op for op in ours_ops if op.get("type") == "write"]
        cemu_ops = [op for op in cemu_ops if op.get("type") == "write"]

    if len(ours_ops) != len(cemu_ops):
        return False, f"count mismatch: {len(ours_ops)} vs {len(cemu_ops)}"

    for i, (our_op, cemu_op) in enumerate(zip(ours_ops, cemu_ops)):
        # Compare type
        if our_op.get("type") != cemu_op.get("type"):
            return False, f"op[{i}] type: {our_op.get('type')} vs {cemu_op.get('type')}"

        # Compare target
        if our_op.get("target") != cemu_op.get("target"):
            return False, f"op[{i}] target: {our_op.get('target')} vs {cemu_op.get('target')}"

        # Compare address
        if our_op.get("addr") != cemu_op.get("addr"):
            return False, f"op[{i}] addr: {our_op.get('addr')} vs {cemu_op.get('addr')}"

        # Compare values
        if our_op.get("type") == "write":
            if not ignore_old and our_op.get("old") != cemu_op.get("old"):
                return False, f"op[{i}] old: {our_op.get('old')} vs {cemu_op.get('old')}"
            if our_op.get("new") != cemu_op.get("new"):
                return False, f"op[{i}] new: {our_op.get('new')} vs {cemu_op.get('new')}"
        else:
            if our_op.get("value") != cemu_op.get("value"):
                return False, f"op[{i}] value: {our_op.get('value')} vs {cemu_op.get('value')}"

    return True, None

def compare_traces(ours_path, cemu_path, max_steps=None, check_io_ops=True, writes_only=False, ignore_old=False):
    """Find the first divergence point between two traces."""

    print("Loading our trace...")
    with open(ours_path, "r") as f:
        ours = json.load(f)

    print("Loading CEmu trace...")
    with open(cemu_path, "r") as f:
        cemu = json.load(f)

    print(f"Our trace: {len(ours)} steps, CEmu trace: {len(cemu)} steps")

    # Compare step by step until divergence
    if max_steps:
        min_len = min(len(ours), len(cemu), max_steps)
    else:
        min_len = min(len(ours), len(cemu))

    first_pc_diff = None
    first_reg_diff = None
    first_io_diff = None

    for i in range(min_len):
        our_step = ours[i]
        cemu_step = cemu[i]

        our_pc = our_step.get("pc", "?")
        cemu_pc = cemu_step.get("pc", "?")

        our_regs = our_step.get("regs_before", {})
        cemu_regs = cemu_step.get("regs_before", {})

        # Check PC
        if first_pc_diff is None and our_pc != cemu_pc:
            first_pc_diff = i

        # Check key registers
        if first_reg_diff is None:
            for reg in ["A", "F", "BC", "DE", "HL", "IX", "IY", "SP"]:
                our_val = our_regs.get(reg, "?")
                cemu_val = cemu_regs.get(reg, "?")
                if our_val != cemu_val:
                    first_reg_diff = (i, reg, our_val, cemu_val)
                    break

        # Check I/O operations
        if check_io_ops and first_io_diff is None:
            our_io_ops = our_step.get("io_ops", [])
            cemu_io_ops = cemu_step.get("io_ops", [])
            match, diff_desc = compare_io_ops(our_io_ops, cemu_io_ops, writes_only, ignore_old)
            if not match:
                first_io_diff = (i, diff_desc, our_io_ops, cemu_io_ops)

        if first_pc_diff is not None or first_reg_diff is not None or first_io_diff is not None:
            break

    print("\n=== First Divergence Found ===")

    divergence_step = None

    if first_reg_diff:
        step, reg, our_val, cemu_val = first_reg_diff
        divergence_step = step if divergence_step is None else min(divergence_step, step)
        print(f"\nFirst REGISTER difference at step {step}:")
        print(f"  Register {reg}: ours={our_val}, cemu={cemu_val}")

    if first_io_diff:
        step, diff_desc, our_ops, cemu_ops = first_io_diff
        divergence_step = step if divergence_step is None else min(divergence_step, step)
        print(f"\nFirst I/O difference at step {step}:")
        print(f"  {diff_desc}")
        print(f"  Ours ({len(our_ops)} ops):")
        for op in our_ops[:5]:  # Show first 5
            print(f"    {format_io_op(op)}")
        if len(our_ops) > 5:
            print(f"    ... and {len(our_ops) - 5} more")
        print(f"  CEmu ({len(cemu_ops)} ops):")
        for op in cemu_ops[:5]:  # Show first 5
            print(f"    {format_io_op(op)}")
        if len(cemu_ops) > 5:
            print(f"    ... and {len(cemu_ops) - 5} more")

    if first_pc_diff:
        divergence_step = first_pc_diff if divergence_step is None else min(divergence_step, first_pc_diff)
        print(f"\nFirst PC difference at step {first_pc_diff}")

    if divergence_step is None:
        print(f"\nNo divergence found in {min_len} steps!")
        if len(ours) != len(cemu):
            print(f"  (trace lengths differ: {len(ours)} vs {len(cemu)})")
        return

    # Show context around divergence
    print(f"\n=== Context (steps {max(0, divergence_step-3)} to {min(min_len-1, divergence_step+2)}) ===")
    for j in range(max(0, divergence_step-3), min(min_len, divergence_step+3)):
        our_s = ours[j]
        cemu_s = cemu[j]
        our_pc = our_s.get("pc")
        cemu_pc = cemu_s.get("pc")
        our_op = our_s.get("opcode", {}).get("bytes", "?")
        cemu_op = cemu_s.get("opcode", {}).get("bytes", "?")
        marker = " <<< DIVERGENCE" if j == divergence_step else ""
        print(f"Step {j}: Ours PC={our_pc} op={our_op}  |  CEmu PC={cemu_pc} op={cemu_op}{marker}")

        if j == divergence_step:
            our_regs = our_s.get("regs_before", {})
            cemu_regs = cemu_s.get("regs_before", {})
            print(f"  Ours:  A={our_regs.get('A')} F={our_regs.get('F')} BC={our_regs.get('BC')} DE={our_regs.get('DE')} HL={our_regs.get('HL')}")
            print(f"  CEmu:  A={cemu_regs.get('A')} F={cemu_regs.get('F')} BC={cemu_regs.get('BC')} DE={cemu_regs.get('DE')} HL={cemu_regs.get('HL')}")

            # Show I/O ops at divergence
            our_io = our_s.get("io_ops", [])
            cemu_io = cemu_s.get("io_ops", [])
            if our_io or cemu_io:
                print(f"  I/O Ours: {[format_io_op(op) for op in our_io[:3]]}")
                print(f"  I/O CEmu: {[format_io_op(op) for op in cemu_io[:3]]}")

if __name__ == "__main__":
    if len(sys.argv) < 3:
        print("Usage: find_first_divergence.py <our_trace.json> <cemu_trace.json> [max_steps] [options]")
        print("\nOptions:")
        print("  max_steps     - Maximum steps to compare (default: all)")
        print("  --no-io       - Skip I/O operation comparison")
        print("  --writes-only - Only compare write operations (CEmu doesn't trace reads)")
        print("  --ignore-old  - Ignore old_value differences (register model differences)")
        sys.exit(1)

    ours_path = sys.argv[1]
    cemu_path = sys.argv[2]
    max_steps = None
    check_io = True
    writes_only = False
    ignore_old = False

    for arg in sys.argv[3:]:
        if arg == "--no-io":
            check_io = False
        elif arg == "--writes-only":
            writes_only = True
        elif arg == "--ignore-old":
            ignore_old = True
        elif arg.isdigit():
            max_steps = int(arg)

    compare_traces(ours_path, cemu_path, max_steps, check_io, writes_only, ignore_old)
