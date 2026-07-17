//! Runtime avcodec profile selection and thin mapping to the upstream V3 [`VideoProfile`].

use dg_core::{DeviceKind, Error, Result};

/// Production support level advertised by dyun for a compiled profile.
///
/// Unverified hardware profiles may still compile and run for development, but must not be
/// advertised as production-supported until upstream signs them off on real hardware.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProfileSupportLevel {
    /// First-party production matrix (NativeFree, Software, NV Host / Device-frame).
    Production,
    /// Compiled for contract/CI only; no production SLA (RKMPP, OneVPL, AMF).
    Unverified,
}

impl ProfileSupportLevel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Unverified => "unverified",
        }
    }
}

/// Stable runtime profile name matching `avcodec-profile-*` Cargo feature suffixes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AvcodecProfile {
    NativeFree,
    Software,
    RkmppHost,
    RkmppHostFallback,
    RkmppZeroCopy,
    NvcodecHost,
    NvcodecHostFallback,
    NvcodecDeviceFrame,
    OnevplHost,
    OnevplHostFallback,
    AmfHost,
    AmfHostFallback,
}

impl AvcodecProfile {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NativeFree => "native-free",
            Self::Software => "software",
            Self::RkmppHost => "rkmpp-host",
            Self::RkmppHostFallback => "rkmpp-host-fallback",
            Self::RkmppZeroCopy => "rkmpp-zero-copy",
            Self::NvcodecHost => "nvcodec-host",
            Self::NvcodecHostFallback => "nvcodec-host-fallback",
            Self::NvcodecDeviceFrame => "nvcodec-device-frame",
            Self::OnevplHost => "onevpl-host",
            Self::OnevplHostFallback => "onevpl-host-fallback",
            Self::AmfHost => "amf-host",
            Self::AmfHostFallback => "amf-host-fallback",
        }
    }

    #[must_use]
    pub const fn cargo_feature(self) -> &'static str {
        match self {
            Self::NativeFree => "avcodec-profile-native-free",
            Self::Software => "avcodec-profile-software",
            Self::RkmppHost => "avcodec-profile-rkmpp-host",
            Self::RkmppHostFallback => "avcodec-profile-rkmpp-host-fallback",
            Self::RkmppZeroCopy => "avcodec-profile-rkmpp-zero-copy",
            Self::NvcodecHost => "avcodec-profile-nvcodec-host",
            Self::NvcodecHostFallback => "avcodec-profile-nvcodec-host-fallback",
            Self::NvcodecDeviceFrame => "avcodec-profile-nvcodec-device-frame",
            Self::OnevplHost => "avcodec-profile-onevpl-host",
            Self::OnevplHostFallback => "avcodec-profile-onevpl-host-fallback",
            Self::AmfHost => "avcodec-profile-amf-host",
            Self::AmfHostFallback => "avcodec-profile-amf-host-fallback",
        }
    }

    pub fn parse(name: &str) -> Result<Self> {
        match name.to_ascii_lowercase().as_str() {
            "native-free" => Ok(Self::NativeFree),
            "software" => Ok(Self::Software),
            "rkmpp-host" => Ok(Self::RkmppHost),
            "rkmpp-host-fallback" => Ok(Self::RkmppHostFallback),
            "rkmpp-zero-copy" => Ok(Self::RkmppZeroCopy),
            "nvcodec-host" => Ok(Self::NvcodecHost),
            "nvcodec-host-fallback" => Ok(Self::NvcodecHostFallback),
            "nvcodec-device-frame" => Ok(Self::NvcodecDeviceFrame),
            "onevpl-host" => Ok(Self::OnevplHost),
            "onevpl-host-fallback" => Ok(Self::OnevplHostFallback),
            "amf-host" => Ok(Self::AmfHost),
            "amf-host-fallback" => Ok(Self::AmfHostFallback),
            other => Err(Error::Config(format!("unknown avcodec profile `{other}`"))),
        }
    }

    #[must_use]
    pub const fn is_compiled(self) -> bool {
        match self {
            Self::NativeFree => cfg!(feature = "avcodec-profile-native-free"),
            Self::Software => cfg!(feature = "avcodec-profile-software"),
            Self::RkmppHost => cfg!(feature = "avcodec-profile-rkmpp-host"),
            Self::RkmppHostFallback => cfg!(feature = "avcodec-profile-rkmpp-host-fallback"),
            Self::RkmppZeroCopy => cfg!(feature = "avcodec-profile-rkmpp-zero-copy"),
            Self::NvcodecHost => cfg!(feature = "avcodec-profile-nvcodec-host"),
            Self::NvcodecHostFallback => cfg!(feature = "avcodec-profile-nvcodec-host-fallback"),
            Self::NvcodecDeviceFrame => cfg!(feature = "avcodec-profile-nvcodec-device-frame"),
            Self::OnevplHost => cfg!(feature = "avcodec-profile-onevpl-host"),
            Self::OnevplHostFallback => cfg!(feature = "avcodec-profile-onevpl-host-fallback"),
            Self::AmfHost => cfg!(feature = "avcodec-profile-amf-host"),
            Self::AmfHostFallback => cfg!(feature = "avcodec-profile-amf-host-fallback"),
        }
    }

    /// Returns whether this profile exposes an image processor stage (resize/CSC).
    #[must_use]
    pub const fn supports_image_processor(self) -> bool {
        !matches!(self, Self::NvcodecDeviceFrame)
    }

    /// AMF host profiles do not guarantee decode; only fallback may reach software decode.
    #[must_use]
    pub const fn supports_amf_decode(self) -> bool {
        matches!(self, Self::AmfHostFallback)
    }

    /// Production support status for capability/CLI/docs (INT3-14).
    ///
    /// NativeFree is a development-only pure-Rust image path and must not be advertised as
    /// production. RKMPP / OneVPL / AMF remain `Unverified` until upstream provides real-device
    /// sign-off. NV Host / Device-frame are marked production in the plan matrix; runtime hardware
    /// verification is still environment-dependent.
    #[must_use]
    pub const fn support_level(self) -> ProfileSupportLevel {
        match self {
            Self::Software
            | Self::NvcodecHost
            | Self::NvcodecHostFallback
            | Self::NvcodecDeviceFrame => ProfileSupportLevel::Production,
            Self::NativeFree
            | Self::RkmppHost
            | Self::RkmppHostFallback
            | Self::RkmppZeroCopy
            | Self::OnevplHost
            | Self::OnevplHostFallback
            | Self::AmfHost
            | Self::AmfHostFallback => ProfileSupportLevel::Unverified,
        }
    }

    /// Convenience: `true` only for profiles on the first-party production matrix.
    #[must_use]
    pub const fn is_production_supported(self) -> bool {
        matches!(self.support_level(), ProfileSupportLevel::Production)
    }

    pub fn ensure_compiled(self) -> Result<()> {
        if !self.is_compiled() {
            return Err(Error::Config(format!(
                "profile `{}` is not compiled; enable cargo feature `{}`",
                self.name(),
                self.cargo_feature()
            )));
        }
        Ok(())
    }

    #[cfg(feature = "avcodec-sdk")]
    /// Maps this dyun profile to the upstream V3 [`VideoProfile`] one-to-one.
    ///
    /// NativeFree maps to [`dg_media_avcodec::VideoProfile::NativeFree`] and is never silently
    /// rewritten to Software.
    pub fn to_sdk(self) -> Result<dg_media_avcodec::VideoProfile> {
        self.ensure_compiled()?;
        use dg_media_avcodec::VideoProfile;
        let profile = match self {
            Self::NativeFree => VideoProfile::NativeFree,
            Self::Software => VideoProfile::Software,
            Self::RkmppHost => VideoProfile::RkmppHost,
            Self::RkmppHostFallback => VideoProfile::RkmppHostFallback,
            Self::RkmppZeroCopy => VideoProfile::RkmppZeroCopy,
            Self::NvcodecHost => VideoProfile::NvcodecHost,
            Self::NvcodecHostFallback => VideoProfile::NvcodecHostFallback,
            Self::NvcodecDeviceFrame => VideoProfile::NvcodecDeviceFrame,
            Self::OnevplHost => VideoProfile::OnevplHost,
            Self::OnevplHostFallback => VideoProfile::OnevplHostFallback,
            Self::AmfHost => VideoProfile::AmfHost,
            Self::AmfHostFallback => VideoProfile::AmfHostFallback,
        };
        Ok(profile)
    }
}

