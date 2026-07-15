//! Production source scan for forbidden backend-selection and V2 assembly patterns
//! (plan 03 §INT3-04).
#![cfg(feature = "avcodec-sdk")]

use std::fs;
use std::path::Path;

const FORBIDDEN: &[&str] = &[
    "create_decoder_with_trace",
    "create_encoder_with_trace",
    "BackendSelectionPolicy::Required",
    "BackendSelectionPolicy::Ordered",
    "AvcodecSessionBuilder",
    // Plan 03 source guard: no V2 assembly or legacy registry leaking allowed.
    "default_registry_builder",
    "RegistryBuilder",
    "VideoSessionFactoryV2",
    "VideoBackendPolicy",
    "VideoProfileDescriptor",
    "VideoIoMemoryPlan",
    "VideoTranscoderRequest",
    "leak_registry",
];

fn read_sources() -> String {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut combined = String::new();
    for entry in std::fs::read_dir(src_dir).expect("read src dir") {
        let entry = entry.expect("entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            let name = path.file_name().unwrap().to_string_lossy();
            // legacy.rs is allowed to keep deprecated selection samples.
            if name == "legacy.rs" {
                continue;
            }
            combined.push_str(&fs::read_to_string(&path).unwrap_or_default());
            combined.push('\n');
        }
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
