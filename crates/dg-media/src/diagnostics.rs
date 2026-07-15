//! Owned diagnostics snapshots from SDK session and transcoder build reports.

#[cfg(feature = "avcodec-sdk")]
use dg_media_avcodec::{
    OwnedVideoBuildReport, VideoTranscodeModeReport, VideoTranscoderBuildReport,
};

/// Owned snapshot of a session build report for logging and CLI export.
///
/// Reports Profile, selected backends, I/O domains, staging plan, and legacy warning.
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct IoDomainLabels {
    decoder_packet_input: Option<String>,
    decoder_image_output: Option<String>,
    processor_image_input: Option<String>,
    processor_image_output: Option<String>,
    encoder_image_input: Option<String>,
    encoder_packet_output: Option<String>,
}

#[cfg(feature = "avcodec-sdk")]
impl From<&OwnedVideoBuildReport> for MediaSessionDiagnostics {
    fn from(report: &OwnedVideoBuildReport) -> Self {
        let allow_staging = report.io.as_ref().map(|io| io.allow_staging);
        let memory_path = report
            .io
            .as_ref()
            .map_or_else(|| "none".to_string(), |io| format!("{io:?}"));
        let domains = report
            .io
            .as_ref()
            .map(|io| {
                fn label(domain: dg_media_avcodec::MemoryDomain) -> String {
                    format!("{domain:?}")
                }
                IoDomainLabels {
                    decoder_packet_input: Some(label(io.decoder_packet_input)),
                    decoder_image_output: Some(label(io.decoder_image_output)),
                    processor_image_input: io.processor_image_input.map(label),
                    processor_image_output: io.processor_image_output.map(label),
                    encoder_image_input: Some(label(io.encoder_image_input)),
                    encoder_packet_output: Some(label(io.encoder_packet_output)),
                }
            })
            .unwrap_or_default();

        Self {
            profile_name: report.profile.to_string(),
            policy_name: report.profile.to_string(),
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
            legacy_warning: None,
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

#[cfg(feature = "avcodec-sdk")]
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

#[cfg(feature = "avcodec-sdk")]
mod session_diagnostics_tests {
    #[test]
    fn session_diagnostics_copies_backend_fields() {
        use super::MediaSessionDiagnostics;
        let report = dg_media_avcodec::OwnedVideoBuildReport {
            profile: "native-free",
            intent: dg_media_avcodec::VideoIntent::Decoder,
            decoder_backend: Some("jpeg"),
            encoder_backend: None,
            image_processor_backend: Some("libyuv"),
            io: None,
            fallback_allowed: false,
            transcoder: None,
            selections: Vec::new(),
            role_reports: Vec::new(),
        };
        let diag = MediaSessionDiagnostics::from(&report);
        assert_eq!(diag.policy_name, "native-free");
        assert_eq!(diag.decoder_backend.as_deref(), Some("jpeg"));
        assert_eq!(diag.image_processor_backend.as_deref(), Some("libyuv"));
        assert_eq!(diag.allow_staging, None);
    }
}