/// Returns every profile compiled into the current binary.
#[must_use]
#[allow(clippy::vec_init_then_push)]
pub fn compiled_profiles() -> Vec<AvcodecProfile> {
    let mut profiles = Vec::new();
    #[cfg(feature = "avcodec-profile-native-free")]
    profiles.push(AvcodecProfile::NativeFree);
    #[cfg(feature = "avcodec-profile-software")]
    profiles.push(AvcodecProfile::Software);
    #[cfg(feature = "avcodec-profile-rkmpp-host")]
    profiles.push(AvcodecProfile::RkmppHost);
    #[cfg(feature = "avcodec-profile-rkmpp-host-fallback")]
    profiles.push(AvcodecProfile::RkmppHostFallback);
    #[cfg(feature = "avcodec-profile-rkmpp-zero-copy")]
    profiles.push(AvcodecProfile::RkmppZeroCopy);
    #[cfg(feature = "avcodec-profile-nvcodec-host")]
    profiles.push(AvcodecProfile::NvcodecHost);
    #[cfg(feature = "avcodec-profile-nvcodec-host-fallback")]
    profiles.push(AvcodecProfile::NvcodecHostFallback);
    #[cfg(feature = "avcodec-profile-nvcodec-device-frame")]
    profiles.push(AvcodecProfile::NvcodecDeviceFrame);
    #[cfg(feature = "avcodec-profile-onevpl-host")]
    profiles.push(AvcodecProfile::OnevplHost);
    #[cfg(feature = "avcodec-profile-onevpl-host-fallback")]
    profiles.push(AvcodecProfile::OnevplHostFallback);
    #[cfg(feature = "avcodec-profile-amf-host")]
    profiles.push(AvcodecProfile::AmfHost);
    #[cfg(feature = "avcodec-profile-amf-host-fallback")]
    profiles.push(AvcodecProfile::AmfHostFallback);
    profiles
}

