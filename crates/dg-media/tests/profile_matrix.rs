//! Profile name and feature matrix tests (plan 13 §6, plan 14 §2).
#![cfg(feature = "avcodec-sdk")]

use dg_media::{
    compiled_profiles, reject_profile_hw_conflict, resolve_profile, AvcodecProfile,
    ProfileSupportLevel,
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
fn compiled_profiles_map_to_sdk_profiles() {
    for profile in compiled_profiles() {
        profile.to_sdk().expect("sdk profile");
    }
}

#[test]
fn legacy_avcodec_alias_resolves_native_free() {
    let result = resolve_profile(None, true);
    #[cfg(feature = "avcodec-profile-native-free")]
    assert_eq!(result, Ok(AvcodecProfile::NativeFree));
}

#[test]
fn multi_profile_requires_explicit_choice() {
    let compiled = compiled_profiles();
    if compiled.len() > 1 {
        let err = resolve_profile(None, false).expect_err("ambiguous");
        assert!(err.to_string().contains("multiple avcodec profiles"));
        // Explicit selection must still succeed for every compiled profile.
        for profile in compiled {
            assert_eq!(resolve_profile(Some(profile.name()), false), Ok(profile));
        }
    }
}

#[test]
fn unverified_hardware_is_not_production() {
    for profile in [
        AvcodecProfile::RkmppHost,
        AvcodecProfile::RkmppZeroCopy,
        AvcodecProfile::OnevplHost,
        AvcodecProfile::AmfHost,
    ] {
        assert_eq!(profile.support_level(), ProfileSupportLevel::Unverified);
        assert!(!profile.is_production_supported());
    }
    assert_eq!(
        AvcodecProfile::NativeFree.support_level(),
        ProfileSupportLevel::Production
    );
}
