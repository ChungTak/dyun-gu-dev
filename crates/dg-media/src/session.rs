//! V3 avcodec [`VideoSdk`] service wrapper.
//!
//! This module exposes a thin, owned-session service that forwards dyun business configuration
//! to the upstream high-level `VideoSdk` facade. It does not expose the upstream `Registry`, does
//! not assemble low-level SDK requests, and does not perform backend policy or memory-domain
//! stamping.

use std::sync::Arc;

use dg_core::{Error, Result};

use crate::profile::AvcodecProfile;
use crate::{media_error_with_context, MediaErrorContext, MediaOperation};

#[cfg(feature = "avcodec-sdk")]
use dg_media_avcodec::{
    ImageProcessorRequest, OwnedVideoBuildReport, VideoBuildError, VideoDecoderRequest,
    VideoDecoderSession, VideoEncoderRequest, VideoEncoderSession, VideoImageProcessorSession,
    VideoRole, VideoSdk, VideoTranscodeRequest, VideoTranscoderSession,
};

/// Owned avcodec video SDK service.
///
/// Internally holds a [`VideoSdk`] facade. Construction uses the default built-in backend
/// registry; tests may inject a pre-built SDK via [`Self::with_sdk`].
pub struct AvcodecSdkService {
    #[cfg(feature = "avcodec-sdk")]
    sdk: VideoSdk,
    #[cfg(not(feature = "avcodec-sdk"))]
    _marker: (),
}

impl AvcodecSdkService {
    /// Creates a new service backed by the default upstream `VideoSdk`.
    pub fn new() -> Result<Self> {
        Self::try_new()
    }

    #[cfg(feature = "avcodec-sdk")]
    fn try_new() -> Result<Self> {
        let sdk = VideoSdk::new().map_err(map_video_build_error)?;
        Ok(Self { sdk })
    }

    #[cfg(not(feature = "avcodec-sdk"))]
    fn try_new() -> Result<Self> {
        Err(Error::Config(
            "avcodec SDK is not enabled; enable an `avcodec-profile-*` feature".to_string(),
        ))
    }

    #[cfg(feature = "avcodec-sdk")]
    #[allow(dead_code)] // used by integration tests
    /// Creates a service wrapping a pre-constructed `VideoSdk` (used by tests).
    pub fn with_sdk(sdk: VideoSdk) -> Self {
        Self { sdk }
    }

    #[cfg(feature = "avcodec-sdk")]
    #[allow(dead_code)] // used by integration tests
    /// Creates a service wrapping a pre-constructed `VideoSdk` built from a shared `Registry`.
    pub fn with_registry(registry: Arc<dg_media_avcodec::Registry>) -> Self {
        Self {
            sdk: VideoSdk::with_registry(registry),
        }
    }

    /// Creates a decoder session from the selected profile and upstream request.
    #[cfg(feature = "avcodec-sdk")]
    pub fn create_decoder(
        &self,
        profile: AvcodecProfile,
        request: VideoDecoderRequest,
    ) -> Result<(VideoDecoderSession, OwnedVideoBuildReport)> {
        let profile = profile.to_sdk()?;
        let created = self
            .sdk
            .create_decoder(profile, request)
            .map_err(map_video_build_error)?;
        Ok(created.into_parts())
    }

    /// Creates an encoder session from the selected profile and upstream request.
    #[cfg(feature = "avcodec-sdk")]
    pub fn create_encoder(
        &self,
        profile: AvcodecProfile,
        request: VideoEncoderRequest,
    ) -> Result<(VideoEncoderSession, OwnedVideoBuildReport)> {
        let profile = profile.to_sdk()?;
        let created = self
            .sdk
            .create_encoder(profile, request)
            .map_err(map_video_build_error)?;
        Ok(created.into_parts())
    }