/// Rejects graph configs that specify both the new `profile` field and legacy `hw`.
pub fn reject_profile_hw_conflict(profile: Option<&str>, hw: Option<&str>) -> Result<()> {
    if profile.is_some() && hw.is_some() {
        return Err(Error::Config(
            "`profile` and legacy `hw` cannot be used together; remove `hw` and select an \
             `avcodec-profile-*` feature"
                .to_string(),
        ));
    }
    Ok(())
}

/// Resolves the active profile from explicit config or compiled defaults.
pub fn resolve_profile(name: Option<&str>, legacy_avcodec_only: bool) -> Result<AvcodecProfile> {
    if let Some(name) = name {
        let profile = AvcodecProfile::parse(name)?;
        profile.ensure_compiled()?;
        return Ok(profile);
    }

    if legacy_avcodec_only {
        // The legacy `avcodec` feature now maps to the software fallback profile.
        let profile = AvcodecProfile::Software;
        profile.ensure_compiled()?;
        return Ok(profile);
    }

    let compiled = compiled_profiles();
    match compiled.as_slice() {
        [] => Err(Error::Config(
            "no avcodec profile compiled; enable an `avcodec-profile-*` feature".to_string(),
        )),
        [single] => Ok(*single),
        many => {
            let names = many
                .iter()
                .map(|profile| profile.name())
                .collect::<Vec<_>>()
                .join(", ");
            Err(Error::Config(format!(
                "multiple avcodec profiles compiled [{names}]; specify `profile` explicitly"
            )))
        }
    }
}

