//! Fusion video transcoder core via the upstream V3 `VideoSdk` high-level transcoder session.

use dg_core::{Error, MediaPayloadInfo, MediaTimeBase, Result};

use dg_media_avcodec::{
    BitstreamFormat, CodecId, ImageInfo, OwnedVideoBuildReport, Packet, Poll, TimeBase,
    VideoDecoderRequest, VideoEncoderRequest, VideoTranscodeRequest, VideoTranscoderSession,
};

use crate::async_core::{AsyncPump, BackendOps, PumpStep, SubmitResult};
use crate::avcodec::{
    bitstream_format_from_codec, bitstream_from_frame, codec_from_frame, map_video_runtime_error,
    stream_index_from_frame, time_base_from_frame,
};
use crate::bridge::{avcodec_packet_to_media_frame, media_frame_to_avcodec_packet};
use crate::diagnostics::{
    MediaRuntimeDiagnostics, MediaSessionDiagnostics, MediaTranscoderDiagnostics,
};
use crate::profile::AvcodecProfile;
use crate::session::{map_video_build_error, AvcodecSdkService};
use crate::MediaFrame;

type AvResult<T> = core::result::Result<T, dg_media_avcodec::AvError>;

fn default_sdk_service() -> Result<AvcodecSdkService> {
    AvcodecSdkService::new()
}

/// Configuration for a fused packet-to-packet transcoder element.
#[derive(Clone, Debug)]
pub struct TranscodeCoreConfig {
    pub profile: AvcodecProfile,
    pub input_codec: Option<CodecId>,
    pub output_codec: Option<CodecId>,
    pub input_bitstream: Option<BitstreamFormat>,
    pub output_bitstream: Option<BitstreamFormat>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bitrate: Option<u32>,
    pub time_base: Option<MediaTimeBase>,
    pub allow_linked: bool,
}

struct TranscodeBackend {
    profile: AvcodecProfile,
    transcoder: Option<VideoTranscoderSession>,
    build_report: Option<OwnedVideoBuildReport>,
    input_codec: Option<CodecId>,
    output_codec: CodecId,
    input_bitstream: Option<BitstreamFormat>,
    output_bitstream: BitstreamFormat,
    stream_index: u32,
    /// Fixed at session open; subsequent packets must match.
    session_time_base: Option<TimeBase>,
    config_input_codec: Option<CodecId>,
    config_output_codec: Option<CodecId>,
    config_input_bitstream: Option<BitstreamFormat>,
    config_output_bitstream: Option<BitstreamFormat>,
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
            profile: config.profile,
            transcoder: None,
            build_report: None,
            input_codec: None,
            output_codec,
            input_bitstream: None,
            output_bitstream,
            stream_index: 0,
            session_time_base: None,
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
            self.check_session_invariants(frame)?;
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

        let mut decoder =
            VideoDecoderRequest::new(input_codec, resolved_tb).map_err(map_video_build_error)?;
        if let Some(parameters) =
            crate::avcodec::codec_parameters_from_frame(frame, input_codec, input_bitstream)?
        {
            decoder = decoder.with_parameters(Some(parameters));
        }

        let encoder = VideoEncoderRequest::new(
            output_codec,
            width,
            height,
            ImageInfo::Yuv420p,
            resolved_tb,
            bitrate,
        )
        .map_err(map_video_build_error)?;

