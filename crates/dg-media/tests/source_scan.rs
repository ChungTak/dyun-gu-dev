//! Production source scan for forbidden backend-selection patterns (plan 02 §4).
#![cfg(feature = "avcodec-sdk")]

use std::fs;
use std::path::Path;

const FORBIDDEN: &[&str] = &[
    "create_decoder_with_trace",
    "create_encoder_with_trace",
    "BackendSelectionPolicy::Required",
    "BackendSelectionPolicy::Ordered",
    "AvcodecSessionBuilder",
];

/// Production modules that must not re-implement SDK backend selection.
const SCAN_ROOTS: &[&str] = &[
    "src/avcodec.rs",
    "src/session.rs",
    "src/profile.rs",
    "src/transcoder.rs",
    "src/diagnostics.rs",
    "src/elements.rs",
    "src/bridge.rs",
    "src/legacy.rs",
];

fn read_sources() -> String {
    let mut combined = String::new();
    for rel in SCAN_ROOTS {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
        combined.push_str(&fs::read_to_string(&path).unwrap_or_default());
        combined.push('\n');
    }
    combined
}

#[test]
fn production_sources_avoid_manual_backend_selection() {
    let sources = read_sources();
    for pattern in FORBIDDEN {
        assert!(
            !sources.contains(pattern),
            "forbidden pattern `{pattern}` found in scanned production sources"
        );
    }
}

#[test]
fn production_sources_do_not_embed_backend_id_candidate_arrays() {
    let sources = read_sources();
    // Profile names are allowed in strings; raw backend id arrays used for selection are not.
    for pattern in [
        "candidates: &[",
        "backend_candidates(",
        "select_with_hints",
        "map_selection_failure",
        "apply_profile_",
    ] {
        assert!(
            !sources.contains(pattern),
            "forbidden selection pattern `{pattern}` found outside legacy module"
        );
    }
}

#[test]
fn legacy_module_is_isolated() {
    let legacy = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/legacy.rs"))
        .expect("legacy.rs");
    assert!(
        legacy.contains("deprecated"),
        "legacy backend loops must remain behind deprecation gate"
    );
}