/// Maps an inference device name to the preferred avcodec profile.
///
/// Returns `None` when the device is unknown or has no matching codec hardware.
#[must_use]
pub fn resolve_profile_from_device(device: &str) -> Option<AvcodecProfile> {
    let kind = device_kind_from_name(device)?;
    let profile = match kind {
        DeviceKind::Cpu | DeviceKind::IntelNpu => AvcodecProfile::Software,
        DeviceKind::IntelGpu => AvcodecProfile::OnevplHostFallback,
        DeviceKind::CudaGpu => AvcodecProfile::NvcodecHostFallback,
        DeviceKind::RknnNpu => AvcodecProfile::RkmppHostFallback,
        DeviceKind::SophonTpu => return None,
    };
    Some(profile)
}

fn device_kind_from_name(value: &str) -> Option<DeviceKind> {
    match value.to_ascii_lowercase().as_str() {
        "cpu" => Some(DeviceKind::Cpu),
        "intel_gpu" | "intel-gpu" | "igpu" => Some(DeviceKind::IntelGpu),
        "intel_npu" | "intel-npu" | "npu" => Some(DeviceKind::IntelNpu),
        "cuda" | "cuda_gpu" | "cuda-gpu" | "nv" | "nvidia" => Some(DeviceKind::CudaGpu),
        "rknn" | "rknn_npu" | "rknn-npu" | "rk" | "rockchip" => Some(DeviceKind::RknnNpu),
        "sophon" | "sophon_tpu" | "sophon-tpu" => Some(DeviceKind::SophonTpu),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compiled_profiles, reject_profile_hw_conflict, resolve_profile,
        resolve_profile_from_device, AvcodecProfile,
    };

    #[cfg(feature = "avcodec-sdk")]
    use dg_media_avcodec::VideoProfile;

    #[test]
    fn nv_device_frame_has_no_processor() {
        assert!(!AvcodecProfile::NvcodecDeviceFrame.supports_image_processor());
        assert!(AvcodecProfile::NativeFree.supports_image_processor());
    }

    #[test]
    fn amf_host_does_not_guarantee_decode() {
        assert!(!AvcodecProfile::AmfHost.supports_amf_decode());
        assert!(AvcodecProfile::AmfHostFallback.supports_amf_decode());
    }

    #[test]
    fn production_matrix_and_unverified_hardware() {
        use super::ProfileSupportLevel;
        assert_eq!(
            AvcodecProfile::Software.support_level(),
            ProfileSupportLevel::Production
        );
        assert_eq!(
            AvcodecProfile::NvcodecHost.support_level(),
            ProfileSupportLevel::Production
        );
        assert_eq!(
            AvcodecProfile::NvcodecDeviceFrame.support_level(),
            ProfileSupportLevel::Production
        );
        assert_eq!(
            AvcodecProfile::NativeFree.support_level(),
            ProfileSupportLevel::Unverified
        );
        assert_eq!(
            AvcodecProfile::RkmppHost.support_level(),
            ProfileSupportLevel::Unverified
        );
        assert_eq!(
            AvcodecProfile::OnevplHost.support_level(),
            ProfileSupportLevel::Unverified
        );
        assert_eq!(
            AvcodecProfile::AmfHost.support_level(),
            ProfileSupportLevel::Unverified
        );
        assert!(!AvcodecProfile::AmfHost.is_production_supported());
        assert!(!AvcodecProfile::NativeFree.is_production_supported());
    }

    #[test]
    fn parse_accepts_stable_profile_names() {
        assert_eq!(
            AvcodecProfile::parse("native-free"),
            Ok(AvcodecProfile::NativeFree)
        );
        assert_eq!(
            AvcodecProfile::parse("NVCODEC-HOST"),
            Ok(AvcodecProfile::NvcodecHost)
        );
        assert!(AvcodecProfile::parse("mystery").is_err());
    }

    #[test]
    fn reject_profile_hw_conflict_fails() {
        let error = reject_profile_hw_conflict(Some("software"), Some("auto"))
            .expect_err("profile and hw must conflict");
        assert!(error.to_string().contains("profile"));
        assert!(reject_profile_hw_conflict(None, Some("auto")).is_ok());
        assert!(reject_profile_hw_conflict(Some("software"), None).is_ok());
    }

    #[test]
    fn resolve_profile_requires_compiled_explicit_choice() {
        let result = resolve_profile(Some("software"), false);
        #[cfg(feature = "avcodec-profile-software")]
        assert_eq!(result, Ok(AvcodecProfile::Software));
        #[cfg(not(feature = "avcodec-profile-software"))]
        {
            let error = result.expect_err("software profile must be unavailable");
            assert!(error.to_string().contains("not compiled"));
        }
    }

    #[test]
    fn resolve_profile_legacy_avcodec_maps_software() {
        let result = resolve_profile(None, true);
        #[cfg(feature = "avcodec-profile-software")]
        assert_eq!(result, Ok(AvcodecProfile::Software));
        #[cfg(not(feature = "avcodec-profile-software"))]
        assert!(result.is_err());
    }

    #[test]
    fn resolve_profile_from_device_maps_inference_hardware() {
        assert_eq!(
            resolve_profile_from_device("intel_gpu"),
            Some(AvcodecProfile::OnevplHostFallback)
        );
        assert_eq!(
            resolve_profile_from_device("CPU"),
            Some(AvcodecProfile::Software)
        );
        assert_eq!(
            resolve_profile_from_device("cuda"),
            Some(AvcodecProfile::NvcodecHostFallback)
        );
        assert_eq!(
            resolve_profile_from_device("rknn_npu"),
            Some(AvcodecProfile::RkmppHostFallback)
        );
        assert_eq!(resolve_profile_from_device("sophon"), None);
        assert_eq!(resolve_profile_from_device("unknown"), None);
    }

    #[test]
    fn resolve_profile_defaults_to_single_compiled_profile() {
        let compiled = compiled_profiles();
        if compiled.len() == 1 {
            assert_eq!(resolve_profile(None, false), Ok(compiled[0]));
        }
    }

    #[test]
    fn resolve_profile_requires_explicit_choice_for_multiple_profiles() {
        let compiled = compiled_profiles();
        if compiled.len() > 1 {
            let error = resolve_profile(None, false).expect_err("ambiguous default");
            assert!(error.to_string().contains("multiple avcodec profiles"));
        }
    }

    #[test]
    #[cfg(all(feature = "avcodec-sdk", feature = "avcodec-profile-native-free"))]
    fn native_free_maps_to_native_free_sdk_profile() {
        let sdk = AvcodecProfile::NativeFree
            .to_sdk()
            .expect("native-free compiled");
        assert!(matches!(sdk, VideoProfile::NativeFree));
    }

    #[test]
    #[cfg(all(feature = "avcodec-sdk", feature = "avcodec-profile-software"))]
    fn software_maps_to_software_sdk_profile() {
        let sdk = AvcodecProfile::Software
            .to_sdk()
            .expect("software compiled");
        assert!(matches!(sdk, VideoProfile::Software));
    }

    #[test]
    fn uncompiled_profile_errors_with_feature_hint() {
        // Pick a profile unlikely to be default-enabled.
        let target = AvcodecProfile::OnevplHost;
        if target.is_compiled() {
            return;
        }
        let error = target.to_sdk().expect_err("uncompiled profile must fail");
        assert!(error.to_string().contains("not compiled"));
        assert!(error.to_string().contains(target.cargo_feature()));
    }

    #[test]
    #[cfg(feature = "avcodec-sdk")]
    fn all_compiled_profiles_map_one_to_one_to_sdk() {
        for profile in compiled_profiles() {
            let sdk = profile.to_sdk().expect("compiled profile must map");
            assert_eq!(profile.name(), sdk.name());
        }
    }
}
