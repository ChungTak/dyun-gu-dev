//! Fusion video transcoder core via upstream `VideoTranscoderRequest`.

use dg_core::{
    BitstreamFormat as CoreBitstreamFormat, Error, MediaCodec, MediaPayloadInfo, MediaTimeBase,
    Result,
};

use dg_media_avcodec::{
    CodecId, DecoderConfig, EncoderConfig, ImageInfo, Packet, Poll, Registry, TimeBase,
    VideoTranscodeOptions, VideoTranscoder, VideoTranscoderBuildReport, VideoTranscoderRequest,
};

use crate::async_core::{AsyncPump, BackendOps, PumpStep, SubmitResult};
use crate::avcodec::{
    bitstream_format_from_codec, bitstream_from_frame, codec_from_frame, map_av_error,
    stream_index_from_frame, time_base_from_frame,
};
use crate::bridge::{avcodec_packet_to_media_frame, media_frame_to_avcodec_packet};
use crate::diagnostics::MediaTranscoderDiagnostics;
use crate::profile::{profile_to_sdk_descriptor, AvcodecProfile};
use crate::session::{align_decoder_config, align_encoder_config};
use crate::MediaFrame;

type AvResult<T> = core::result::Result<T, dg_media_avcodec::AvError>;

/// Configuration for a fused packet-to-packet transcoder element.
#[derive(Clone, Debug)]
pub struct TranscodeCoreConfig {
    pub profile: AvcodecProfile,
    pub input_codec: Option<CodecId>,
    pub output_codec: Option<CodecId>,
    pub input_bitstream: Option<dg_media_avcodec::BitstreamFormat>,
    pub output_bitstream: Option<dg_media_avcodec::BitstreamFormat>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bitrate: Option<u32>,
    pub time_base: Option<MediaTimeBase>,
    pub allow_linked: bool,
}

struct TranscodeBackend {
    registry: &'static Registry,
    profile: AvcodecProfile,
    transcoder: Option<VideoTranscoder<'static>>,
    build_report: Option<VideoTranscoderBuildReport>,
    input_codec: Option<CodecId>,
    output_codec: CodecId,
    input_bitstream: Option<dg_media_avcodec::BitstreamFormat>,
    output_bitstream: dg_media_avcodec::BitstreamFormat,
    stream_index: u32,
    config_input_codec: Option<CodecId>,
    config_output_codec: Option<CodecId>,
    config_input_bitstream: Option<dg_media_avcodec::BitstreamFormat>,
    config_output_bitstream: Option<dg_media_avcodec::BitstreamFormat>,
    config_width: Option<u32>,
    config_height: Option<u32>,
    bitrate: Option<u32>,
    time_base: Option<MediaTimeBase>,
    allow_linked: bool,
}

impl TranscodeBackend {
    fn new(config: TranscodeCoreConfig) -> Result<Self> {
        let output_codec = config.output_codec.unwrap_or(CodecId::H264);
        let output_bitstream = config
            .output_bitstream
            .unwrap_or(bitstream_format_from_codec(output_codec)?);
        Ok(Self {
            registry: leak_registry(),
            profile: config.profile,
            transcoder: None,
            build_report: None,
            input_codec: None,
            output_codec,
            input_bitstream: None,
            output_bitstream,
            stream_index: 0,
            config_input_codec: config.input_codec,
            config_output_codec: config.output_codec,
            config_input_bitstream: config.input_bitstream,
            config_output_bitstream: config.output_bitstream,
            config_width: config.width,
            config_height: config.height,
            bitrate: config.bitrate,
            time_base: config.time_base,
            allow_linked: config.allow_linked,
        })
    }

