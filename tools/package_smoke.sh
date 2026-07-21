#!/usr/bin/env bash
# CORE7-10 / R7-012: unpack-style smoke for the C ABI release package layout.
#
# Builds a local release package for the host target, then verifies:
#   - libdg_capi.so.2 exists and has SONAME libdg_capi.so.2 (Linux)
#   - development symlink libdg_capi.so -> libdg_capi.so.2
#   - header + C examples present
#   - exported symbols include dg_version / dg_runtime_init / dg_engine_create
#   - C11 example compiles and links against the packaged header/library
#
# Usage:
#   ./tools/package_smoke.sh [--artifact-dir DIR] [--skip-build]
set -euo pipefail

ARTIFACT_DIR="package-smoke"
SKIP_BUILD=0
TARGET="${CARGO_BUILD_TARGET:-}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --artifact-dir)
            ARTIFACT_DIR="$2"
            shift 2
            ;;
        --skip-build)
            SKIP_BUILD=1
            shift
            ;;
        --target)
            TARGET="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1" >&2
            echo "Usage: $0 [--artifact-dir DIR] [--skip-build] [--target TRIPLE]" >&2
            exit 1
            ;;
    esac
done

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

mkdir -p "$ARTIFACT_DIR"
HOST_TARGET="$(rustc -vV | awk '/^host:/{print $2}')"
TARGET="${TARGET:-$HOST_TARGET}"
PACKAGE="dyun-gu-local-${TARGET}"
DIST="$ARTIFACT_DIR/$PACKAGE"

if [[ "$SKIP_BUILD" -eq 0 ]]; then
    echo "[package-smoke] building dg-capi for $TARGET"
    cargo build --release --locked --target "$TARGET" -p dg-capi -p dg-cli
fi

echo "[package-smoke] assembling package at $DIST"
rm -rf "$DIST"
mkdir -p "$DIST/bin" "$DIST/lib/pkgconfig" "$DIST/include" "$DIST/examples/c" "$DIST/docs"

if [[ -f "target/$TARGET/release/dg-cli" ]]; then
    cp "target/$TARGET/release/dg-cli" "$DIST/bin/"
elif [[ -f "target/$TARGET/release/dg" ]]; then
    cp "target/$TARGET/release/dg" "$DIST/bin/"
fi

find "target/$TARGET/release" -maxdepth 1 -type f \
    \( -name 'libdg_capi.a' -o -name 'libdg_capi.so' -o -name 'libdg_capi.dylib' -o -name 'dg_capi.dll' \) \
    -exec cp {} "$DIST/lib/" \; || true

if [[ -f "$DIST/lib/libdg_capi.so" ]]; then
    mv "$DIST/lib/libdg_capi.so" "$DIST/lib/libdg_capi.so.2"
    ln -sfn libdg_capi.so.2 "$DIST/lib/libdg_capi.so"
fi

