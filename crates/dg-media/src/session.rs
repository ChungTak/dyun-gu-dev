//! Avcodec session assembly via upstream `VideoSessionFactoryV2`.

use dg_core::{Error, Result};

use dg_media_avcodec::{
    Decoder, DecoderConfig, Encoder, EncoderConfig, ImageOpKind, ImageProcessor,
    ImageProcessorConfig, MemoryDomain, Registry, VideoProfileDescriptor, VideoSessionBuildError,
    VideoSessionBuildReport, VideoSessionBundle, VideoSessionFactoryV2, VideoSessionRequest,
    VideoSessionRole,
};

use crate::avcodec::map_av_error;
use crate::profile::{
    profile_to_sdk_descriptor, profile_to_sdk_descriptor_with_processor, AvcodecProfile,
};

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
        let config = align_decoder_config(config, &sdk_profile)?;
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
        let config = align_encoder_config(config, &sdk_profile)?;
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
        // Processor requests need a descriptor whose IO plan enables the processor stage;
        // host profiles leave processor domains unset on the base descriptor.
        let sdk_profile = profile_to_sdk_descriptor_with_processor(profile)?;
        let config = align_processor_config(config.with_target_op(target_op), &sdk_profile)?;
        let bundle = self.build(&VideoSessionRequest {
            profile: sdk_profile,
            decoder: None,
            processor: Some(config),
            encoder: None,
        })?;
        let processor = bundle.processor.ok_or_else(|| {
            Error::Media("image processor session missing from factory bundle".into())
        })?;
        Ok((processor, bundle.report))
    }
}

/// Stamps decoder config domains/staging from the profile IO plan (Factory V2 contract).
pub(crate) fn align_decoder_config(
    config: DecoderConfig,
    profile: &VideoProfileDescriptor,
) -> Result<DecoderConfig> {
    reject_explicit_domain_conflict(
        "decoder image",
        config.memory_domain,
        profile.io.decoder_image_output,
        /*default_is_host*/ true,
    )?;
    if let Some(packet_domain) = config.packet_input_domain {
        if packet_domain != profile.io.decoder_packet_input {
            return Err(domain_conflict(
                "decoder packet",
                packet_domain,
                profile.io.decoder_packet_input,
            ));
        }
    }
    if config.allow_staging != profile.io.allow_staging {
        // Default DecoderConfig uses allow_staging=false; only reject non-default mismatches
        // after stamping is applied — Factory rejects any residual mismatch.
        if config.allow_staging && !profile.io.allow_staging {
            return Err(Error::Config(format!(
                "profile `{}` forbids staging (allow_staging=false); cannot enable staging on decoder",
                profile.name
            )));
        }
    }
    Ok(config
        .with_memory_domain(profile.io.decoder_image_output)
        .with_packet_input_domain(profile.io.decoder_packet_input)
        .with_allow_staging(profile.io.allow_staging))
}

/// Stamps encoder config domains/staging from the profile IO plan.
pub(crate) fn align_encoder_config(
    config: EncoderConfig,
    profile: &VideoProfileDescriptor,
) -> Result<EncoderConfig> {
    reject_explicit_domain_conflict(
        "encoder image",
        config.memory_domain,
        profile.io.encoder_image_input,
        true,
    )?;
    if config.allow_staging && !profile.io.allow_staging {
        return Err(Error::Config(format!(
            "profile `{}` forbids staging (allow_staging=false); cannot enable staging on encoder",
            profile.name
        )));
    }
    Ok(config
        .with_memory_domain(profile.io.encoder_image_input)
        .with_packet_output_domain(profile.io.encoder_packet_output)
        .with_allow_staging(profile.io.allow_staging))
}

