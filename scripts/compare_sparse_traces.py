#!/usr/bin/env python3
"""
Compare sparse traces from CEmu and our emulator.
Both should log at the same cycle intervals (every 100K cycles).
"""

import re
import sys

def parse_snapshot(line):
    """Parse a [snapshot] line and return a dict of values."""
    match = re.search(r'\[snapshot\] cycle=(\d+) PC=([0-9A-F]+) SP=([0-9A-F]+) AF=([0-9A-F]+) BC=([0-9A-F]+) DE=([0-9A-F]+) HL=([0-9A-F]+)', line)
    if not match:
        return None

    # Also parse interrupt and control state
    intr_match = re.search(r'INTR\[stat=([0-9A-F]+) en=([0-9A-F]+)', line)
    ctrl_match = re.search(r'CTRL\[pwr=([0-9A-F]+) spd=([0-9A-F]+)', line)
    halt_match = re.search(r'HALT=(\d+|true|false)', line)
    iff_match = re.search(r'IFF1=(\d+|true|false)', line)

    halt_val = halt_match.group(1) if halt_match else "0"
    halt = halt_val in ("1", "true")

    iff_val = iff_match.group(1) if iff_match else "0"
    iff1 = iff_val in ("1", "true")

    return {
        'cycle': int(match.group(1)),
        'pc': int(match.group(2), 16),
        'sp': int(match.group(3), 16),
        'af': int(match.group(4), 16),
        'bc': int(match.group(5), 16),
        'de': int(match.group(6), 16),
        'hl': int(match.group(7), 16),
        'intr_stat': int(intr_match.group(1), 16) if intr_match else 0,
        'intr_en': int(intr_match.group(2), 16) if intr_match else 0,
        'halt': halt,
        'iff1': iff1,
        'pwr': int(ctrl_match.group(1), 16) if ctrl_match else 0,
        'spd': int(ctrl_match.group(2), 16) if ctrl_match else 0,
        'line': line.strip()
    }

def load_snapshots(filename):
    """Load all snapshots from a trace file."""
    snapshots = []
    with open(filename, 'r') as f:
        for line in f:
            if line.startswith('[snapshot]'):
                snap = parse_snapshot(line)
                if snap:
                    snapshots.append(snap)
    return snapshots

def compare_snapshots(cemu, ours):
    """Compare two snapshots and return differences."""
    diffs = []

    # Compare key fields
    if cemu['pc'] != ours['pc']:
        diffs.append(f"PC: CEmu={cemu['pc']:06X} vs Ours={ours['pc']:06X}")
    if cemu['sp'] != ours['sp']:
        diffs.append(f"SP: CEmu={cemu['sp']:06X} vs Ours={ours['sp']:06X}")
    if cemu['af'] != ours['af']:
        diffs.append(f"AF: CEmu={cemu['af']:04X} vs Ours={ours['af']:04X}")
    if cemu['bc'] != ours['bc']:
        diffs.append(f"BC: CEmu={cemu['bc']:06X} vs Ours={ours['bc']:06X}")
    if cemu['de'] != ours['de']:
        diffs.append(f"DE: CEmu={cemu['de']:06X} vs Ours={ours['de']:06X}")
    if cemu['hl'] != ours['hl']:
        diffs.append(f"HL: CEmu={cemu['hl']:06X} vs Ours={ours['hl']:06X}")
    if cemu['halt'] != ours['halt']:
        diffs.append(f"HALT: CEmu={cemu['halt']} vs Ours={ours['halt']}")
    if cemu['iff1'] != ours['iff1']:
        diffs.append(f"IFF1: CEmu={cemu['iff1']} vs Ours={ours['iff1']}")

    return diffs

def main():
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <cemu_trace> <ours_trace>")
        sys.exit(1)

    cemu_file = sys.argv[1]
    ours_file = sys.argv[2]

    print(f"Loading CEmu trace: {cemu_file}")
    cemu_snaps = load_snapshots(cemu_file)
    print(f"  Loaded {len(cemu_snaps)} snapshots")

    print(f"Loading our trace: {ours_file}")
    ours_snaps = load_snapshots(ours_file)
    print(f"  Loaded {len(ours_snaps)} snapshots")

    # Build dictionaries by cycle for comparison
    cemu_by_cycle = {s['cycle']: s for s in cemu_snaps}
    ours_by_cycle = {s['cycle']: s for s in ours_snaps}

    # Find common cycles (excluding init/HALT/ON_KEY_PRESSED special entries)
    common_cycles = sorted(set(cemu_by_cycle.keys()) & set(ours_by_cycle.keys()))

    print(f"\nComparing {len(common_cycles)} common cycle points...")

    matches = 0
    divergences = []

    for cycle in common_cycles:
        cemu = cemu_by_cycle[cycle]
        ours = ours_by_cycle[cycle]

        diffs = compare_snapshots(cemu, ours)

        if not diffs:
            matches += 1
        else:
            divergences.append((cycle, diffs, cemu, ours))
            if len(divergences) <= 5:  # Show first 5 divergences
                print(f"\n=== DIVERGENCE at cycle {cycle} ===")
                for d in diffs:
                    print(f"  {d}")
                print(f"  CEmu: {cemu['line'][:100]}...")
                print(f"  Ours: {ours['line'][:100]}...")

    print(f"\n=== Summary ===")
    print(f"Total common cycle points: {len(common_cycles)}")
    print(f"Matches: {matches}")
    print(f"Divergences: {len(divergences)}")

    if divergences:
        print(f"\nFirst divergence at cycle: {divergences[0][0]}")
        print(f"Last divergence at cycle: {divergences[-1][0]}")
        return 1
    else:
        print("\n✓ All cycle points match!")
        return 0

if __name__ == '__main__':
    sys.exit(main())
