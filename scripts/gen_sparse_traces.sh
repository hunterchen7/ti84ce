#!/bin/bash
# Generate sparse 100M cycle traces for both CEmu and our emulator
# Usage: ./scripts/gen_sparse_traces.sh

set -e

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
TRACES_DIR="traces"

# Create traces directory if it doesn't exist
mkdir -p "$TRACES_DIR"

CEMU_TRACE="$TRACES_DIR/cemu_sparse_${TIMESTAMP}.log"
OURS_TRACE="$TRACES_DIR/ours_sparse_${TIMESTAMP}.log"

echo "=== Generating sparse 100M cycle traces with timestamp: $TIMESTAMP ==="
echo ""

# Rebuild CEmu trace_cli
echo "Building CEmu trace_cli..."
cd cemu-ref
make -j4 trace_cli 2>&1 | tail -5
cd ..

# Generate CEmu trace
echo "Generating CEmu sparse trace -> $CEMU_TRACE"
echo "  (This will take a few minutes for 100M cycles...)"
cd cemu-ref
./trace_cli > "../$CEMU_TRACE" 2>&1
cd ..
echo "  Done: $(wc -l < "$CEMU_TRACE") lines"

# Generate our trace
echo "Generating our sparse trace -> $OURS_TRACE"
echo "  (This will take a few minutes for 100M cycles...)"
cd core
cargo run --release --example sparse_trace > "../$OURS_TRACE" 2>&1
cd ..
echo "  Done: $(wc -l < "$OURS_TRACE") lines"

echo ""
echo "=== Comparing sparse traces ==="
python3 scripts/compare_sparse_traces.py "$CEMU_TRACE" "$OURS_TRACE"

echo ""
echo "=== Trace files ==="
echo "CEmu: $CEMU_TRACE"
echo "Ours: $OURS_TRACE"
