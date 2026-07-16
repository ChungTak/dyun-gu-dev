//! Deprecated hardware preference parsing for one release cycle of `hw` compatibility.
//!
//! Production session creation always goes through [`crate::session::AvcodecSdkService`]
//! (V3 `VideoSdk`). This module only maps legacy `hw=` strings to modern Profile names.
//!
//! **Removal schedule (plan 13):** `hw` is deprecated since `0.1.0` and will be removed in
//! `0.2.0`. Migrate configs to explicit `profile=` before that release.

#![allow(deprecated)]

use dg_core::{Error, Result};

use crate::profile::AvcodecProfile;

/// Deprecated hardware preference used by legacy `hw` element parameters.
///
/// Scheduled for removal in **0.2.0** (deprecated since 0.1.0).
#[deprecated(
    since = "0.1.0",
    note = "use `profile` with `avcodec-profile-*` features instead of `hw`; removed in 0.2.0"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HwPreference {
    Auto,
    Rockchip,
    Nvidia,
    Intel,
    Amd,
    Software,
}

impl HwPreference {
    pub fn parse(value: Option<&str>) -> Result<Self> {
        match value.unwrap_or("auto").to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "rk" | "rockchip" | "rknn" | "rknpu" => Ok(Self::Rockchip),
            "nv" | "nvidia" | "cuda" => Ok(Self::Nvidia),
            "intel" | "vaapi" => Ok(Self::Intel),
            "amd" | "amf" => Ok(Self::Amd),
            "sw" | "software" | "cpu" | "none" => Ok(Self::Software),
            other => Err(Error::Config(format!(
                "hw must be one of `auto`, `rk`, `rockchip`, `rknn`, `rknpu`, `nv`, `nvidia`, \
                 `cuda`, `intel`, `vaapi`, `amd`, `amf`, `sw`, `software`, `cpu`, or `none`, \
                 got `{other}`"
            ))),
        }
    }
}

/// Maps a legacy hardware preference to the closest modern profile and warning text.
#[must_use]
pub fn resolve_legacy_hw(hw: HwPreference) -> (Option<AvcodecProfile>, &'static str) {
    match hw {
        HwPreference::Auto => (
            Some(AvcodecProfile::Software),
            "legacy hw=auto no longer auto-selects hardware; use profile=software or an \
             explicit avcodec-profile-* feature",
        ),
        HwPreference::Rockchip => (
            Some(AvcodecProfile::RkmppHost),
            "legacy hw=rockchip is deprecated; use profile=rkmpp-host with feature \
             avcodec-profile-rkmpp-host",
        ),
        HwPreference::Nvidia => (
            Some(AvcodecProfile::NvcodecHost),
            "legacy hw=nvidia/cuda is deprecated; use profile=nvcodec-host with feature \
             avcodec-profile-nvcodec-host (not device-frame)",
        ),
        HwPreference::Intel => (
            Some(AvcodecProfile::OnevplHost),
            "legacy hw=intel/vaapi is deprecated; use profile=onevpl-host with feature \
             avcodec-profile-onevpl-host",
        ),
        HwPreference::Amd => (
            Some(AvcodecProfile::AmfHost),
            "legacy hw=amd/amf is deprecated; use profile=amf-host with feature \
             avcodec-profile-amf-host",
        ),
        HwPreference::Software => (
            Some(AvcodecProfile::Software),
            "legacy hw=software is deprecated; use profile=software with feature \
             avcodec-profile-software",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{resolve_legacy_hw, HwPreference};
    use crate::profile::AvcodecProfile;

    #[test]
    fn hardware_preference_accepts_aliases() {
        assert_eq!(HwPreference::parse(Some("rk")), Ok(HwPreference::Rockchip));
        assert_eq!(HwPreference::parse(Some("CUDA")), Ok(HwPreference::Nvidia));
        assert_eq!(HwPreference::parse(Some("cpu")), Ok(HwPreference::Software));
        assert!(HwPreference::parse(Some("mystery")).is_err());
    }

    #[test]
    fn legacy_auto_maps_software_with_warning() {
        let (profile, warning) = resolve_legacy_hw(HwPreference::Auto);
        assert_eq!(profile, Some(AvcodecProfile::Software));
        assert!(warning.contains("hw=auto"));
    }

    #[test]
    fn legacy_cuda_maps_nvcodec_host_not_device_frame() {
        assert_eq!(HwPreference::parse(Some("cuda")), Ok(HwPreference::Nvidia));
        let (profile, warning) = resolve_legacy_hw(HwPreference::Nvidia);
        assert_eq!(profile, Some(AvcodecProfile::NvcodecHost));
        assert!(warning.contains("not device-frame"));
    }
}
