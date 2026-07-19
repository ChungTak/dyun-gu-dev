//! ABI snapshot guard for the committed `include/dg_capi.h`.
//!
//! Plan 12/13 require C header changes to be intentional. This test fails if a
//! previously published symbol disappears from the generated header.

use std::fs;
use std::path::PathBuf;

fn header_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("include/dg_capi.h")
}

fn snapshot_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/abi_snapshot.txt")
}

fn load_required_symbols() -> Vec<String> {
    fs::read_to_string(snapshot_path())
        .expect("read abi_snapshot.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

#[test]
fn committed_header_contains_required_abi_symbols() {
    let header = fs::read_to_string(header_path()).expect("read include/dg_capi.h");
    let required = load_required_symbols();
    assert!(
        !required.is_empty(),
        "abi_snapshot.txt must list at least one symbol"
    );

    let mut missing = Vec::new();
    for symbol in &required {
        // Match free functions or enum/struct type tags.
        let present = header.contains(&format!("{symbol}("))
            || header.contains(&format!("enum {symbol}"))
            || header.contains(&format!("struct {symbol}"))
            || header.contains(&format!("typedef enum {symbol}"))
            || header.contains(&format!("}} {symbol};"))
            || header.contains(&format!("typedef struct {symbol}"));
        if !present {
            // Fallback: bare name for opaque typedefs like `typedef struct DgEngine DgEngine;`
            if !header.contains(symbol.as_str()) {
                missing.push(symbol.clone());
            }
        }
    }
    assert!(
        missing.is_empty(),
        "dg_capi.h is missing committed ABI symbols: {missing:?}\n\
         If this is intentional, update tests/abi_snapshot.txt and document the break."
    );
}

#[test]
fn status_enum_numeric_values_are_stable() {
    let header = fs::read_to_string(header_path()).expect("read header");
    // These values are part of the published C ABI.
    for needle in [
        "Ok = 0",
        "Again = 1",
        "EndOfStream = 2",
        "Busy = 3",
        "InvalidArgument = -1",
        "NullPointer = -2",
        "ParseError = -4",
        "RuntimeError = -6",
        "Unsupported = -7",
        "Panic = -8",
    ] {
        assert!(
            header.contains(needle),
            "DgStatus value missing or changed: {needle}"
        );
    }
}

#[test]
fn version_and_owned_handle_symbols_remain_exported() {
    let header = fs::read_to_string(header_path()).expect("read header");
    assert!(header.contains("const char *dg_version(void)"));
    assert!(header.contains("struct DgAbiVersion"));
    assert!(header.contains("dg_abi_version("));
    for symbol in [
        "DgError",
        "DgOwnedBytes",
        "dg_error_status",
        "dg_error_category",
        "dg_error_operation",
        "dg_error_message",
        "dg_error_free",
        "dg_owned_bytes_data",
        "dg_owned_bytes_len",
        "dg_owned_bytes_free",
    ] {
        assert!(header.contains(symbol), "dg_capi.h must export {symbol}");
    }
}
