# CEmu vs emu-core boot trace comparison (2026-01-29)

## Goal
Capture richer boot-time traces from both emulators, then compare to find the earliest divergence and likely causes.

## How to capture traces
- emu-core: `cargo run --example trace_boot --manifest-path core/Cargo.toml > trace_ours.log`
- CEmu core (local clone in `cemu-ref`): `./cemu-ref/trace_cli > trace_cemu.log 2>&1`

Notes:
- emu-core trace logs CPU/interrupt/control state plus timers + LCD, and opcode bytes at PC.
- CEmu trace logs snapshot state changes plus per-instruction `[inst]` lines (PC + opcode bytes + key state) via a CPU trace callback.
- CEmu prints to stderr for some output; redirecting `2>&1` keeps the trace intact.

## How to compare traces
1. **Align the series.** emu-core logs `[snapshot]` once per step; CEmu logs `[inst]` once per instruction.
2. **Normalize differences.** CEmu uses `0/1` while emu-core logs `true/false`. Normalize booleans and `IM` formats before diffing.
3. **Compare a minimal field set first.** Start with `PC`, `SP`, `ADL`, `IFF1`, `IFF2`, `HALT`, and `op`.
4. **Find the earliest divergence** and focus fixes there; later diffs are usually cascading.

Quick alignment script (run from repo root):
```bash
python3 - <<'PY'
import re
from itertools import zip_longest

fields = ['PC','SP','ADL','IFF1','IFF2','HALT','op']

def norm_bool(v):
    if v in ('false','0'):
        return '0'
    if v in ('true','1'):
        return '1'
    return v

def norm_op(v):
    if v is None:
        return v
    return v.replace(' (init)','').strip()

def parse_line(line):
    d={}
    m=re.search(r'\\bop=([^\\]]+)$', line)
    if m:
        d['op']=norm_op(m.group(1).strip())
    for key in ['PC','SP','ADL','IFF1','IFF2','HALT']:
        m=re.search(r'\\b'+re.escape(key)+r'=?([^\\s\\]]+)', line)
        if m:
            val=m.group(1)
            if key in ('ADL','IFF1','IFF2','HALT'):
                val=norm_bool(val)
            d[key]=val
    return d

def parse(path, kind):
    out=[]
    with open(path,'r',errors='replace') as f:
        for line in f:
            line=line.rstrip('\\n')
            if line.startswith(kind):
                out.append(parse_line(line))
    return out

ours=parse('trace_ours.log','[snapshot]')
cemu=parse('trace_cemu.log','[inst]')

for i,(a,b) in enumerate(zip_longest(ours, cemu)):
    if a is None or b is None:
        print('length mismatch at', i, 'ours', len(ours), 'cemu', len(cemu))
        break
    diffs=[]
    for k in fields:
        if k in a and k in b and a[k]!=b[k]:
            diffs.append((k,a[k],b[k]))
    if diffs:
        print('diverge_at', i)
        print('diffs', diffs)
        print('ours', a)
        print('cemu', b)
        break
else:
    print('no divergence (on compared fields), len', len(ours))
PY
```

## Key findings so far
1. **ED z=6 decode mismatch fixed.**
   - CEmu treats `ED 7E` as `RSMIX` (MADL=0), not IM2.
   - emu-core now only sets IM for `ED 46/56/5E` (y=0/2/3). This removed the earliest IM divergence at `PC=000003`.

2. **Current earliest divergence: ADL flips off too early.**
   - At `PC=000E57` (opcode `3E`), emu-core has `ADL=0` while CEmu keeps `ADL=1`.
   - This is likely due to suffix handling (e.g., `0x40` treated as a suffix that resets ADL). CEmu keeps ADL enabled here.

3. **Trace visibility is now high enough to pinpoint issues.**
   - With opcode bytes + peripheral snapshots logged each step, the first mismatch is actionable instead of “blind”.

## Changes made (full list from this effort)
### emu-core
- **CPU reset defaults aligned with CEmu**
  - Default `ADL=false`, `SP=0x000000`, `MBASE=0x00`.
- **Suffix handling groundwork**
  - Added `pending_adl` mechanism and applied it before fetch (still needs refinement).
- **ED z=6 IM decode fix**
  - Only `y=0/2/3` map to IM0/1/2; `ED 7E` no longer changes IM.
- **Trace improvements**
  - `trace_boot` logs opcode bytes using `mask_addr` so Z80-mode fetches match real bytes.
  - Added logging for interrupt status/control, timers, LCD, and on-key wake.
- **Snapshot APIs for tracing**
  - Added `TimerSnapshot` and `LcdSnapshot` plus getters in `Emu`.
  - Added getters for CPU state (`iff2`, `interrupt_mode`, `adl`, etc.), IRQ state, control registers, and address masking.
- **Peripheral behavior alignment**
  - Control port power reads now return stored value (no forced bit 4).
  - Added LCD/timer getters for trace visibility.
- **Keypad behavior/tests**
  - Keypad read now uses last scanned data; tests updated to scan before read.
  - Added scan helper in tests and updated keypad routing tests accordingly.
  - Removed unused `scan_enabled` helper and underscored unused `key_state` param.
- **CPU tests updated for ADL/MBASE**
  - Tests now set `cpu.adl = true` or `cpu.mbase = 0xD0` where needed.
  - `test_new_cpu` now expects `adl=false`.
- **Misc cleanup**
  - Removed unused `cycles_before` in `core/src/emu.rs`.

### CEmu reference
- **Per-instruction trace hook**
  - Added `cpu_set_trace_callback()` and per-instruction trace calls.
  - `trace_cli` prints `[inst]` lines with opcode bytes for direct comparison.

## What we learned
- **ED z=6 table is not “all IM”.** Only y=0/2/3 are IM ops; y=7 is `RSMIX` in CEmu.
- **ADL suffix behavior is subtle.** Treating suffix opcodes as permanent ADL changes is likely wrong; CEmu keeps ADL true through `0x40` here.
- **Earliest divergence wins.** Fixing the first mismatch prevents cascading “false” divergences later.

## Recommended approach going forward
1. **Always fix the earliest divergence first.**
2. **Add trace-level assertions or micro-tests** for any fix (e.g., suffix/ADL semantics).
3. **Keep changes minimal and reversible.** Avoid broad tweaks until a trace shows a concrete mismatch.
4. **Use CEmu as reference, not guesswork.** If we can’t explain a divergence using CEmu’s decode/execute path, pause and inspect it.

## Next steps
- Fix ADL suffix handling so ADL doesn’t reset at `0x40` in this boot path.
- Re-run the trace comparison and update the earliest divergence.
