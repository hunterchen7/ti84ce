#!/usr/bin/env python3
import json
import sys

def extract_steps(filepath, start_step, end_step, label):
    print(f"\n=== {label} ===")
    with open(filepath, "r") as f:
        for i, line in enumerate(f):
            if start_step <= i <= end_step:
                data = json.loads(line)
                pc = data.get("pc", "?")
                opcode = data.get("opcode", {})
                obytes = opcode.get("bytes", "?")
                mnem = opcode.get("mnemonic", "?")
                print(f"Step {i}: PC={pc} opcode={obytes} mnem={mnem}")
                if i == start_step or i == start_step + 2:
                    regs = data.get("regs_before", {})
                    print(f"  Regs: A={regs.get('A')} F={regs.get('F')} BC={regs.get('BC')} DE={regs.get('DE')} HL={regs.get('HL')}")
                    print(f"        IX={regs.get('IX')} IY={regs.get('IY')} SP={regs.get('SP')}")
            if i > end_step:
                break

if __name__ == "__main__":
    extract_steps("traces/fulltrace_20260203_020900.json", 2066675, 2066685, "Our Emulator")
    extract_steps("/tmp/cemu_trace_4m.json", 2066675, 2066685, "CEmu")