    /// Creates an image-processor session from the selected profile and upstream request.
    #[cfg(feature = "avcodec-sdk")]
    pub fn create_image_processor(
        &self,
        profile: AvcodecProfile,
        request: ImageProcessorRequest,
    ) -> Result<(VideoImageProcessorSession, OwnedVideoBuildReport)> {
        let profile = profile.to_sdk()?;
        let created = self
            .sdk
            .create_image_processor(profile, request)
            .map_err(map_video_build_error)?;
        Ok(created.into_parts())
    }

    /// Creates a transcoder session from the selected profile and upstream request.
    #[cfg(feature = "avcodec-sdk")]
    pub fn create_transcoder(
        &self,
        profile: AvcodecProfile,
        request: VideoTranscodeRequest,
    ) -> Result<(VideoTranscoderSession, OwnedVideoBuildReport)> {
        let profile = profile.to_sdk()?;
        let created = self
            .sdk
            .create_transcoder(profile, request)
            .map_err(map_video_build_error)?;
        Ok(created.into_parts())
    }
}

#[cfg(feature = "avcodec-sdk")]
pub(crate) fn map_video_build_error(error: VideoBuildError) -> Error {
    use dg_media_avcodec::VideoRole;

    let operation = match error.role {
        Some(VideoRole::Decoder) => MediaOperation::CreateDecoder,
        Some(VideoRole::Encoder) => MediaOperation::CreateEncoder,
        Some(VideoRole::ImageProcessor) => MediaOperation::CreateProcessor,
        _ => MediaOperation::Select,
    };

    let detail = if let Some(source_err) = error.source.as_ref() {
        format!("{} (source: {source_err:?})", error.message)
    } else {
        error.message.clone()
    };

    let mut context = MediaErrorContext::new("video build failed", operation, detail)
        .with_profile(error.profile.unwrap_or("unknown"));

    if let Some(role) = error.role {
        context = context.with_role(map_video_role(role));
    }

    if let Some(selection) = error.selection_failure.as_ref() {
        let backend = selection
            .trace
            .selected_backend
            .or(selection.trace.backend_hint)
            .unwrap_or("none");
        context = context.with_backend(backend);
    }

    if let Some(codec) = error.codec {
        context = context.with_codec(format!("{codec:?}"));
    }

    let source = error
        .source_domain
        .map(crate::bridge::avcodec_memory_domain_to_core);
    let target = error
        .target_domain
        .map(crate::bridge::avcodec_memory_domain_to_core);
    context = context.with_domains(source, target, error.allow_staging);

    media_error_with_context(context)
}

#[cfg(feature = "avcodec-sdk")]
fn map_video_role(role: VideoRole) -> &'static str {
    match role {
        VideoRole::Decoder => "decoder",
        VideoRole::Encoder => "encoder",
        VideoRole::ImageProcessor => "image-processor",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_build_error_contains_profile_context() {
        // `VideoBuildError` is only available inside the SDK; exercise `map_video_build_error`
        // through a real build path that fails because the `software` profile is not compiled.
        #[cfg(all(feature = "avcodec-sdk", feature = "avcodec-profile-native-free"))]
        {
            use dg_media_avcodec::{CodecId, ImageInfo, TimeBase, VideoEncoderRequest};
            let service = AvcodecSdkService::new().expect("sdk");
            let request = VideoEncoderRequest::new(
                CodecId::H264,
                32,
                32,
                ImageInfo::Yuv420p,
                TimeBase::new(1, 30),
                1_000_000,
            )
            .expect("valid request");
            let err = match service.create_encoder(AvcodecProfile::Software, request) {
                Err(err) => err,
                Ok(_) => panic!("software not compiled"),
            };
            let text = err.to_string();
            assert!(text.contains("software"), "{text}");
        }

        // Keep the test body non-empty when the SDK is disabled.
        #[cfg(not(all(feature = "avcodec-sdk", feature = "avcodec-profile-native-free")))]
        {
            let _ = AvcodecProfile::NativeFree;
        }
    }
}
