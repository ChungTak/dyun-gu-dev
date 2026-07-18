#!/usr/bin/env bash
set -euo pipefail

# Basic CPU/mock soak harness. Runs the workspace test suite repeatedly for a
# configurable duration and logs each iteration. This is a software-level
# regression soak; production release evidence must be collected on target
# hardware with the real GraphSpec/model/stream hash recorded in
# dev-docs/006_core_modules_product_ready_plan/RELEASE_EVIDENCE_TEMPLATE.md.

DURATION=7200
RELEASE_FLAG=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --duration)
            DURATION="$2"
            shift 2
            ;;
        --release)
            RELEASE_FLAG="--release"
            shift
            ;;
        *)
            echo "Unknown option: $1" >&2
            echo "Usage: $0 [--duration SECONDS] [--release]" >&2
            exit 1
            ;;
    esac
done

LOG_DIR="soak-logs"
mkdir -p "$LOG_DIR"

START=$(date +%s)
ITER=0

while true; do
    ITER=$((ITER + 1))
    NOW=$(date +%s)
    ELAPSED=$((NOW - START))
    if [[ $ELAPSED -ge $DURATION ]]; then
        break
    fi

    echo "[soak] iteration $ITER, elapsed ${ELAPSED}s, remaining $((DURATION - ELAPSED))s"
    cargo test --workspace --locked $RELEASE_FLAG \
        > "$LOG_DIR/iter-$ITER.log" 2>&1
    tail -n 5 "$LOG_DIR/iter-$ITER.log"
done

NOW=$(date +%s)
ELAPSED=$((NOW - START))
echo "[soak] completed $ITER iterations in ${ELAPSED}s"