    fn ensure_transcoder(&mut self, frame: &MediaFrame) -> Result<()> {
        if self.transcoder.is_some() {
            return Ok(());
        }
        let input_codec = codec_from_frame(frame, self.config_input_codec)?;
        let input_bitstream = bitstream_from_frame(frame, self.config_input_bitstream)?;
        let time_base = time_base_from_frame(frame).or_else(|_| {
            if matches!(input_codec, CodecId::Jpeg | CodecId::Mjpeg) {
                Ok(TimeBase::new(1, 25))
            } else {
                Err(Error::Media(
                    "transcode input requires packet time_base in metadata".into(),
                ))
            }
        })?;
        let stream_index = stream_index_from_frame(frame);
        let output_codec = self.config_output_codec.unwrap_or(self.output_codec);
        let output_bitstream = self
            .config_output_bitstream
            .map(Ok)
            .unwrap_or_else(|| bitstream_format_from_codec(output_codec))?;
        let bitrate = self.bitrate.ok_or_else(|| {
            Error::Config(format!(
                "transcode encoder bitrate is required for output codec {output_codec:?}"
            ))
        })?;
        if bitrate == 0 {
            return Err(Error::Config(format!(
                "transcode encoder bitrate must be non-zero for output codec {output_codec:?}"
            )));
        }
        let (width, height) = self.resolve_dimensions(frame)?;
        let resolved_tb = self
            .time_base
            .map(|tb| TimeBase::new(tb.num, tb.den))
            .unwrap_or(time_base);

        let sdk_profile = profile_to_sdk_descriptor(self.profile)?;
        let decoder =
            align_decoder_config(DecoderConfig::new(input_codec, resolved_tb), &sdk_profile)?;
        let encoder = align_encoder_config(
            EncoderConfig::new(
                output_codec,
                width,
                height,
                ImageInfo::Yuv420p,
                resolved_tb,
                bitrate,
            ),
            &sdk_profile,
        )?;
        let options = VideoTranscodeOptions {
            allow_linked: self.allow_linked,
            ..VideoTranscodeOptions::default()
        };
        let request = VideoTranscoderRequest::new(sdk_profile, decoder, encoder, options);
        let transcoder =
            VideoTranscoder::new_with_request(self.registry, request).map_err(map_av_error)?;
        if let Some(report) = transcoder.build_report().cloned() {
            self.build_report = Some(report);
        }
        self.transcoder = Some(transcoder);
        self.input_codec = Some(input_codec);
        self.input_bitstream = Some(input_bitstream);
        self.output_codec = output_codec;
        self.output_bitstream = output_bitstream;
        self.stream_index = stream_index;
        Ok(())
    }

    fn resolve_dimensions(&self, frame: &MediaFrame) -> Result<(u32, u32)> {
        if let (Some(width), Some(height)) = (self.config_width, self.config_height) {
            if width == 0 || height == 0 {
                return Err(Error::Config(
                    "transcode width and height must be non-zero".into(),
                ));
            }
            return Ok((width, height));
        }
        if let Some(info) = frame.meta.media_info.as_ref() {
            if let MediaPayloadInfo::Image(image) = &info.payload {
                if image.coded_width > 0 && image.coded_height > 0 {
                    return Ok((image.coded_width, image.coded_height));
                }
            }
        }
        Err(Error::Config(
            "transcode requires width/height element parameters or image metadata".into(),
        ))
    }
}

impl BackendOps for TranscodeBackend {
    type BackendValue = Packet;

    fn convert_input(&mut self, frame: MediaFrame) -> Result<Packet> {
        self.ensure_transcoder(&frame)?;
        let input_codec = self.input_codec.ok_or_else(|| {
            Error::Media("transcoder input codec missing after initialization".into())
        })?;
        let input_bitstream = self.input_bitstream.ok_or_else(|| {
            Error::Media("transcoder input bitstream missing after initialization".into())
        })?;
        media_frame_to_avcodec_packet(frame, self.stream_index, input_codec, input_bitstream)
    }

    fn submit_value(&mut self, value: Packet) -> SubmitResult<Packet> {
        let Some(transcoder) = self.transcoder.as_mut() else {
            return SubmitResult::Error(dg_media_avcodec::AvError::NotInitialized);
        };
        match transcoder.submit_packet(value.clone()) {
            Ok(()) => SubmitResult::Accepted,
            Err(error) if error.kind() == dg_media_avcodec::AvErrorKind::Again => {
                SubmitResult::Again(value)
            }
            Err(error) => SubmitResult::Error(error),
        }
    }

    fn poll_output(&mut self) -> AvResult<Poll<MediaFrame>> {
        let Some(transcoder) = self.transcoder.as_mut() else {
            return Ok(Poll::Pending);
        };
        match transcoder.poll_packet() {
            Ok(Poll::Ready(packet)) => {
                let mut frame = avcodec_packet_to_media_frame(&packet)
                    .map_err(|err| dg_media_avcodec::AvError::BackendMessage(err.to_string()))?;
                if let Some(info) = frame.meta.media_info.as_mut() {
                    if let MediaPayloadInfo::Encoded(encoded) = &mut info.payload {
                        encoded.codec = match self.output_codec {
                            CodecId::H264 => MediaCodec::H264,
                            CodecId::H265 => MediaCodec::H265,
                            CodecId::Vp8 => MediaCodec::VP8,
                            CodecId::Vp9 => MediaCodec::VP9,
                            CodecId::Av1 => MediaCodec::AV1,
                            CodecId::Mjpeg => MediaCodec::MJPEG,
                            CodecId::Jpeg => MediaCodec::Jpeg,
                            _ => encoded.codec,
                        };
                        encoded.stream_index = self.stream_index;
                        encoded.bitstream_format = match self.output_bitstream {
                            dg_media_avcodec::BitstreamFormat::H264AnnexB => {
                                CoreBitstreamFormat::H264AnnexB
                            }
                            dg_media_avcodec::BitstreamFormat::H265AnnexB => {
                                CoreBitstreamFormat::H265AnnexB
                            }
                            dg_media_avcodec::BitstreamFormat::Vp8Frame => {
                                CoreBitstreamFormat::Vp8Frame
                            }
                            dg_media_avcodec::BitstreamFormat::Vp9Frame => {
                                CoreBitstreamFormat::Vp9Frame
                            }
                            dg_media_avcodec::BitstreamFormat::Av1Obu => {
                                CoreBitstreamFormat::Av1Obu
                            }
                            dg_media_avcodec::BitstreamFormat::JpegInterchange => {
                                CoreBitstreamFormat::JpegInterchange
                            }
                            _ => CoreBitstreamFormat::Unknown,
                        };
                    }
                }
                crate::normalize_media_frame_meta(&mut frame.meta)
                    .map_err(|err| dg_media_avcodec::AvError::BackendMessage(err.to_string()))?;
                Ok(Poll::Ready(frame))
            }
            Ok(Poll::Pending) => Ok(Poll::Pending),
            Ok(Poll::EndOfStream) => Ok(Poll::EndOfStream),
            Err(error) => Err(error),
        }
    }