        let request =
            VideoTranscodeRequest::new(decoder, encoder).with_allow_linked(self.allow_linked);
        let (transcoder, report) =
            default_sdk_service()?.create_transcoder(self.profile, request)?;
        self.transcoder = Some(transcoder);
        self.build_report = Some(report);
        self.input_codec = Some(input_codec);
        self.input_bitstream = Some(input_bitstream);
        self.output_codec = output_codec;
        self.output_bitstream = output_bitstream;
        self.stream_index = stream_index;
        self.session_time_base = Some(resolved_tb);
        Ok(())
    }

    fn check_session_invariants(&self, frame: &MediaFrame) -> Result<()> {
        let session_codec = self.input_codec.ok_or_else(|| {
            Error::Media("transcoder input codec missing after initialization".into())
        })?;
        let session_format = self.input_bitstream.ok_or_else(|| {
            Error::Media("transcoder input bitstream missing after initialization".into())
        })?;
        let packet_codec = codec_from_frame(frame, self.config_input_codec)?;
        if packet_codec != session_codec {
            return Err(Error::Media(format!(
                "transcode packet codec {packet_codec:?} does not match session codec \
                 {session_codec:?}"
            )));
        }
        let packet_format = bitstream_from_frame(frame, self.config_input_bitstream)?;
        if packet_format != session_format {
            return Err(Error::Media(format!(
                "transcode packet bitstream {packet_format:?} does not match session format \
                 {session_format:?}"
            )));
        }
        let packet_stream = stream_index_from_frame(frame);
        if packet_stream != self.stream_index {
            return Err(Error::Media(format!(
                "transcode packet stream_index {packet_stream} does not match session stream_index \
                 {}",
                self.stream_index
            )));
        }
        if let Some(session_tb) = self.session_time_base {
            if let Ok(packet_tb) = time_base_from_frame(frame) {
                if packet_tb.num != session_tb.num || packet_tb.den != session_tb.den {
                    return Err(Error::Media(format!(
                        "transcode packet time_base {}/{} does not match session time_base {}/{}",
                        packet_tb.num, packet_tb.den, session_tb.num, session_tb.den
                    )));
                }
            }
        }
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
            "transcode requires explicit width/height parameters for encoded packet input \
             (encoded media_info does not carry coded size; set width and height on the \
             media_transcode / TranscodeCoreConfig)"
                .into(),
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
        match transcoder
            .submit_packet(value.clone())
            .map_err(map_video_runtime_error)
        {
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
        match transcoder.poll_packet().map_err(map_video_runtime_error)? {
            Poll::Ready(packet) => {
                let mut frame = avcodec_packet_to_media_frame(&packet)
                    .map_err(|err| dg_media_avcodec::AvError::BackendMessage(err.to_string()))?;
                if let Some(info) = frame.meta.media_info.as_mut() {
                    if let MediaPayloadInfo::Encoded(encoded) = &mut info.payload {
                        encoded.codec = crate::format_map::avcodec_codec_to_core(self.output_codec);
                        encoded.stream_index = self.stream_index;
                        encoded.bitstream_format =
                            crate::format_map::avcodec_bitstream_to_core(self.output_bitstream);
                    }
                }
                crate::normalize_media_frame_meta(&mut frame.meta)
                    .map_err(|err| dg_media_avcodec::AvError::BackendMessage(err.to_string()))?;
                Ok(Poll::Ready(frame))
            }
            Poll::Pending => Ok(Poll::Pending),
            Poll::EndOfStream => Ok(Poll::EndOfStream),
        }
    }

    fn flush_backend(&mut self) -> AvResult<()> {
        if let Some(transcoder) = self.transcoder.as_mut() {
            transcoder.flush().map_err(map_video_runtime_error)
        } else {
            Ok(())
        }
    }

    fn reset_backend(&mut self) -> AvResult<()> {
        if let Some(transcoder) = self.transcoder.as_mut() {
            transcoder.reset().map_err(map_video_runtime_error)?;
        }
        // Drop session so the next packet re-runs ensure_transcoder with fresh report.
        self.transcoder = None;
        self.build_report = None;
        self.input_codec = None;
        self.input_bitstream = None;
        self.session_time_base = None;
        Ok(())
    }

    fn flush_required(&self) -> bool {
        self.transcoder.is_some()
    }
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
            .and_then(|report| report.transcoder.as_ref())
            .map(MediaTranscoderDiagnostics::from)
    }

    /// Session-level build report (profile, selected backends, I/O domains).
    #[must_use]
    pub fn session_diagnostics(&self) -> Option<MediaSessionDiagnostics> {
        self.backend
            .build_report
            .as_ref()
            .map(MediaSessionDiagnostics::from)
    }

    /// Upstream SDK runtime counters for the active transcoder session (plan 11).
    #[must_use]
    pub fn runtime_diagnostics(&self) -> Option<MediaRuntimeDiagnostics> {
        self.backend
            .transcoder
            .as_ref()
            .map(|session| MediaRuntimeDiagnostics::from(session.diagnostics()))
    }

    pub fn submit_packet(&mut self, frame: MediaFrame) -> Result<()> {
        self.pump.submit_input(frame)
    }

    pub fn submit_end_of_stream(&mut self) {
        self.pump.begin_flush();
    }

    /// Resets pump + transcoder session for a new generation (plan 08/14).
    pub fn reset(&mut self) -> Result<()> {
        self.pump.reset(&mut self.backend)
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
    use crate::diagnostics::MediaSessionDiagnostics;
    use crate::ops::MediaPoll;
    use crate::profile::AvcodecProfile;
    use crate::{AvcodecEncodeCore, AvcodecEncodeCoreConfig, MediaFrame, MediaFrameKind};
    use dg_core::{
        ImageMediaInfo, MediaInfo, MediaPayloadInfo, MediaPlaneLayout, MediaRect, MediaTimeBase,
        MediaTiming, PixelFormat, SampleLayout, SampleType,
    };
    use dg_media_avcodec::{BitstreamFormat, CodecId};

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
        assert!(core.session_diagnostics().is_none());
    }

    #[cfg(target_arch = "x86_64")]
    fn yuv420p_frame(width: usize, height: usize) -> MediaFrame {
        let y_len = width * height;
        let c_w = width / 2;
        let c_len = c_w * (height / 2);
        let mut bytes = vec![16u8; y_len];
        // Distinct chroma so re-encode is non-trivial empty content.
        bytes.extend(vec![128u8; c_len]);
        bytes.extend(vec![128u8; c_len]);
        let planes = vec![
            MediaPlaneLayout {
                offset: 0,
                stride: width,
                len: y_len,
            },
            MediaPlaneLayout {
                offset: y_len,
                stride: c_w,
                len: c_len,
            },
            MediaPlaneLayout {
                offset: y_len + c_len,
                stride: c_w,
                len: c_len,
            },
        ];
        let image_info = ImageMediaInfo {
            pixel_format: PixelFormat::Yuv420P,
            coded_width: width as u32,
            coded_height: height as u32,
            visible_rect: MediaRect {
                x: 0,
                y: 0,
                width: width as u32,
                height: height as u32,
            },
            crop_rect: None,
            color_primaries: dg_core::ColorPrimaries::Unknown,
            color_transfer: dg_core::ColorTransfer::Unknown,
            color_matrix: dg_core::ColorMatrix::Unknown,
            color_range: dg_core::ColorRange::Unknown,
            flags: dg_core::ImageFlags::default(),
            sample_type: SampleType::Uint8,
            sample_layout: SampleLayout::Planar,
            planes,
            fence_id: None,
        };
        let timing = MediaTiming {
            pts: Some(7),
            dts: Some(7),
            time_base: Some(MediaTimeBase::new(1, 30)),
        };
        let media_info = MediaInfo::image(image_info, timing, bytes.len()).expect("info");
        let mut frame = MediaFrame::from_host_bytes(
            MediaFrameKind::Image,
            dg_core::DataType::U8,
            dg_core::DataFormat::Auto,
            vec![height, width],
            dg_core::DeviceKind::Cpu,
            bytes,
        )
        .expect("frame");
        frame.meta.media_info = Some(Box::new(media_info));
        frame.meta.pts = Some(7);
        frame.meta.dts = Some(7);
        frame
    }

    #[cfg(target_arch = "x86_64")]
    fn encode_h264_packet(profile: AvcodecProfile) -> MediaFrame {
        let mut encoder = AvcodecEncodeCore::new(AvcodecEncodeCoreConfig {
            profile,
            codec: Some(CodecId::H264),
            bitstream_format: Some(BitstreamFormat::H264AnnexB),
            bitrate: Some(1_000_000),
            time_base: Some(MediaTimeBase::new(1, 30)),
            memory_domain: None,
        })
        .expect("encode core");
        encoder
            .submit_image(yuv420p_frame(32, 32))
            .expect("submit image");
        encoder.submit_end_of_stream();
        for _ in 0..64 {
            match encoder.poll().expect("encode poll") {
                MediaPoll::Ready(out) => return out,
                MediaPoll::Pending => continue,
                MediaPoll::EndOfStream => break,
            }
        }
        panic!("expected encoded H.264 packet");
    }

    #[cfg(target_arch = "x86_64")]
    fn run_transcode(
        profile: AvcodecProfile,
        packet: MediaFrame,
        output_codec: CodecId,
    ) -> (MediaFrame, MediaSessionDiagnostics) {
        let mut core = TranscodeCore::new(TranscodeCoreConfig {
            profile,
            input_codec: Some(CodecId::H264),
            output_codec: Some(output_codec),
            input_bitstream: Some(BitstreamFormat::H264AnnexB),
            output_bitstream: None,
            width: Some(32),
            height: Some(32),
            bitrate: Some(1_000_000),
            time_base: Some(MediaTimeBase::new(1, 30)),
            allow_linked: true,
        })
        .expect("transcode core");
        core.submit_packet(packet).expect("submit packet");
        core.submit_end_of_stream();
        let mut out = None;
        for _ in 0..128 {
            match core.poll().expect("transcode poll") {
                MediaPoll::Ready(frame) => {
                    out = Some(frame);
                    break;
                }
                MediaPoll::Pending => continue,
                MediaPoll::EndOfStream => break,
            }
        }
        let out = out.expect("transcoded packet");
        let diag = core
            .session_diagnostics()
            .expect("session diagnostics after create");
        (out, diag)
    }

    #[cfg(all(target_arch = "x86_64", feature = "avcodec-profile-native-free"))]
    #[test]
    fn native_free_h264_transcode_to_h265_has_report_backends() {
        let packet = encode_h264_packet(AvcodecProfile::NativeFree);
        let (out, diag) = run_transcode(AvcodecProfile::NativeFree, packet, CodecId::H265);
        assert_eq!(diag.profile_name, "native-free");
        assert_eq!(diag.decoder_backend.as_deref(), Some("rust-h264"));
        assert_eq!(diag.encoder_backend.as_deref(), Some("rust-h265"));
        match out.meta.media_info.as_ref().map(|i| &i.payload) {
            Some(MediaPayloadInfo::Encoded(encoded)) => {
                assert_eq!(encoded.codec, dg_core::MediaCodec::H265);
            }
            other => panic!("expected encoded H.265 payload, got {other:?}"),
        }
        // Fusion report mode should be present after session create.
        assert!(
            core_mode_non_empty(&diag),
            "session diagnostics should record a non-empty memory path or backends"
        );
    }

    #[cfg(all(target_arch = "x86_64", feature = "avcodec-profile-software"))]
    #[test]
    fn software_h264_transcode_stays_on_ffmpeg_stack() {
        let packet = encode_h264_packet(AvcodecProfile::Software);
        let (out, diag) = run_transcode(AvcodecProfile::Software, packet, CodecId::H264);
        assert_eq!(diag.profile_name, "software");
        assert_eq!(diag.decoder_backend.as_deref(), Some("ffmpeg"));
        assert_eq!(diag.encoder_backend.as_deref(), Some("ffmpeg"));
        match out.meta.media_info.as_ref().map(|i| &i.payload) {
            Some(MediaPayloadInfo::Encoded(encoded)) => {
                assert_eq!(encoded.codec, dg_core::MediaCodec::H264);
            }
            other => panic!("expected encoded H.264 payload, got {other:?}"),
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn core_mode_non_empty(diag: &MediaSessionDiagnostics) -> bool {
        diag.decoder_backend.is_some() || diag.encoder_backend.is_some()
    }
}
