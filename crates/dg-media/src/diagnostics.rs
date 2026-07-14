//! Owned diagnostics snapshots from SDK session and transcoder build reports.

use dg_media_avcodec::{
    MemoryDomain, VideoIoMemoryPlan, VideoSessionBuildReport, VideoTranscodeModeReport,
    VideoTranscoderBuildReport,
};

/// Owned snapshot of a Factory V2 session build report for logging and CLI export.
///
/// Plan 12: reports Profile, selected backends, I/O domains, staging plan, and legacy warning.
/// Does not include pointers, fds, or media payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaSessionDiagnostics {
    pub profile_name: String,
    pub policy_name: String,
    pub decoder_backend: Option<String>,
    pub encoder_backend: Option<String>,
    pub image_processor_backend: Option<String>,
    pub allow_staging: Option<bool>,
    pub memory_path: String,
    /// Six-direction I/O domains when the SDK report includes a plan.
    pub decoder_packet_input: Option<String>,
    pub decoder_image_output: Option<String>,
    pub processor_image_input: Option<String>,
    pub processor_image_output: Option<String>,
    pub encoder_image_input: Option<String>,
    pub encoder_packet_output: Option<String>,
    pub legacy_warning: Option<String>,
}

impl From<&VideoSessionBuildReport> for MediaSessionDiagnostics {
    fn from(report: &VideoSessionBuildReport) -> Self {
        let (allow_staging, memory_path) = match report.memory_path {
            dg_media_avcodec::VideoMemoryPath::Host { allow_staging } => {
                (Some(allow_staging), "host".to_string())
            }
            dg_media_avcodec::VideoMemoryPath::ZeroCopy { domain } => {
                (Some(false), format!("zero_copy:{domain:?}"))
            }
        };
        let domains = report.io.map(IoDomainLabels::from_plan).unwrap_or_default();
        Self {
            profile_name: report.policy_name.clone(),
            policy_name: report.policy_name.clone(),
            decoder_backend: report.decoder_backend.map(str::to_string),
            encoder_backend: report.encoder_backend.map(str::to_string),
            image_processor_backend: report.image_processor_backend.map(str::to_string),
            allow_staging,
            memory_path,
            decoder_packet_input: domains.decoder_packet_input,
            decoder_image_output: domains.decoder_image_output,
            processor_image_input: domains.processor_image_input,
            processor_image_output: domains.processor_image_output,
            encoder_image_input: domains.encoder_image_input,
            encoder_packet_output: domains.encoder_packet_output,
            legacy_warning: report.legacy_alias_warning.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct IoDomainLabels {
    decoder_packet_input: Option<String>,
    decoder_image_output: Option<String>,
    processor_image_input: Option<String>,
    processor_image_output: Option<String>,
    encoder_image_input: Option<String>,
    encoder_packet_output: Option<String>,
}

impl IoDomainLabels {
    fn from_plan(plan: VideoIoMemoryPlan) -> Self {
        fn label(domain: MemoryDomain) -> String {
            format!("{domain:?}")
        }
        Self {
            decoder_packet_input: Some(label(plan.decoder_packet_input)),
            decoder_image_output: Some(label(plan.decoder_image_output)),
            processor_image_input: plan.processor_image_input.map(label),
            processor_image_output: plan.processor_image_output.map(label),
            encoder_image_input: Some(label(plan.encoder_image_input)),
            encoder_packet_output: Some(label(plan.encoder_packet_output)),
        }
    }
}

/// Owned snapshot of a fusion transcoder build report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaTranscoderDiagnostics {
    pub mode: String,
    pub allow_staging: bool,
    pub adapter_stages: usize,
}

impl From<&VideoTranscoderBuildReport> for MediaTranscoderDiagnostics {
    fn from(report: &VideoTranscoderBuildReport) -> Self {
        let mode = match report.mode {
            VideoTranscodeModeReport::Passthrough => "passthrough".to_string(),
            VideoTranscodeModeReport::Linked { backend } => format!("linked:{backend}"),
            VideoTranscodeModeReport::Adapted => "adapted".to_string(),
        };
        Self {
            mode,
            allow_staging: report.allow_staging,
            adapter_stages: report.adapter_chain.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MediaSessionDiagnostics;
    use dg_media_avcodec::{
        MemoryDomain, SelectionTrace, VideoIoMemoryPlan, VideoMemoryPath, VideoSessionBuildReport,
    };

    #[test]
    fn session_diagnostics_copies_backend_fields() {
        let decoder_trace = SelectionTrace {
            backend_hint: None,
            selected_backend: Some("jpeg"),
            steps: Vec::new(),
        };
        let processor_trace = SelectionTrace {
            backend_hint: None,
            selected_backend: Some("libyuv"),
            steps: Vec::new(),
        };
        let io = VideoIoMemoryPlan::new()
            .with_decoder_packet_input(MemoryDomain::Host)
            .with_decoder_image_output(MemoryDomain::Host)
            .with_encoder_image_input(MemoryDomain::Host)
            .with_encoder_packet_output(MemoryDomain::Host);
        let report = VideoSessionBuildReport::new(
            "native-free".to_string(),
            VideoMemoryPath::Host {
                allow_staging: false,
            },
            Some(&decoder_trace),
            Some(&processor_trace),
            None,
            Some(io),
            None,
        );
        let diag = MediaSessionDiagnostics::from(&report);
        assert_eq!(diag.policy_name, "native-free");
        assert_eq!(diag.decoder_backend.as_deref(), Some("jpeg"));
        assert_eq!(diag.image_processor_backend.as_deref(), Some("libyuv"));
        assert_eq!(diag.allow_staging, Some(false));
        assert_eq!(diag.decoder_image_output.as_deref(), Some("Host"));
        assert!(diag.processor_image_input.is_none());
    }
}
