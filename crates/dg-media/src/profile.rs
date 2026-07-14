//! Runtime avcodec profile selection and SDK Profile V2 mapping.

use dg_core::{Error, Result};
use dg_media_avcodec::{
    MemoryDomain, ProfileName, VideoBackendPolicy, VideoIoMemoryPlan, VideoProfileDescriptor,
};

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
        let profile = AvcodecProfile::NativeFree;
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

fn backend_policy(profile: AvcodecProfile) -> VideoBackendPolicy {
    match profile {
        AvcodecProfile::NativeFree | AvcodecProfile::Software => VideoBackendPolicy::software(),
        AvcodecProfile::RkmppHost => VideoBackendPolicy::rkmpp_host(false),
        AvcodecProfile::RkmppHostFallback => VideoBackendPolicy::rkmpp_host(true),
        AvcodecProfile::RkmppZeroCopy => VideoBackendPolicy::rkmpp_zero_copy(),
        AvcodecProfile::NvcodecHost => VideoBackendPolicy::nvcodec_host(false),
        AvcodecProfile::NvcodecHostFallback => VideoBackendPolicy::nvcodec_host(true),
        AvcodecProfile::NvcodecDeviceFrame => VideoBackendPolicy::nvcodec_device_frame(),
        AvcodecProfile::OnevplHost => VideoBackendPolicy::onevpl_host(false),
        AvcodecProfile::OnevplHostFallback => VideoBackendPolicy::onevpl_host(true),
        AvcodecProfile::AmfHost => VideoBackendPolicy::amf_host(false),
        AvcodecProfile::AmfHostFallback => VideoBackendPolicy::amf_host(true),
    }
}

fn io_memory_plan(profile: AvcodecProfile) -> VideoIoMemoryPlan {
    match profile {
        AvcodecProfile::NativeFree
        | AvcodecProfile::Software
        | AvcodecProfile::RkmppHost
        | AvcodecProfile::RkmppHostFallback
        | AvcodecProfile::NvcodecHost
        | AvcodecProfile::NvcodecHostFallback
        | AvcodecProfile::OnevplHost
        | AvcodecProfile::OnevplHostFallback
        | AvcodecProfile::AmfHost
        | AvcodecProfile::AmfHostFallback => VideoIoMemoryPlan::new()
            .with_decoder_packet_input(MemoryDomain::Host)
            .with_decoder_image_output(MemoryDomain::Host)
            .with_encoder_image_input(MemoryDomain::Host)
            .with_encoder_packet_output(MemoryDomain::Host),
        AvcodecProfile::RkmppZeroCopy => VideoIoMemoryPlan::new()
            .with_decoder_packet_input(MemoryDomain::Host)
            .with_decoder_image_output(MemoryDomain::DrmPrime)
            .with_processor(MemoryDomain::DrmPrime, MemoryDomain::DmaBuf)
            .with_encoder_image_input(MemoryDomain::DmaBuf)
            .with_encoder_packet_output(MemoryDomain::Host)
            .with_allow_staging(false),
        AvcodecProfile::NvcodecDeviceFrame => VideoIoMemoryPlan::new()
            .with_decoder_packet_input(MemoryDomain::Host)
            .with_decoder_image_output(MemoryDomain::CudaDevice)
            .with_encoder_image_input(MemoryDomain::CudaDevice)
            .with_encoder_packet_output(MemoryDomain::Host)
            .with_allow_staging(false),
    }
}

/// Converts a dyun profile to the upstream SDK [`VideoProfileDescriptor`].
pub fn profile_to_sdk_descriptor(profile: AvcodecProfile) -> Result<VideoProfileDescriptor> {
    profile.ensure_compiled()?;
    let policy = backend_policy(profile);
    let descriptor = VideoProfileDescriptor::new(
        ProfileName::Borrowed(profile.name()),
        io_memory_plan(profile),
    )
    .with_decoder_policy(policy.decoder)
    .with_processor_policy(policy.image_processor)
    .with_encoder_policy(policy.encoder);
    descriptor.validate().map_err(|error| {
        Error::Config(format!(
            "profile `{}` validation failed: {error:?}",
            profile.name()
        ))
    })?;
    Ok(descriptor)
}

#[cfg(test)]
mod tests {
    use super::{
        compiled_profiles, profile_to_sdk_descriptor, reject_profile_hw_conflict, resolve_profile,
        AvcodecProfile,
    };
    use dg_media_avcodec::MemoryDomain;

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
    fn resolve_profile_legacy_avcodec_maps_native_free() {
        let result = resolve_profile(None, true);
        #[cfg(feature = "avcodec-profile-native-free")]
        assert_eq!(result, Ok(AvcodecProfile::NativeFree));
        #[cfg(not(feature = "avcodec-profile-native-free"))]
        assert!(result.is_err());
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
    #[cfg(feature = "avcodec-profile-rkmpp-zero-copy")]
    fn zero_copy_profile_topology_is_frozen() {
        let descriptor = profile_to_sdk_descriptor(AvcodecProfile::RkmppZeroCopy)
            .expect("rkmpp-zero-copy must compile when feature enabled");
        let io = descriptor.io;
        assert_eq!(io.decoder_packet_input, MemoryDomain::Host);
        assert_eq!(io.decoder_image_output, MemoryDomain::DrmPrime);
        assert_eq!(io.processor_image_input, Some(MemoryDomain::DrmPrime));
        assert_eq!(io.processor_image_output, Some(MemoryDomain::DmaBuf));
        assert_eq!(io.encoder_image_input, MemoryDomain::DmaBuf);
        assert!(!io.allow_staging);
    }

    #[test]
    #[cfg(feature = "avcodec-profile-nvcodec-device-frame")]
    fn nv_device_frame_profile_has_no_processor() {
        let descriptor = profile_to_sdk_descriptor(AvcodecProfile::NvcodecDeviceFrame)
            .expect("nvcodec-device-frame must compile when feature enabled");
        assert_eq!(descriptor.io.decoder_image_output, MemoryDomain::CudaDevice);
        assert!(!descriptor.io.processor_enabled());
        assert!(!descriptor.io.allow_staging);
    }

    #[test]
    fn host_profiles_use_host_topology() {
        let descriptor =
            profile_to_sdk_descriptor(AvcodecProfile::NativeFree).expect("native-free profile");
        let io = descriptor.io;
        assert_eq!(io.decoder_packet_input, MemoryDomain::Host);
        assert_eq!(io.decoder_image_output, MemoryDomain::Host);
        assert_eq!(io.encoder_image_input, MemoryDomain::Host);
        assert!(!io.processor_enabled());
    }

    #[test]
    fn all_compiled_profiles_map_to_valid_descriptors() {
        for profile in compiled_profiles() {
            profile_to_sdk_descriptor(profile).expect("compiled profile must map");
        }
    }
}