cp crates/dg-capi/include/dg_capi.h "$DIST/include/"
cp crates/dg-capi/examples/*.c "$DIST/examples/c/"
cp README.md "$DIST/docs/" 2>/dev/null || true

cat > "$DIST/lib/pkgconfig/dg-capi.pc" <<EOF
prefix=${DIST}
libdir=\${prefix}/lib
includedir=\${prefix}/include
Name: dg-capi
Description: dyun-gu stable C ABI
Version: 0.1.0
Libs: -L\${libdir} -ldg_capi
Cflags: -I\${includedir}
EOF

commit=$(git rev-parse HEAD 2>/dev/null || echo unknown)
python3 - "$DIST/manifest.json" "$PACKAGE" "$TARGET" "$commit" <<'PY'
import json, sys
path, name, target, commit = sys.argv[1:5]
json.dump({
    "name": name,
    "target": target,
    "dyun_commit": commit,
    "abi_version": "2.0",
    "soname": "libdg_capi.so.2",
}, open(path, "w"), indent=2)
PY

failures=0
report="$ARTIFACT_DIR/package-smoke-report.json"
checks=()

check() {
    local name="$1"
    local ok="$2"
    local detail="${3:-}"
    if [[ "$ok" -eq 1 ]]; then
        echo "[package-smoke] PASS $name"
        checks+=("{\"name\":\"$name\",\"result\":\"pass\",\"detail\":$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$detail")}")
    else
        echo "[package-smoke] FAIL $name: $detail" >&2
        checks+=("{\"name\":\"$name\",\"result\":\"fail\",\"detail\":$(python3 -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$detail")}")
        failures=$((failures + 1))
    fi
}

# Header + examples
[[ -f "$DIST/include/dg_capi.h" ]] && check "header_present" 1 || check "header_present" 0 "missing dg_capi.h"
[[ -f "$DIST/examples/c/basic.c" ]] && check "example_basic_present" 1 || check "example_basic_present" 0 "missing basic.c"
grep -q 'struct DgStringView' "$DIST/include/dg_capi.h" && check "header_has_string_view" 1 || check "header_has_string_view" 0 "DgStringView missing"
grep -q 'max_frame_bytes' "$DIST/include/dg_capi.h" && check "header_has_runtime_limits" 1 || check "header_has_runtime_limits" 0 "max_frame_bytes missing"

# Shared library layout
if [[ -f "$DIST/lib/libdg_capi.so.2" ]]; then
    check "soname_file_present" 1
    if command -v readelf >/dev/null 2>&1; then
        soname=$(readelf -d "$DIST/lib/libdg_capi.so.2" | awk '/SONAME/{print $5}' | tr -d '[]')
        if [[ "$soname" == "libdg_capi.so.2" ]]; then
            check "soname_value" 1 "$soname"
        else
            check "soname_value" 0 "expected libdg_capi.so.2 got ${soname:-empty}"
        fi
    else
        check "soname_value" 1 "readelf unavailable; file present only"
    fi
    if [[ -L "$DIST/lib/libdg_capi.so" ]]; then
        check "dev_symlink" 1
    else
        check "dev_symlink" 0 "libdg_capi.so is not a symlink"
    fi
    if command -v nm >/dev/null 2>&1; then
        symbols=$(nm -D --defined-only "$DIST/lib/libdg_capi.so.2" 2>/dev/null || nm -gU "$DIST/lib/libdg_capi.so.2" 2>/dev/null || true)
        for sym in dg_version dg_runtime_init dg_engine_create dg_tensor_create dg_abi_version; do
            if echo "$symbols" | grep -q "$sym"; then
                check "symbol_$sym" 1
            else
                check "symbol_$sym" 0 "missing exported symbol $sym"
            fi
        done
    fi
elif [[ -f "$DIST/lib/libdg_capi.a" ]]; then
    check "soname_file_present" 1 "static archive only (no cdylib on this target)"
else
    check "soname_file_present" 0 "no libdg_capi.so.2 or .a found"
fi

# Compile + link C11 example against the package (host only)
lib_present=0
if [[ -f "$DIST/lib/libdg_capi.so.2" || -f "$DIST/lib/libdg_capi.so" || -f "$DIST/lib/libdg_capi.a" ]]; then
    lib_present=1
fi
if [[ "$TARGET" == "$HOST_TARGET" && "$lib_present" -eq 1 ]]; then
    if command -v cc >/dev/null 2>&1; then
        out_bin="$ARTIFACT_DIR/basic_smoke"
        if cc -std=c11 -Wall -Werror \
            -I"$DIST/include" \
            "$DIST/examples/c/basic.c" \
            -L"$DIST/lib" -ldg_capi \
            -Wl,-rpath,"$DIST/lib" \
            -o "$out_bin" 2>"$ARTIFACT_DIR/basic_compile.err"; then
            check "c11_compile_link" 1
            if "$out_bin" >"$ARTIFACT_DIR/basic_run.out" 2>"$ARTIFACT_DIR/basic_run.err"; then
                check "c11_run" 1
            else
                check "c11_run" 0 "$(head -c 200 "$ARTIFACT_DIR/basic_run.err" || true)"
            fi
        else
            check "c11_compile_link" 0 "$(head -c 400 "$ARTIFACT_DIR/basic_compile.err" || true)"
        fi
    else
        check "c11_compile_link" 1 "cc unavailable; skipped"
    fi
else
    check "c11_compile_link" 1 "cross target or no shared lib; compile skipped"
fi

python3 - "$report" "$failures" "${checks[@]}" <<'PY'
import json, sys
path = sys.argv[1]
failures = int(sys.argv[2])
checks = [json.loads(item) for item in sys.argv[3:]]
summary = {
    "failures": failures,
    "passed": sum(1 for c in checks if c["result"] == "pass"),
    "checks": checks,
}
with open(path, "w") as f:
    json.dump(summary, f, indent=2)
print(json.dumps(summary, indent=2))
PY

if [[ "$failures" -ne 0 ]]; then
    echo "[package-smoke] FAILED with $failures check(s); report=$report" >&2
    exit 1
fi
echo "[package-smoke] OK report=$report"