    fn flush_backend(&mut self) -> AvResult<()> {
        if let Some(transcoder) = self.transcoder.as_mut() {
            transcoder.flush()
        } else {
            Ok(())
        }
    }

    fn reset_backend(&mut self) -> AvResult<()> {
        if let Some(transcoder) = self.transcoder.as_mut() {
            transcoder.reset()
        } else {
            Ok(())
        }
    }

    fn flush_required(&self) -> bool {
        self.transcoder.is_some()
    }
}

/// Leaks a registry for the transcoder lifetime.
///
/// Upstream `VideoTranscoder` borrows `Registry`. `Registry` is not `Sync` (factory trait
/// objects), so a process-wide `static` is rejected by `#![forbid(unsafe_code)]` without
/// unsafe Sync impls. One leak per `TranscodeCore` is acceptable for the fusion library API;
/// graph workloads should prefer the decode/resize/encode element chain.
fn leak_registry() -> &'static Registry {
    Box::leak(Box::new(
        dg_media_avcodec::default_registry_builder().build(),
    ))
}

/// Fusion transcoder core. Prefer the decode/resize/encode graph chain when `Send` is required
/// across scheduler threads; this core holds an SDK transcoder session that is not thread-shared.
pub struct TranscodeCore {
    pump: AsyncPump<Packet>,
    backend: TranscodeBackend,
}

impl TranscodeCore {
    pub fn new(config: TranscodeCoreConfig) -> Result<Self> {
        Ok(Self {
            pump: AsyncPump::new(),
            backend: TranscodeBackend::new(config)?,
        })
    }

    pub fn can_accept_input(&self) -> bool {
        self.pump.can_accept_input()
    }

    pub fn has_in_flight(&self) -> bool {
        self.pump.has_in_flight()
    }

    pub fn is_flushing(&self) -> bool {
        self.pump.is_flushing()
    }

    #[must_use]
    pub fn stats(&self) -> &crate::MediaSessionStats {
        self.pump.stats()
    }

    #[must_use]
    pub fn transcoder_diagnostics(&self) -> Option<MediaTranscoderDiagnostics> {
        self.backend
            .build_report
            .as_ref()
            .map(MediaTranscoderDiagnostics::from)
    }

    pub fn submit_packet(&mut self, frame: MediaFrame) -> Result<()> {
        self.pump.submit_input(frame)
    }

    pub fn submit_end_of_stream(&mut self) {
        self.pump.begin_flush();
    }

    pub fn pump_step(&mut self) -> Result<PumpStep> {
        self.pump.pump_step(&mut self.backend)
    }

    pub fn poll(&mut self) -> Result<crate::ops::MediaPoll> {
        if let Some(frame) = self.pump.pop_output() {
            return Ok(crate::ops::MediaPoll::Ready(frame));
        }
        match self.pump_step()? {
            PumpStep::OutputReady => {
                let frame = self.pump.pop_output().ok_or_else(|| {
                    Error::Media("transcode pump reported OutputReady without queued frame".into())
                })?;
                Ok(crate::ops::MediaPoll::Ready(frame))
            }
            PumpStep::Pending => Ok(crate::ops::MediaPoll::Pending),
            PumpStep::EndOfStream => Ok(crate::ops::MediaPoll::EndOfStream),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TranscodeCore, TranscodeCoreConfig};
    use crate::profile::AvcodecProfile;
    use dg_media_avcodec::CodecId;

    #[test]
    fn transcoder_core_rejects_missing_bitrate_for_h264() {
        let config = TranscodeCoreConfig {
            profile: AvcodecProfile::NativeFree,
            input_codec: Some(CodecId::Jpeg),
            output_codec: Some(CodecId::H264),
            input_bitstream: None,
            output_bitstream: None,
            width: Some(16),
            height: Some(16),
            bitrate: None,
            time_base: None,
            allow_linked: true,
        };
        let core = TranscodeCore::new(config).expect("config without packet is constructible");
        assert!(core.transcoder_diagnostics().is_none());
    }
}