/// Stamps processor config domains/staging from a processor-enabled profile IO plan.
pub(crate) fn align_processor_config(
    config: ImageProcessorConfig,
    profile: &VideoProfileDescriptor,
) -> Result<ImageProcessorConfig> {
    let (input, output) = match (
        profile.io.processor_image_input,
        profile.io.processor_image_output,
    ) {
        (Some(input), Some(output)) => (input, output),
        _ => {
            return Err(Error::Config(format!(
                "profile `{}` does not enable an image processor stage",
                profile.name
            )));
        }
    };
    // Defaults are Host; only treat non-Host as explicit overrides when topology is non-Host.
    if config.memory_domain != MemoryDomain::Host && config.memory_domain != input {
        return Err(domain_conflict("processor", config.memory_domain, input));
    }
    if config.allow_staging && !profile.io.allow_staging {
        return Err(Error::Config(format!(
            "profile `{}` forbids staging (allow_staging=false); cannot enable staging on processor",
            profile.name
        )));
    }
    let target_op = config.target_op.unwrap_or(ImageOpKind::Resize);
    Ok(config
        .with_memory_domain(input)
        .with_input_memory_domain(input)
        .with_output_memory_domain(output)
        .with_allow_staging(profile.io.allow_staging)
        .with_target_op(target_op))
}

fn reject_explicit_domain_conflict(
    role: &str,
    configured: MemoryDomain,
    expected: MemoryDomain,
    treat_host_as_default: bool,
) -> Result<()> {
    if configured == expected {
        return Ok(());
    }
    // DecoderConfig/EncoderConfig default to Host. When the profile expects a non-Host domain
    // and the caller left the default Host, we overwrite — that is not a conflict.
    if treat_host_as_default && configured == MemoryDomain::Host && expected != MemoryDomain::Host {
        return Ok(());
    }
    // Explicit non-default that disagrees with the profile is a topology violation.
    if configured != MemoryDomain::Host || expected == MemoryDomain::Host {
        return Err(domain_conflict(role, configured, expected));
    }
    Ok(())
}

fn domain_conflict(role: &str, configured: MemoryDomain, expected: MemoryDomain) -> Error {
    Error::Config(format!(
        "{role} memory domain {configured:?} conflicts with profile topology {expected:?}"
    ))
}

