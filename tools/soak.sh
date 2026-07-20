#!/usr/bin/env bash
set -euo pipefail

# CPU/mock soak harness. When a candidate binary and GraphSpec are supplied it
# runs a single long-lived graph for the configured duration; otherwise it falls
# back to repeating the workspace test suite. In both cases it writes a machine-
# readable summary to the artifact directory. Production release evidence must
# still be collected on target hardware with the real GraphSpec/model/stream
# hash recorded in dev-docs/007_core_modules_product_ready_closure_plan/RELEASE_EVIDENCE_TEMPLATE.md.

DURATION=7200
RELEASE_FLAG=""
ARTIFACT_DIR="soak-logs"
CANDIDATE=""
SPEC=""
BASELINE=""
PROFILE=""

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
        --artifact-dir)
            ARTIFACT_DIR="$2"
            shift 2
            ;;
        --candidate)
            CANDIDATE="$2"
            shift 2
            ;;
        --spec)
            SPEC="$2"
            shift 2
            ;;
        --baseline)
            BASELINE="$2"
            shift 2
            ;;
        --profile)
            PROFILE="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1" >&2
            echo "Usage: $0 [--duration SECONDS] [--release] [--artifact-dir DIR] [--candidate BIN] [--spec GRAPH] [--baseline ARTIFACT] [--profile NAME]" >&2
            exit 1
            ;;
    esac
done

mkdir -p "$ARTIFACT_DIR"

START=$(date +%s)
ITER=0
MODE=""
EXIT_CODE=0

if [[ -n "$CANDIDATE" && -n "$SPEC" ]]; then
    MODE="candidate"
    echo "[soak] candidate=$CANDIDATE spec=$SPEC duration=${DURATION}s artifact=$ARTIFACT_DIR"
    # Run the candidate graph under a hard timeout. The candidate is expected to
    # handle SIGTERM gracefully and shut down within its own deadline.
    timeout --signal=TERM "$DURATION" "$CANDIDATE" run --config "$SPEC" --format json \
        > "$ARTIFACT_DIR/soak.log" 2>&1 || EXIT_CODE=$?
    if [[ $EXIT_CODE -eq 124 ]]; then
        echo "[soak] candidate stopped by timeout (expected)"
        EXIT_CODE=0
    fi
else
    MODE="workspace-tests"
    while true; do
        ITER=$((ITER + 1))
        NOW=$(date +%s)
        ELAPSED=$((NOW - START))
        if [[ $ELAPSED -ge $DURATION ]]; then
            break
        fi

        echo "[soak] iteration $ITER, elapsed ${ELAPSED}s, remaining $((DURATION - ELAPSED))s"
        cargo test --workspace --locked $RELEASE_FLAG \
            > "$ARTIFACT_DIR/iter-$ITER.log" 2>&1 || EXIT_CODE=$?
        tail -n 5 "$ARTIFACT_DIR/iter-$ITER.log"

        if [[ $EXIT_CODE -ne 0 ]]; then
            echo "[soak] workspace tests failed with exit code $EXIT_CODE" >&2
            break
        fi
    done
fi

NOW=$(date +%s)
ELAPSED=$((NOW - START))
echo "[soak] completed mode=$MODE iterations=$ITER elapsed=${ELAPSED}s exit=$EXIT_CODE"

python3 - "$ARTIFACT_DIR/soak-summary.json" "$MODE" "$ITER" "$ELAPSED" "$EXIT_CODE" "$DURATION" "$CANDIDATE" "$SPEC" "$BASELINE" "$PROFILE" <<'PY'
import json, sys
path, mode, iterations, elapsed, exit_code, duration, candidate, spec, baseline, profile = sys.argv[1:11]
summary = {
    "mode": mode,
    "iterations": int(iterations),
    "elapsed_seconds": int(elapsed),
    "exit_code": int(exit_code),
    "duration_requested_seconds": int(duration),
    "candidate": candidate or None,
    "spec": spec or None,
    "baseline": baseline or None,
    "profile": profile or None,
}
with open(path, "w") as f:
    json.dump(summary, f, indent=2)
PY
