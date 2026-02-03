#!/usr/bin/env python3
import json
import sys

def compare_traces(ours_path, cemu_path, max_steps=2066700):
    """Find the first divergence point between two traces."""

    print("Loading our trace...")
    with open(ours_path, "r") as f:
        ours = json.load(f)

    print("Loading CEmu trace...")
    with open(cemu_path, "r") as f:
        cemu = json.load(f)

    print(f"Our trace: {len(ours)} steps, CEmu trace: {len(cemu)} steps")

    # Compare step by step until divergence
    min_len = min(len(ours), len(cemu), max_steps)

    first_pc_diff = None
    first_reg_diff = None

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

        if first_pc_diff is not None or first_reg_diff is not None:
            break

    print("\n=== First Divergence Found ===")

    if first_reg_diff:
        step, reg, our_val, cemu_val = first_reg_diff
        print(f"First register difference at step {step}:")
        print(f"  Register {reg}: ours={our_val}, cemu={cemu_val}")

        # Show context: a few steps before and after
        print(f"\nContext (steps {max(0, step-5)} to {min(min_len-1, step+3)}):")
        for j in range(max(0, step-5), min(min_len, step+4)):
            our_s = ours[j]
            cemu_s = cemu[j]
            our_pc = our_s.get("pc")
            cemu_pc = cemu_s.get("pc")
            our_op = our_s.get("opcode", {}).get("bytes", "?")
            cemu_op = cemu_s.get("opcode", {}).get("bytes", "?")
            marker = " <<< FIRST DIFF" if j == step else ""
            print(f"  Step {j}: Ours PC={our_pc} op={our_op}  |  CEmu PC={cemu_pc} op={cemu_op}{marker}")
            if j == step:
                our_regs = our_s.get("regs_before", {})
                cemu_regs = cemu_s.get("regs_before", {})
                print(f"    Ours:  A={our_regs.get('A')} F={our_regs.get('F')} BC={our_regs.get('BC')} DE={our_regs.get('DE')} HL={our_regs.get('HL')}")
                print(f"    CEmu:  A={cemu_regs.get('A')} F={cemu_regs.get('F')} BC={cemu_regs.get('BC')} DE={cemu_regs.get('DE')} HL={cemu_regs.get('HL')}")

    if first_pc_diff:
        print(f"\nFirst PC difference at step {first_pc_diff}")

if __name__ == "__main__":
    compare_traces("traces/fulltrace_20260203_030606.json", "/tmp/cemu_trace_4m.json")
