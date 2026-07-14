//! Deprecated hardware preference parsing and legacy backend candidate loops.
//!
//! These helpers remain for one release cycle of `hw` / direct-backend compatibility.
//! Production profile paths must not call the candidate-loop creators below.

#![allow(dead_code)]
#![allow(deprecated)]

use dg_core::{Error, Result};

use crate::avcodec::map_av_error;
use crate::profile::AvcodecProfile;

use dg_media_avcodec::{
    AvError, AvErrorKind, CodecId, Decoder, DecoderConfig, Encoder, EncoderConfig, Image,
    ImageOpKind, ImageProcessor, ImageProcessorConfig, MemoryDomain, Registry, TimeBase,
};

/// Deprecated hardware preference used by legacy `hw` element parameters.
#[deprecated(
    since = "0.1.0",
    note = "use `profile` with `avcodec-profile-*` features instead of `hw`"
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

pub(crate) fn registry() -> Registry {
    dg_media_avcodec::default_registry_builder().build()
}

fn backend_candidates(codec: CodecId, hw: HwPreference, encode: bool) -> Vec<&'static str> {
    if matches!(codec, CodecId::Jpeg | CodecId::Mjpeg) {
        return if encode {
            vec!["jpeg"]
        } else {
            vec!["jpeg", "zune"]
        };
    }

    let hardware = match hw {
        HwPreference::Auto => vec!["rkmpp", "nvcodec", "onevpl", "amf"],
        HwPreference::Rockchip => vec!["rkmpp"],
        HwPreference::Nvidia => vec!["nvcodec"],
        HwPreference::Intel => vec!["onevpl"],
        HwPreference::Amd => vec!["amf"],
        HwPreference::Software => Vec::new(),
    };
    let software = if encode {
        ["ffmpeg", "x264", "openh264"].as_slice()
    } else {
        ["ffmpeg", "openh264"].as_slice()
    };
    hardware
        .into_iter()
        .chain(software.iter().copied())
        .collect()
}

pub(crate) fn csc_candidates(hw: HwPreference) -> Vec<&'static str> {
    let hardware = match hw {
        HwPreference::Auto | HwPreference::Rockchip => vec!["librga"],
        HwPreference::Nvidia | HwPreference::Intel | HwPreference::Amd | HwPreference::Software => {
            Vec::new()
        }
    };
    hardware.into_iter().chain(["libyuv"]).collect()
}

