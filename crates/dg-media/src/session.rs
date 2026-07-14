//! Avcodec session assembly via registry selection with trace reporting.

use std::vec::Vec;

use dg_core::{Error, MemoryDomain, Result};

use dg_media_avcodec::{
    Decoder, DecoderConfig, Encoder, EncoderConfig, ImageOpKind, ImageProcessor,
    ImageProcessorConfig, Registry, SelectionFailureReport, SelectionTrace,
};

use crate::profile::{profile_descriptor, AvcodecProfile};

/// Element role used when building an avcodec session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionRole {
    Decoder,
    Encoder,
    ImageProcessor,
}

/// Diagnostics returned after a successful session build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionBuildReport {
    pub profile_name: &'static str,
    pub role: SessionRole,
    pub selected_backend: Option<&'static str>,
    pub memory_domain: MemoryDomain,
    pub allow_staging: bool,
    pub trace: SelectionTrace,
}

/// Builds avcodec decoder/encoder/processor sessions from a profile and registry.
pub struct AvcodecSessionBuilder<'a> {
    registry: &'a Registry,
    profile: AvcodecProfile,
}

impl<'a> AvcodecSessionBuilder<'a> {
    #[must_use]
    pub const fn new(registry: &'a Registry, profile: AvcodecProfile) -> Self {
        Self { registry, profile }
    }

    pub fn build_decoder(
        &self,
        mut config: DecoderConfig,
    ) -> Result<(Box<dyn Decoder>, SessionBuildReport)> {
        self.profile.ensure_session_create_supported()?;
        let descriptor = profile_descriptor(self.profile);
        config = apply_profile_decoder(config, &descriptor);
        let (decoder, trace) = self.select_with_hints(
            &descriptor.decode_backend_hints,
            descriptor.allow_fallback,
            |hint| {
                let hinted = config.clone().with_backend_hint(hint);
                self.registry.create_decoder_with_trace(&hinted)
            },
        )?;
        Ok((
            decoder,
            build_report(self.profile, SessionRole::Decoder, &descriptor, trace),
        ))
    }

    pub fn build_encoder(
        &self,
        mut config: EncoderConfig,
    ) -> Result<(Box<dyn Encoder>, SessionBuildReport)> {
        self.profile.ensure_session_create_supported()?;
        let descriptor = profile_descriptor(self.profile);
        config = apply_profile_encoder(config, &descriptor);
        let (encoder, trace) = self.select_with_hints(
            &descriptor.encode_backend_hints,
            descriptor.allow_fallback,
            |hint| {
                let hinted = config.clone().with_backend_hint(hint);
                self.registry.create_encoder_with_trace(&hinted)
            },
        )?;
        Ok((
            encoder,
            build_report(self.profile, SessionRole::Encoder, &descriptor, trace),
        ))
    }

    pub fn build_image_processor(
        &self,
        mut config: ImageProcessorConfig,
        target_op: ImageOpKind,
    ) -> Result<(Box<dyn ImageProcessor>, SessionBuildReport)> {
        self.profile.ensure_session_create_supported()?;
        let descriptor = profile_descriptor(self.profile);
        config = apply_profile_processor(config, &descriptor, target_op);
        let (processor, trace) = self.select_with_hints(
            &descriptor.processor_backend_hints,
            descriptor.allow_fallback,
            |hint| {
                let hinted = config.with_backend_hint(hint);
                self.registry.create_image_processor_with_trace(&hinted)
            },
        )?;
        Ok((
            processor,
            build_report(
                self.profile,
                SessionRole::ImageProcessor,
                &descriptor,
                trace,
            ),
        ))
    }

