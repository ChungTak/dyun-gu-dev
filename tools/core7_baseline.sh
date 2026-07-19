#!/usr/bin/env bash
set -euo pipefail

# Capture Plan 7 admission baseline: HEAD, lock/header hashes, toolchain and
# recent history. The output is intended to be checked into
# dev-docs/007_core_modules_product_ready_closure_plan/ADMISSION_BASELINE.md as
# evidence for CORE7-01.

OUT_DIR="target/core7-baseline"
mkdir -p "$OUT_DIR"

OUT_FILE="$OUT_DIR/baseline.md"
{
    echo "# CORE7-01 自动采集基线"
    echo
    echo "Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo
    echo "## Identity"
    echo
    echo '| 字段 | 值 |'
    echo '|---|---|'
    echo "| HEAD | \`$(git rev-parse HEAD)\` |"
    echo "| 分支 | \`$(git rev-parse --abbrev-ref HEAD)\` |"
    echo "| 工作树 | $(if [[ -z $(git status --short) ]]; then echo clean; else echo dirty; fi) |"
    echo "| Rust | \`$(rustc --version)\` |"
    echo "| Host | \`$(rustc -vV | sed -n 's/^host: //p')\` |"
    echo "| Cargo.lock SHA-256 | \`$(sha256sum Cargo.lock | cut -d' ' -f1)\` |"
    echo "| C header SHA-256 | \`$(sha256sum crates/dg-capi/include/dg_capi.h | cut -d' ' -f1)\` |"
    echo
    echo '## Toolchain'
    echo
    echo '```'
    rustc --version --verbose
    cargo --version --verbose
    echo '```'
    echo
    echo '## Recent log'
    echo
    echo '```'
    git log -10 --oneline
    echo '```'
    echo
    echo '## Workspace metadata (no deps)'
    echo
    echo '```json'
    cargo metadata --locked --no-deps --format-version 1 | head -c 4096
    echo '```'
} > "$OUT_FILE"

echo "[core7-baseline] wrote $OUT_FILE"