fn no_backend_error(
    codec: CodecId,
    hw: HwPreference,
    candidates: &[&'static str],
    attempts: &[String],
) -> Error {
    Error::Media(format!(
        "no backend available for codec {codec:?} with hardware preference {hw:?}; attempted [{}]; enable one of cargo features: {}",
        attempts.join("; "),
        candidates
            .iter()
            .map(|candidate| match *candidate {
                "jpeg" | "zune" => "`avcodec` (jpeg/zune)".to_string(),
                other => format!("`codec-{other}`"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn no_csc_backend_error(
    codec: CodecId,
    hw: HwPreference,
    candidates: &[&'static str],
    attempts: &[String],
) -> Error {
    Error::Media(format!(
        "no CSC image processor available for codec {codec:?} with hardware preference {hw:?}; attempted [{}]; enable one of cargo features: {}",
        attempts.join("; "),
        candidates
            .iter()
            .map(|candidate| match *candidate {
                "librga" => "`codec-librga`".to_string(),
                "libyuv" => "`avcodec` (libyuv)".to_string(),
                other => format!("`codec-{other}`"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn is_skippable_selection_error(error: &AvError) -> bool {
    matches!(
        error.kind(),
        AvErrorKind::Unsupported | AvErrorKind::SelectionFailed
    )
}

/// Legacy decoder creation via manual backend candidate loops.
pub(crate) fn create_decoder(codec: CodecId, hw: HwPreference) -> Result<Box<dyn Decoder>> {
    let candidates = backend_candidates(codec, hw, false);
    let mut attempts = Vec::new();
    let registry = registry();
    let config = DecoderConfig::new(codec, TimeBase::new(1, 25))
        .with_memory_domain(MemoryDomain::Host)
        .with_allow_staging(true);
    for candidate in &candidates {
        let hinted = config.clone().with_backend_hint(Some(candidate));
        match registry.create_decoder(&hinted) {
            Ok(decoder) => return Ok(decoder),
            Err(error) if is_skippable_selection_error(&error) => {
                attempts.push(format!("{candidate}: {}", map_av_error(error)));
            }
            Err(error) => return Err(map_av_error(error)),
        }
    }
    Err(no_backend_error(codec, hw, &candidates, &attempts))
}

/// Legacy encoder creation via manual backend candidate loops.
pub(crate) fn create_encoder(
    codec: CodecId,
    hw: HwPreference,
    image: &Image,
) -> Result<Box<dyn Encoder>> {
    let candidates = backend_candidates(codec, hw, true);
    let mut attempts = Vec::new();
    let registry = registry();
    let config = EncoderConfig::new(
        codec,
        image.coded_width,
        image.coded_height,
        image.format,
        TimeBase::new(1, 25),
        1,
    )
    .with_memory_domain(MemoryDomain::Host)
    .with_allow_staging(true);
    for candidate in &candidates {
        let hinted = config.clone().with_backend_hint(Some(candidate));
        match registry.create_encoder(&hinted) {
            Ok(encoder) => return Ok(encoder),
            Err(error) if is_skippable_selection_error(&error) => {
                attempts.push(format!("{candidate}: {}", map_av_error(error)));
            }
            Err(error) => return Err(map_av_error(error)),
        }
    }
    Err(no_backend_error(codec, hw, &candidates, &attempts))
}

/// Legacy CSC processor creation via manual backend candidate loops.
pub(crate) fn create_csc_processor(
    codec: CodecId,
    hw: HwPreference,
) -> Result<Box<dyn ImageProcessor>> {
    let candidates = csc_candidates(hw);
    let config = ImageProcessorConfig::new()
        .with_memory_domain(MemoryDomain::Host)
        .with_allow_staging(true)
        .with_target_op(ImageOpKind::Csc);
    let registry = registry();
    let mut attempts = Vec::new();
    for candidate in &candidates {
        let hinted = config.with_backend_hint(Some(candidate));
        match registry.create_image_processor(&hinted) {
            Ok(processor) => return Ok(processor),
            Err(error) if is_skippable_selection_error(&error) => {
                attempts.push(format!("{candidate}: {}", map_av_error(error)));
            }
            Err(error) => return Err(map_av_error(error)),
        }
    }
    Err(no_csc_backend_error(codec, hw, &candidates, &attempts))
}

#[cfg(test)]
mod tests {
    use super::{
        create_csc_processor, create_decoder, create_encoder, csc_candidates, resolve_legacy_hw,
        HwPreference,
    };
    use crate::profile::AvcodecProfile;
    use dg_media_avcodec::{CodecId, Image, ImageInfo};

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

    #[test]
    fn jpeg_selection_uses_default_registry() {
        let image = Image::new_host_packed(ImageInfo::Rgb24, 2, 2, 0, 6, vec![0; 12], 1)
            .expect("valid JPEG image");
        assert!(create_encoder(CodecId::Jpeg, HwPreference::Auto, &image).is_ok());
    }

    #[test]
    fn csc_candidates_prefer_rockchip_hardware() {
        assert_eq!(
            csc_candidates(HwPreference::Rockchip),
            vec!["librga", "libyuv"]
        );
        assert_eq!(csc_candidates(HwPreference::Auto), vec!["librga", "libyuv"]);
    }

    #[test]
    fn software_csc_candidates_skip_hardware() {
        assert_eq!(csc_candidates(HwPreference::Software), vec!["libyuv"]);
    }

    #[test]
    fn default_registry_creates_software_csc_processor() {
        assert!(create_csc_processor(CodecId::H264, HwPreference::Software).is_ok());
    }

    #[test]
    fn h264_selection_reports_all_attempts_without_video_backends() {
        let error = match create_decoder(CodecId::H264, HwPreference::Auto) {
            Ok(_) => panic!("H264 must require an explicitly enabled backend"),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(message.contains("codec H264"), "{message}");
        assert!(message.contains("hardware preference Auto"), "{message}");
        assert!(message.contains("rkmpp"), "{message}");
        assert!(message.contains("ffmpeg"), "{message}");
        assert!(message.contains("codec-openh264"), "{message}");
    }

    #[test]
    fn h264_encoder_selection_reports_all_attempts_without_video_backends() {
        let image = Image::new_host_packed(ImageInfo::Rgb24, 2, 2, 0, 6, vec![0; 12], 1)
            .expect("valid H264 image");
        let error = match create_encoder(CodecId::H264, HwPreference::Auto, &image) {
            Ok(_) => panic!("H264 must require an explicitly enabled backend"),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(message.contains("codec H264"), "{message}");
        assert!(message.contains("hardware preference Auto"), "{message}");
        assert!(message.contains("x264"), "{message}");
        assert!(message.contains("codec-openh264"), "{message}");
    }

    #[test]
    fn hardware_preference_accepts_aliases() {
        assert_eq!(
            HwPreference::parse(Some("rknpu")),
            Ok(HwPreference::Rockchip)
        );
        assert_eq!(HwPreference::parse(Some("vaapi")), Ok(HwPreference::Intel));
        assert!(HwPreference::parse(Some("mystery")).is_err());
    }
}
