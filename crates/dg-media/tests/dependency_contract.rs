//! Dependency tree contract tests for avcodec profile features (plan 14 §3).

use std::process::Command;

#[cfg(any(
    feature = "avcodec-profile-native-free",
    feature = "avcodec-profile-software"
))]
fn cargo_tree(features: &str) -> String {
    let output = Command::new("cargo")
        .args([
            "tree",
            "-p",
            "dg-media",
            "--features",
            features,
            "-e",
            "features",
        ])
        .output()
        .expect("cargo tree must run");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
#[cfg(feature = "avcodec-profile-native-free")]
fn native_free_tree_excludes_ffmpeg_and_hardware_backends() {
    let tree = cargo_tree("avcodec-profile-native-free");
    for forbidden in [
        "ffmpeg", "rkmpp", "nvcodec", "onevpl", "amf", "x264", "x265", "openh264",
    ] {
        assert!(
            !tree.contains(forbidden),
            "native-free tree must not contain `{forbidden}`\n{tree}"
        );
    }
}

#[test]
fn default_workspace_does_not_activate_avcodec() {
    let output = Command::new("cargo")
        .args(["tree", "-p", "dg-media", "-e", "features"])
        .output()
        .expect("cargo tree");
    assert!(output.status.success());
    let tree = String::from_utf8_lossy(&output.stdout);
    assert!(
        !tree.contains("avcodec v"),
        "default dg-media build must not pull avcodec crate\n{tree}"
    );
}

#[test]
#[cfg(feature = "avcodec-sdk")]
fn dg_media_avcodec_is_only_direct_codec_dependency() {
    let manifest = include_str!("../Cargo.toml");
    assert!(
        !manifest.contains("dg-media-avcodec") || manifest.contains("optional = true"),
        "dg-media must treat dg-media-avcodec as optional"
    );
    let avcodec_manifest = include_str!("../../dg-media-avcodec/Cargo.toml");
    // Plan 4 RC3 (0.2.0-rc.3): UP4-002 Software FFmpeg 4/5 + jpeg allow-list.
    assert!(
        avcodec_manifest.contains("rev = \"3f80f558e48ced6d3dc2c1e067307bfd12bec89d\""),
        "dg-media-avcodec must pin 3f80f55 0.2.0-rc.3 revision"
    );
    assert!(
        avcodec_manifest.contains("profile-native-free"),
        "profile features must forward upstream profile-* presets"
    );
    // Direct dependency must be the curated avcodec package only.
    assert!(
        avcodec_manifest.contains("package = \"avcodec\""),
        "facade must depend on package avcodec"
    );
    for low_level in [
        "avcodec-backend-",
        "avcodec-codec-",
        "ffmpeg-sys",
        "x264-sys",
    ] {
        assert!(
            !avcodec_manifest.contains(low_level),
            "dg-media-avcodec must not list low-level codec crate `{low_level}`"
        );
    }
}

#[test]
#[cfg(feature = "avcodec-profile-software")]
fn software_profile_forwards_profile_software_not_software_default() {
    let tree = cargo_tree("avcodec-profile-software");
    assert!(
        tree.contains("profile-software") || tree.contains("avcodec"),
        "software profile must activate avcodec\n{tree}"
    );
    assert!(
        !tree.contains("software-default"),
        "software profile must not use legacy software-default feature\n{tree}"
    );
}
