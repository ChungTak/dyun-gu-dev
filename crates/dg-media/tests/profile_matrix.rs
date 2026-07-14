//! Profile name and feature matrix tests (plan 13 §6, plan 14 §2).
#![cfg(feature = "avcodec-sdk")]

use dg_media::{
    compiled_profiles, profile_to_sdk_descriptor, reject_profile_hw_conflict, resolve_profile,
    AvcodecProfile,
};

#[test]
fn stable_profile_names_match_cargo_features() {
    let cases = [
        ("native-free", "avcodec-profile-native-free"),
        ("software", "avcodec-profile-software"),
        ("rkmpp-host", "avcodec-profile-rkmpp-host"),
        ("rkmpp-zero-copy", "avcodec-profile-rkmpp-zero-copy"),
        ("nvcodec-host", "avcodec-profile-nvcodec-host"),
        (
            "nvcodec-device-frame",
            "avcodec-profile-nvcodec-device-frame",
        ),
        ("onevpl-host", "avcodec-profile-onevpl-host"),
        ("amf-host", "avcodec-profile-amf-host"),
    ];
    for (name, feature) in cases {
        let profile = AvcodecProfile::parse(name).expect("parse profile");
        assert_eq!(profile.name(), name);
        assert_eq!(profile.cargo_feature(), feature);
    }
}

#[test]
fn profile_hw_conflict_is_rejected() {
    let err = reject_profile_hw_conflict(Some("software"), Some("auto")).expect_err("conflict");
    assert!(err.to_string().contains("profile"));
}

#[test]
fn compiled_profiles_are_valid_descriptors() {
    for profile in compiled_profiles() {
        profile_to_sdk_descriptor(profile).expect("descriptor");
    }
}

#[test]
fn legacy_avcodec_alias_resolves_native_free() {
    let result = resolve_profile(None, true);
    #[cfg(feature = "avcodec-profile-native-free")]
    assert_eq!(result, Ok(AvcodecProfile::NativeFree));
}
