//! Runtime avcodec profile selection and descriptor mapping.

use dg_core::{Error, MemoryDomain, Result};

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

/// Profile-level backend and memory policy consumed by session builders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileDescriptor {
    pub decode_backend_hints: &'static [&'static str],
    pub encode_backend_hints: &'static [&'static str],
    pub processor_backend_hints: &'static [&'static str],
    pub allow_fallback: bool,
    pub memory_domain: MemoryDomain,
    pub allow_staging: bool,
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

    /// Returns an error for profiles blocked on upstream UP-* deliverables.
    pub fn ensure_session_create_supported(self) -> Result<()> {
        match self {
            Self::RkmppZeroCopy => Err(Error::Media(
                "profile `rkmpp-zero-copy` requires upstream UP-03 Profile V2 Session Factory; \
                 session creation is not available yet"
                    .to_string(),
            )),
            Self::NvcodecDeviceFrame => Err(Error::Media(
                "profile `nvcodec-device-frame` requires upstream UP-06 nvcodec-device-frame \
                 contract; session creation is not available yet"
                    .to_string(),
            )),
            _ => Ok(()),
        }
    }
}

/// Returns every profile compiled into the current binary.
#[must_use]
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
///
/// When `legacy_avcodec_only` is true the legacy `avcodec` feature path maps to
/// [`AvcodecProfile::NativeFree`] for compatibility.
pub fn resolve_profile(name: Option<&str>, legacy_avcodec_only: bool) -> Result<AvcodecProfile> {
    if let Some(name) = name {
        let profile = AvcodecProfile::parse(name)?;
        if !profile.is_compiled() {
            return Err(Error::Config(format!(
                "profile `{name}` is not compiled; enable cargo feature `{}`",
                profile.cargo_feature()
            )));
        }
        return Ok(profile);
    }

    if legacy_avcodec_only {
        let profile = AvcodecProfile::NativeFree;
        if !profile.is_compiled() {
            return Err(Error::Config(
                "legacy `avcodec` feature requires the native-free profile to be compiled"
                    .to_string(),
            ));
        }
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

/// Maps a profile to backend hints and memory policy without duplicating upstream id lists.
#[must_use]
pub fn profile_descriptor(profile: AvcodecProfile) -> ProfileDescriptor {
    match profile {
        AvcodecProfile::NativeFree => ProfileDescriptor {
            decode_backend_hints: &["jpeg", "zune", "rust-h264"],
            encode_backend_hints: &["jpeg", "rust-h264"],
            processor_backend_hints: &["libyuv"],
            allow_fallback: false,
            memory_domain: MemoryDomain::Host,
            allow_staging: true,
        },
        AvcodecProfile::Software => ProfileDescriptor {
            decode_backend_hints: &["jpeg", "zune", "ffmpeg", "openh264"],
            encode_backend_hints: &["jpeg", "ffmpeg", "x264", "x265", "openh264"],
            processor_backend_hints: &["libyuv"],
            allow_fallback: false,
            memory_domain: MemoryDomain::Host,
            allow_staging: true,
        },
        AvcodecProfile::RkmppHost => ProfileDescriptor {
            decode_backend_hints: &["rkmpp"],
            encode_backend_hints: &["rkmpp"],
            processor_backend_hints: &["librga"],
            allow_fallback: false,
            memory_domain: MemoryDomain::Host,
            allow_staging: true,
        },
        AvcodecProfile::RkmppHostFallback => ProfileDescriptor {
            decode_backend_hints: &["rkmpp", "ffmpeg", "openh264"],
            encode_backend_hints: &["rkmpp", "ffmpeg", "x264", "openh264"],
            processor_backend_hints: &["librga", "libyuv"],
            allow_fallback: true,
            memory_domain: MemoryDomain::Host,
            allow_staging: true,
        },
        AvcodecProfile::RkmppZeroCopy => ProfileDescriptor {
            decode_backend_hints: &["rkmpp"],
            encode_backend_hints: &["rkmpp"],
            processor_backend_hints: &["librga"],
            allow_fallback: false,
            memory_domain: MemoryDomain::DrmPrime,
            allow_staging: false,
        },
        AvcodecProfile::NvcodecHost => ProfileDescriptor {
            decode_backend_hints: &["nvcodec"],
            encode_backend_hints: &["nvcodec"],
            processor_backend_hints: &["libyuv"],
            allow_fallback: false,
            memory_domain: MemoryDomain::Host,
            allow_staging: true,
        },
        AvcodecProfile::NvcodecHostFallback => ProfileDescriptor {
            decode_backend_hints: &["nvcodec", "ffmpeg", "openh264"],
            encode_backend_hints: &["nvcodec", "ffmpeg", "x264", "openh264"],
            processor_backend_hints: &["libyuv"],
            allow_fallback: true,
            memory_domain: MemoryDomain::Host,
            allow_staging: true,
        },
        AvcodecProfile::NvcodecDeviceFrame => ProfileDescriptor {
            decode_backend_hints: &["nvcodec"],
            encode_backend_hints: &["nvcodec"],
            processor_backend_hints: &[],
            allow_fallback: false,
            memory_domain: MemoryDomain::CudaDevice,
            allow_staging: false,
        },
        AvcodecProfile::OnevplHost => ProfileDescriptor {
            decode_backend_hints: &["onevpl"],
            encode_backend_hints: &["onevpl"],
            processor_backend_hints: &["libyuv"],
            allow_fallback: false,
            memory_domain: MemoryDomain::Host,
            allow_staging: true,
        },
        AvcodecProfile::OnevplHostFallback => ProfileDescriptor {
            decode_backend_hints: &["onevpl", "ffmpeg", "openh264"],
            encode_backend_hints: &["onevpl", "ffmpeg", "x264", "openh264"],
            processor_backend_hints: &["libyuv"],
            allow_fallback: true,
            memory_domain: MemoryDomain::Host,
            allow_staging: true,
        },
        AvcodecProfile::AmfHost => ProfileDescriptor {
            decode_backend_hints: &["amf"],
            encode_backend_hints: &["amf"],
            processor_backend_hints: &["libyuv"],
            allow_fallback: false,
            memory_domain: MemoryDomain::Host,
            allow_staging: true,
        },
        AvcodecProfile::AmfHostFallback => ProfileDescriptor {
            decode_backend_hints: &["amf", "ffmpeg", "openh264"],
            encode_backend_hints: &["amf", "ffmpeg", "x264", "openh264"],
            processor_backend_hints: &["libyuv"],
            allow_fallback: true,
            memory_domain: MemoryDomain::Host,
            allow_staging: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        compiled_profiles, profile_descriptor, reject_profile_hw_conflict, resolve_profile,
        AvcodecProfile,
    };

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
    fn profile_descriptor_marks_fallback_profiles() {
        let host = profile_descriptor(AvcodecProfile::RkmppHost);
        let fallback = profile_descriptor(AvcodecProfile::RkmppHostFallback);
        assert!(!host.allow_fallback);
        assert!(fallback.allow_fallback);
        assert!(host.allow_staging);
        assert!(!profile_descriptor(AvcodecProfile::RkmppZeroCopy).allow_staging);
    }

    #[test]
    fn gated_profiles_block_session_create() {
        assert!(AvcodecProfile::RkmppZeroCopy
            .ensure_session_create_supported()
            .is_err());
        assert!(AvcodecProfile::NvcodecDeviceFrame
            .ensure_session_create_supported()
            .is_err());
        assert!(AvcodecProfile::NativeFree
            .ensure_session_create_supported()
            .is_ok());
    }
}
