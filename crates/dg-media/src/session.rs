//! Avcodec session assembly via upstream `VideoSessionFactoryV2`.

use dg_core::{Error, Result};

use dg_media_avcodec::{
    Decoder, DecoderConfig, Encoder, EncoderConfig, ImageOpKind, ImageProcessor,
    ImageProcessorConfig, Registry, VideoSessionBuildError, VideoSessionBuildReport,
    VideoSessionBundle, VideoSessionFactoryV2, VideoSessionRequest, VideoSessionRole,
};

use crate::avcodec::map_av_error;
use crate::profile::{profile_to_sdk_descriptor, AvcodecProfile};

/// Shared SDK service holding a registry and building sessions through Factory V2.
pub struct AvcodecSdkService {
    registry: Registry,
}

impl AvcodecSdkService {
    #[must_use]
    pub fn new(registry: Registry) -> Self {
        Self { registry }
    }

    #[must_use]
    pub fn from_default_registry() -> Self {
        Self::new(dg_media_avcodec::default_registry_builder().build())
    }

    pub fn build(&self, request: &VideoSessionRequest) -> Result<VideoSessionBundle> {
        VideoSessionFactoryV2::new(&self.registry)
            .build(request)
            .map_err(map_session_build_error)
    }

    pub fn build_decoder(
        &self,
        profile: AvcodecProfile,
        config: DecoderConfig,
    ) -> Result<(Box<dyn Decoder>, VideoSessionBuildReport)> {
        let sdk_profile = profile_to_sdk_descriptor(profile)?;
        let bundle = self.build(&VideoSessionRequest {
            profile: sdk_profile,
            decoder: Some(config),
            processor: None,
            encoder: None,
        })?;
        let decoder = bundle
            .decoder
            .ok_or_else(|| Error::Media("decoder session missing from factory bundle".into()))?;
        Ok((decoder, bundle.report))
    }

    pub fn build_encoder(
        &self,
        profile: AvcodecProfile,
        config: EncoderConfig,
    ) -> Result<(Box<dyn Encoder>, VideoSessionBuildReport)> {
        let sdk_profile = profile_to_sdk_descriptor(profile)?;
        let bundle = self.build(&VideoSessionRequest {
            profile: sdk_profile,
            decoder: None,
            processor: None,
            encoder: Some(config),
        })?;
        let encoder = bundle
            .encoder
            .ok_or_else(|| Error::Media("encoder session missing from factory bundle".into()))?;
        Ok((encoder, bundle.report))
    }

    pub fn build_image_processor(
        &self,
        profile: AvcodecProfile,
        config: ImageProcessorConfig,
        target_op: ImageOpKind,
    ) -> Result<(Box<dyn ImageProcessor>, VideoSessionBuildReport)> {
        let sdk_profile = profile_to_sdk_descriptor(profile)?;
        let processor_config = config.with_target_op(target_op);
        let bundle = self.build(&VideoSessionRequest {
            profile: sdk_profile,
            decoder: None,
            processor: Some(processor_config),
            encoder: None,
        })?;
        let processor = bundle.processor.ok_or_else(|| {
            Error::Media("image processor session missing from factory bundle".into())
        })?;
        Ok((processor, bundle.report))
    }
}

fn map_session_build_error(error: VideoSessionBuildError) -> Error {
    use crate::{media_error_with_context, MediaErrorContext, MediaOperation};

    let role = match error.role {
        VideoSessionRole::Decoder => "decoder",
        VideoSessionRole::Encoder => "encoder",
        VideoSessionRole::ImageProcessor => "image_processor",
    };
    let backend = error
        .failure
        .as_ref()
        .and_then(|failure| {
            failure
                .trace
                .selected_backend
                .or(failure.trace.backend_hint)
        })
        .unwrap_or("none");
    let detail = format!(
        "session build failed for role {role}: {}",
        map_av_error(error.error)
    );
    media_error_with_context(
        MediaErrorContext::new("Unsupported", MediaOperation::Select, detail)
            .with_backend(backend)
            .with_role(role),
    )
}

#[cfg(test)]
mod tests {
    use dg_media_avcodec::{CodecId, DecoderConfig, TimeBase};

    use super::AvcodecSdkService;
    use crate::profile::AvcodecProfile;

    #[test]
    fn native_free_decoder_builds_with_factory_v2() {
        let service = AvcodecSdkService::from_default_registry();
        let config = DecoderConfig::new(CodecId::Jpeg, TimeBase::new(1, 25));
        let (_decoder, report) = service
            .build_decoder(AvcodecProfile::NativeFree, config)
            .expect("jpeg decoder must be available in native-free profile");
        assert_eq!(report.policy_name, "native-free");
        assert!(report.decoder_backend.is_some());
    }
}