fn map_session_build_error(error: VideoSessionBuildError) -> Error {
    use crate::{media_error_with_context, MediaErrorContext, MediaOperation};
    use dg_media_avcodec::AvError;

    let role = match error.role {
        VideoSessionRole::Decoder => "decoder",
        VideoSessionRole::Encoder => "encoder",
        VideoSessionRole::ImageProcessor => "image_processor",
    };
    let operation = match error.role {
        VideoSessionRole::Decoder => MediaOperation::CreateDecoder,
        VideoSessionRole::Encoder => MediaOperation::CreateEncoder,
        VideoSessionRole::ImageProcessor => MediaOperation::CreateProcessor,
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
    let diagnosis = error
        .failure
        .as_ref()
        .map(|failure| format!("{:?}", failure.diagnosis))
        .unwrap_or_else(|| "none".to_string());

    // Factory V2 attaches profile/domain on AvError::WithContext for topology failures.
    let (profile, codec, source_domain, target_domain, allow_staging) = match &error.error {
        AvError::WithContext { context, .. } => (
            context.profile_name.clone(),
            context.codec,
            context.source_domain,
            context.target_domain,
            context.allow_staging,
        ),
        _ => (None, None, None, None, None),
    };
    let detail = format!(
        "session build failed for role {role} (diagnosis={diagnosis}): {}",
        map_av_error(error.error)
    );
    let mut ctx = MediaErrorContext::new("Unsupported", operation, detail)
        .with_backend(backend)
        .with_role(role);
    if let Some(profile) = profile {
        ctx = ctx.with_profile(profile);
    }
    if let Some(codec) = codec {
        ctx = ctx.with_codec(format!("{codec:?}"));
    }
    ctx = ctx.with_domains(
        source_domain.and_then(av_domain_to_core),
        target_domain.and_then(av_domain_to_core),
        allow_staging,
    );
    media_error_with_context(ctx)
}

fn av_domain_to_core(domain: MemoryDomain) -> Option<dg_core::MemoryDomain> {
    match domain {
        MemoryDomain::Host => Some(dg_core::MemoryDomain::Host),
        MemoryDomain::DmaBuf => Some(dg_core::MemoryDomain::DmaBuf),
        MemoryDomain::DrmPrime => Some(dg_core::MemoryDomain::DrmPrime),
        MemoryDomain::CudaDevice => Some(dg_core::MemoryDomain::CudaDevice),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use dg_media_avcodec::{
        CodecId, DecoderConfig, ImageOpKind, ImageProcessorConfig, MemoryDomain, TimeBase,
    };
    #[cfg(feature = "avcodec-profile-nvcodec-device-frame")]
    use dg_media_avcodec::{EncoderConfig, ImageInfo};

    #[cfg(feature = "avcodec-profile-nvcodec-device-frame")]
    use super::align_encoder_config;
    #[cfg(feature = "avcodec-profile-rkmpp-zero-copy")]
    use super::align_processor_config;
    use super::{align_decoder_config, AvcodecSdkService};
    use crate::profile::{
        profile_to_sdk_descriptor, profile_to_sdk_descriptor_with_processor, AvcodecProfile,
    };

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

    #[test]
    fn native_free_resize_processor_builds_with_factory_v2() {
        let service = AvcodecSdkService::from_default_registry();
        let (_processor, report) = service
            .build_image_processor(
                AvcodecProfile::NativeFree,
                ImageProcessorConfig::new(),
                ImageOpKind::Resize,
            )
            .expect("host profiles must enable processor topology for resize");
        assert_eq!(report.policy_name, "native-free");
        assert!(report.image_processor_backend.is_some());
    }

    #[test]
    fn align_decoder_stamps_host_topology() {
        let profile = profile_to_sdk_descriptor(AvcodecProfile::NativeFree).expect("profile");
        let config = align_decoder_config(
            DecoderConfig::new(CodecId::H264, TimeBase::new(1, 25)),
            &profile,
        )
        .expect("align");
        assert_eq!(config.memory_domain, MemoryDomain::Host);
        assert_eq!(config.packet_input_domain(), MemoryDomain::Host);
        assert!(!config.allow_staging);
    }

    #[test]
    #[cfg(feature = "avcodec-profile-rkmpp-zero-copy")]
    fn align_decoder_stamps_zero_copy_image_domain() {
        let profile = profile_to_sdk_descriptor(AvcodecProfile::RkmppZeroCopy).expect("profile");
        let config = align_decoder_config(
            DecoderConfig::new(CodecId::H264, TimeBase::new(1, 25)),
            &profile,
        )
        .expect("align");
        assert_eq!(config.memory_domain, MemoryDomain::DrmPrime);
        assert_eq!(config.packet_input_domain(), MemoryDomain::Host);
        assert!(!config.allow_staging);
    }

    #[test]
    #[cfg(feature = "avcodec-profile-nvcodec-device-frame")]
    fn align_encoder_stamps_cuda_device_domain() {
        let profile =
            profile_to_sdk_descriptor(AvcodecProfile::NvcodecDeviceFrame).expect("profile");
        let config = align_encoder_config(
            EncoderConfig::new(
                CodecId::H264,
                64,
                64,
                ImageInfo::Nv12,
                TimeBase::new(1, 25),
                100_000,
            ),
            &profile,
        )
        .expect("align");
        assert_eq!(config.memory_domain, MemoryDomain::CudaDevice);
        assert!(!config.allow_staging);
    }

    #[test]
    #[cfg(feature = "avcodec-profile-rkmpp-zero-copy")]
    fn align_processor_stamps_drm_to_dmabuf() {
        let profile =
            profile_to_sdk_descriptor_with_processor(AvcodecProfile::RkmppZeroCopy).expect("p");
        let config = align_processor_config(ImageProcessorConfig::new(), &profile).expect("align");
        assert_eq!(config.input_memory_domain(), MemoryDomain::DrmPrime);
        assert_eq!(config.output_memory_domain(), MemoryDomain::DmaBuf);
        assert!(!config.allow_staging);
    }

    #[test]
    fn device_frame_rejects_processor_descriptor() {
        let result = profile_to_sdk_descriptor_with_processor(AvcodecProfile::NvcodecDeviceFrame);
        #[cfg(feature = "avcodec-profile-nvcodec-device-frame")]
        {
            assert!(result.is_err());
        }
        #[cfg(not(feature = "avcodec-profile-nvcodec-device-frame"))]
        {
            assert!(result.is_err());
        }
    }
}