    fn select_with_hints<T>(
        &self,
        hints: &[&'static str],
        allow_fallback: bool,
        mut create: impl FnMut(
            Option<&'static str>,
        ) -> core::result::Result<(T, SelectionTrace), SelectionFailureReport>,
    ) -> Result<(T, SelectionTrace)> {
        if allow_fallback {
            let (value, trace) = create(None).map_err(map_selection_failure)?;
            return Ok((value, trace));
        }

        let mut last_report = None;
        for hint in hints {
            match create(Some(hint)) {
                Ok(result) => return Ok(result),
                Err(report) => last_report = Some(report),
            }
        }
        Err(map_selection_failure(last_report.unwrap_or_else(|| {
            SelectionFailureReport {
                error: dg_media_avcodec::AvError::selection_failed(
                    dg_media_avcodec::AvErrorDetail::NoCandidateBackendMatched,
                ),
                trace: SelectionTrace {
                    backend_hint: None,
                    selected_backend: None,
                    steps: Vec::new(),
                },
                diagnosis: dg_media_avcodec::SelectionFailureDiagnosis::CapabilityTooNarrow,
            }
        })))
    }
}

fn apply_profile_decoder(
    config: DecoderConfig,
    descriptor: &crate::profile::ProfileDescriptor,
) -> DecoderConfig {
    config
        .with_memory_domain(core_memory_domain_to_avcodec(descriptor.memory_domain))
        .with_allow_staging(descriptor.allow_staging)
}

fn apply_profile_encoder(
    config: EncoderConfig,
    descriptor: &crate::profile::ProfileDescriptor,
) -> EncoderConfig {
    config
        .with_memory_domain(core_memory_domain_to_avcodec(descriptor.memory_domain))
        .with_allow_staging(descriptor.allow_staging)
}

fn apply_profile_processor(
    config: ImageProcessorConfig,
    descriptor: &crate::profile::ProfileDescriptor,
    target_op: ImageOpKind,
) -> ImageProcessorConfig {
    config
        .with_memory_domain(core_memory_domain_to_avcodec(descriptor.memory_domain))
        .with_allow_staging(descriptor.allow_staging)
        .with_target_op(target_op)
}

fn core_memory_domain_to_avcodec(value: MemoryDomain) -> dg_media_avcodec::MemoryDomain {
    crate::bridge::core_memory_domain_to_avcodec(value)
}

fn build_report(
    profile: AvcodecProfile,
    role: SessionRole,
    descriptor: &crate::profile::ProfileDescriptor,
    trace: SelectionTrace,
) -> SessionBuildReport {
    SessionBuildReport {
        profile_name: profile.name(),
        role,
        selected_backend: trace.selected_backend,
        memory_domain: descriptor.memory_domain,
        allow_staging: descriptor.allow_staging,
        trace,
    }
}

fn map_selection_failure(report: SelectionFailureReport) -> Error {
    use crate::{media_error_with_context, MediaErrorContext, MediaOperation};

    let backend = report
        .trace
        .selected_backend
        .or(report.trace.backend_hint)
        .unwrap_or("none");
    let detail = format!(
        "session selection failed: {}; diagnosis={:?}",
        crate::avcodec::map_av_error(report.error),
        report.diagnosis
    );
    media_error_with_context(
        MediaErrorContext::new("Unsupported", MediaOperation::Select, detail)
            .with_backend(backend)
            .with_role("session"),
    )
}

#[cfg(test)]
mod tests {
    use dg_media_avcodec::{CodecId, TimeBase};

    use super::AvcodecSessionBuilder;
    use crate::profile::AvcodecProfile;

    #[test]
    fn native_free_decoder_builds_with_trace() {
        let registry = dg_media_avcodec::default_registry_builder().build();
        let builder = AvcodecSessionBuilder::new(&registry, AvcodecProfile::NativeFree);
        let config = dg_media_avcodec::DecoderConfig::new(CodecId::Jpeg, TimeBase::new(1, 25));
        let (_decoder, report) = builder
            .build_decoder(config)
            .expect("jpeg decoder must be available in native-free profile");
        assert_eq!(report.profile_name, "native-free");
        assert!(report.selected_backend.is_some());
    }
}
