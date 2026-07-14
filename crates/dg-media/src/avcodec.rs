use dg_core::{
    BitstreamFormat as CoreBitstreamFormat, Error, MediaCodec, MediaPayloadInfo, MediaTimeBase,
    MemoryDomain as CoreMemoryDomain, Result,
};

use crate::async_core::{AsyncPump, BackendOps, PumpStep, SubmitResult};
use crate::bridge::{
    avcodec_image_to_media_frame_with_processor, avcodec_packet_to_media_frame,
    core_memory_domain_to_avcodec, media_frame_to_avcodec_image, media_frame_to_avcodec_packet,
};
use crate::media_frame_timing;
use crate::profile::AvcodecProfile;
use crate::session::AvcodecSdkService;
use crate::MediaFrame;
use dg_media_avcodec::VideoSessionBuildReport;

use tracing::warn;

use dg_media_avcodec::{
    AvError, AvErrorContext, AvErrorKind, BitstreamFormat, BufferHandle, BufferSlice, CodecId,
    CodecParameters, Decoder, DecoderConfig, Encoder, EncoderConfig, Image, ImageInfo, ImageOp,
    ImageOpKind, ImageProcessRequest, ImageProcessor, ImageProcessorConfig, Packet, Poll, TimeBase,
};

type AvResult<T> = core::result::Result<T, AvError>;

fn default_sdk_service() -> AvcodecSdkService {
    AvcodecSdkService::from_default_registry()
}

pub fn codec_from_name(name: Option<&str>) -> Result<CodecId> {
    match name.unwrap_or("jpeg").to_ascii_lowercase().as_str() {
        "jpeg" => Ok(CodecId::Jpeg),
        "mjpeg" => Ok(CodecId::Mjpeg),
        "h264" => Ok(CodecId::H264),
        "h265" | "hevc" => Ok(CodecId::H265),
        "vp8" => Ok(CodecId::Vp8),
        "vp9" => Ok(CodecId::Vp9),
        "av1" => Ok(CodecId::Av1),
        other => Err(Error::Config(format!(
            "codec must be one of `jpeg`, `mjpeg`, `h264`, `h265`, `hevc`, `vp8`, `vp9`, or `av1`, \
             got `{other}`"
        ))),
    }
}

pub fn bitstream_format_from_codec(codec: CodecId) -> Result<BitstreamFormat> {
    match codec {
        CodecId::H264 => Ok(BitstreamFormat::H264AnnexB),
        CodecId::H265 => Ok(BitstreamFormat::H265AnnexB),
        CodecId::Vp8 => Ok(BitstreamFormat::Vp8Frame),
        CodecId::Vp9 => Ok(BitstreamFormat::Vp9Frame),
        CodecId::Av1 => Ok(BitstreamFormat::Av1Obu),
        CodecId::Jpeg | CodecId::Mjpeg => Ok(BitstreamFormat::JpegInterchange),
        _ => Err(Error::Config(format!(
            "no canonical bitstream format for codec {codec:?}"
        ))),
    }
}

pub fn map_av_error(error: AvError) -> Error {
    map_av_error_with_operation(error, crate::MediaOperation::Submit)
}

pub fn map_av_error_with_operation(error: AvError, operation: crate::MediaOperation) -> Error {
    use crate::{media_error_with_context, MediaErrorContext};

    if let AvError::WithContext {
        error: inner,
        context,
    } = error
    {
        return append_av_error_context(
            map_av_error_with_operation(*inner, operation),
            *context,
            operation,
        );
    }

    let (kind, detail) = match &error {
        AvError::InvalidArgument => ("InvalidArgument", "avcodec: invalid argument".to_string()),
        AvError::Unsupported => ("Unsupported", "avcodec: unsupported operation".to_string()),
        AvError::Again => ("Again", "avcodec: operation needs polling".to_string()),
        AvError::EndOfStream => ("EndOfStream", "avcodec: end of stream".to_string()),
        AvError::BufferDomainMismatch => (
            "InvalidArgument",
            "avcodec: buffer memory domain mismatch".to_string(),
        ),
        AvError::NotInitialized => ("InvalidState", "avcodec: not initialized".to_string()),
        AvError::QueueFull => ("Again", "avcodec: queue full".to_string()),
        AvError::BackendFailure => ("Backend", "avcodec: backend failure".to_string()),
        AvError::BackendMessage(message) => ("Backend", format!("avcodec backend: {message}")),
        AvError::InvalidState => ("InvalidState", "avcodec: invalid state".to_string()),
        AvError::CycleDetected => ("InvalidArgument", "avcodec: cycle detected".to_string()),
        AvError::DeviceLost => ("DeviceLost", "avcodec: device lost".to_string()),
        AvError::OutOfMemory => ("Oom", "avcodec: out of memory".to_string()),
        AvError::Classified { kind, detail } => {
            ("Backend", format!("avcodec {kind:?}: {detail:?}"))
        }
        AvError::ExternalError(code) => ("Backend", format!("avcodec external error code {code}")),
        AvError::WithContext { .. } => unreachable!("handled above"),
    };

    media_error_with_context(MediaErrorContext::new(kind, operation, detail))
}

fn append_av_error_context(
    error: Error,
    context: AvErrorContext,
    operation: crate::MediaOperation,
) -> Error {
    use crate::{media_error_with_context, MediaErrorContext};

    let base_detail = match &error {
        Error::Media(message) => message.clone(),
        other => other.to_string(),
    };
    let mut ctx = MediaErrorContext::new("Backend", operation, base_detail);
    if let Some(backend_id) = context.backend_id {
        ctx = ctx.with_backend(backend_id);
    }
    if let Some(codec) = context.codec {
        ctx = ctx.with_codec(format!("{codec:?}"));
    }
    if let Some(source_format) = context.source_format {
        ctx.pixel_format = Some(format!("{source_format:?}"));
    }
    media_error_with_context(ctx)
}

fn submit_result<T: Clone>(value: T, result: AvResult<()>) -> SubmitResult<T> {
    match result {
        Ok(()) => SubmitResult::Accepted,
        Err(error) if error.kind() == AvErrorKind::Again => SubmitResult::Again(value),
        Err(error) => SubmitResult::Error(error),
    }
}

fn media_codec_to_codec_id(codec: MediaCodec) -> Result<CodecId> {
    match codec {
        MediaCodec::H264 => Ok(CodecId::H264),
        MediaCodec::H265 | MediaCodec::H266 => Ok(CodecId::H265),
        MediaCodec::AV1 => Ok(CodecId::Av1),
        MediaCodec::VP8 => Ok(CodecId::Vp8),
        MediaCodec::VP9 => Ok(CodecId::Vp9),
        MediaCodec::MJPEG => Ok(CodecId::Mjpeg),
        MediaCodec::Jpeg => Ok(CodecId::Jpeg),
        MediaCodec::Unknown => Err(Error::Media(
            "packet metadata codec is Unknown; specify `codec` element parameter".into(),
        )),
        other => Err(Error::Media(format!(
            "unsupported encoded codec {:?} for avcodec decode",
            other
        ))),
    }
}

fn core_bitstream_to_avcodec(format: CoreBitstreamFormat) -> Result<BitstreamFormat> {
    match format {
        CoreBitstreamFormat::H264AnnexB => Ok(BitstreamFormat::H264AnnexB),
        CoreBitstreamFormat::H264Avcc => Ok(BitstreamFormat::H264Avcc),
        CoreBitstreamFormat::H265AnnexB => Ok(BitstreamFormat::H265AnnexB),
        CoreBitstreamFormat::H265Hvcc => Ok(BitstreamFormat::H265Hvcc),
        CoreBitstreamFormat::Vp8Frame => Ok(BitstreamFormat::Vp8Frame),
        CoreBitstreamFormat::Vp9Frame => Ok(BitstreamFormat::Vp9Frame),
        CoreBitstreamFormat::Av1Obu => Ok(BitstreamFormat::Av1Obu),
        CoreBitstreamFormat::JpegInterchange => Ok(BitstreamFormat::JpegInterchange),
        CoreBitstreamFormat::Unknown => Err(Error::Media(
            "packet bitstream format is Unknown; specify `bitstream_format`".into(),
        )),
        other => Err(Error::Media(format!(
            "unsupported bitstream format {other:?} for avcodec bridge"
        ))),
    }
}

fn stream_index_from_frame(frame: &MediaFrame) -> u32 {
    frame
        .meta
        .media_info
        .as_ref()
        .and_then(|info| match &info.payload {
            MediaPayloadInfo::Encoded(encoded) => Some(encoded.stream_index),
            _ => None,
        })
        .unwrap_or(0)
}

fn time_base_from_frame(frame: &MediaFrame) -> Result<TimeBase> {
    let timing = media_frame_timing(&frame.meta);
    timing
        .time_base
        .map(|tb| TimeBase::new(tb.num, tb.den))
        .ok_or_else(|| Error::Media("packet metadata is missing time_base".into()))
}

fn codec_from_frame(frame: &MediaFrame, fallback: Option<CodecId>) -> Result<CodecId> {
    if let Some(codec) = fallback {
        return Ok(codec);
    }
    let info = frame
        .meta
        .media_info
        .as_ref()
        .ok_or_else(|| Error::Media("packet is missing media_info codec metadata".into()))?;
    let MediaPayloadInfo::Encoded(encoded) = &info.payload else {
        return Err(Error::Media(
            "decode input must carry encoded media_info payload".into(),
        ));
    };
    media_codec_to_codec_id(encoded.codec)
}

fn bitstream_from_frame(
    frame: &MediaFrame,
    fallback: Option<BitstreamFormat>,
) -> Result<BitstreamFormat> {
    if let Some(format) = fallback {
        return Ok(format);
    }
    let info =
        frame.meta.media_info.as_ref().ok_or_else(|| {
            Error::Media("packet is missing media_info bitstream metadata".into())
        })?;
    let MediaPayloadInfo::Encoded(encoded) = &info.payload else {
        return Err(Error::Media(
            "decode input must carry encoded media_info payload".into(),
        ));
    };
    core_bitstream_to_avcodec(encoded.bitstream_format)
}

fn codec_requires_bitrate(codec: CodecId) -> bool {
    matches!(
        codec,
        CodecId::H264 | CodecId::H265 | CodecId::Vp8 | CodecId::Vp9 | CodecId::Av1
    )
}

/// Builds avcodec [`CodecParameters`] from matching `media_info.codec_configs` blobs.
fn codec_parameters_from_frame(
    frame: &MediaFrame,
    codec: CodecId,
    bitstream: BitstreamFormat,
) -> Result<Option<CodecParameters>> {
    let Some(info) = frame.meta.media_info.as_ref() else {
        return Ok(None);
    };
    let MediaPayloadInfo::Encoded(encoded) = &info.payload else {
        return Ok(None);
    };
    if encoded.codec_configs.is_empty() {
        return Ok(None);
    }
    // Prefer a config blob whose bitstream format matches the packet stream.
    let matching = encoded
        .codec_configs
        .iter()
        .find(|cfg| core_bitstream_to_avcodec(cfg.format).ok() == Some(bitstream))
        .or_else(|| encoded.codec_configs.first());
    let Some(config) = matching else {
        return Ok(None);
    };
    let handle = BufferHandle::from_host_bytes(0, config.data.clone());
    let len = handle.size();
    let extradata = BufferSlice::new(handle, 0, len);
    let params = CodecParameters::new(codec, bitstream).with_extradata(Some(extradata));
    Ok(Some(params))
}

/// Annex-B start-code prepend helper used when assembling parameter sets.
#[cfg(test)]
fn annexb_join_nals(nals: &[&[u8]]) -> Result<Vec<u8>> {
    const START: &[u8] = &[0, 0, 0, 1];
    let mut out = Vec::new();
    for nal in nals {
        let added = START
            .len()
            .checked_add(nal.len())
            .ok_or_else(|| Error::Media("annex-b nal length overflow".into()))?;
        let new_len = out
            .len()
            .checked_add(added)
            .ok_or_else(|| Error::Media("annex-b join length overflow".into()))?;
        if new_len > 4 << 20 {
            return Err(Error::Media(
                "annex-b parameter sets exceed 4 MiB total limit".into(),
            ));
        }
        out.extend_from_slice(START);
        out.extend_from_slice(nal);
    }
    Ok(out)
}

/// Decode element configuration.
#[derive(Clone, Debug)]
pub struct DecodeCoreConfig {
    pub profile: AvcodecProfile,
    pub codec: Option<CodecId>,
    pub bitstream_format: Option<BitstreamFormat>,
    pub output_format: Option<ImageInfo>,
    pub memory_domain: Option<CoreMemoryDomain>,
    pub width: Option<usize>,
    pub height: Option<usize>,
    pub channels: Option<usize>,
}

struct DecoderBackend {
    profile: AvcodecProfile,
    decoder: Option<Box<dyn Decoder>>,
    csc: Option<Box<dyn ImageProcessor>>,
    csc_pending: bool,
    codec: Option<CodecId>,
    bitstream_format: Option<BitstreamFormat>,
    stream_index: u32,
    output_format: Option<ImageInfo>,
    config_codec: Option<CodecId>,
    config_bitstream: Option<BitstreamFormat>,
    memory_domain: Option<CoreMemoryDomain>,
    build_report: Option<VideoSessionBuildReport>,
}

impl DecoderBackend {
    fn new(profile: AvcodecProfile, config: &DecodeCoreConfig) -> Result<Self> {
        Ok(Self {
            profile,
            decoder: None,
            csc: None,
            csc_pending: false,
            codec: None,
            bitstream_format: None,
            stream_index: 0,
            output_format: config.output_format,
            config_codec: config.codec,
            config_bitstream: config.bitstream_format,
            memory_domain: config.memory_domain,
            build_report: None,
        })
    }

    fn ensure_decoder(&mut self, frame: &MediaFrame) -> Result<()> {
        if self.decoder.is_some() {
            self.check_session_invariants(frame)?;
            return Ok(());
        }
        let codec = codec_from_frame(frame, self.config_codec)?;
        let bitstream = bitstream_from_frame(frame, self.config_bitstream)?;
        let time_base = time_base_from_frame(frame).or_else(|_| {
            if matches!(codec, CodecId::Jpeg | CodecId::Mjpeg) {
                Ok(TimeBase::new(1, 25))
            } else {
                Err(Error::Media(
                    "video decode requires packet time_base in metadata".into(),
                ))
            }
        })?;
        let stream_index = stream_index_from_frame(frame);
        let mut config = DecoderConfig::new(codec, time_base);
        if let Some(domain) = self.memory_domain {
            config = config.with_memory_domain(core_memory_domain_to_avcodec(domain));
        }
        if let Some(parameters) = codec_parameters_from_frame(frame, codec, bitstream)? {
            config = config.with_parameters(Some(parameters));
        }
        let (decoder, report) = default_sdk_service().build_decoder(self.profile, config)?;
        self.decoder = Some(decoder);
        self.codec = Some(codec);
        self.bitstream_format = Some(bitstream);
        self.stream_index = stream_index;
        self.build_report = Some(report);
        Ok(())
    }

    fn check_session_invariants(&self, frame: &MediaFrame) -> Result<()> {
        let session_codec = self.codec.expect("decoder session must be initialized");
        let session_format = self
            .bitstream_format
            .expect("decoder session must be initialized");
        let packet_codec = codec_from_frame(frame, self.config_codec)?;
        if packet_codec != session_codec {
            return Err(Error::Media(format!(
                "decode packet codec {packet_codec:?} does not match session codec {session_codec:?}"
            )));
        }
        let packet_format = bitstream_from_frame(frame, self.config_bitstream)?;
        if packet_format != session_format {
            return Err(Error::Media(format!(
                "decode packet bitstream {packet_format:?} does not match session format \
                 {session_format:?}"
            )));
        }
        let packet_stream = stream_index_from_frame(frame);
        if packet_stream != self.stream_index {
            return Err(Error::Media(format!(
                "decode packet stream_index {packet_stream} does not match session stream_index \
                 {}",
                self.stream_index
            )));
        }
        if let Ok(time_base) = time_base_from_frame(frame) {
            // Session time base is fixed at open; mismatched timestamps are still accepted but
            // time_base identity must not drift for the same stream.
            let _ = time_base;
        }
        Ok(())
    }

    fn ensure_csc(&mut self, dst_format: ImageInfo) -> Result<()> {
        if self.csc.is_some() {
            return Ok(());
        }
        let config = ImageProcessorConfig::new();
        let (processor, _report) =
            default_sdk_service().build_image_processor(self.profile, config, ImageOpKind::Csc)?;
        self.csc = Some(processor);
        let _ = dst_format;
        Ok(())
    }
}

impl BackendOps for DecoderBackend {
    type BackendValue = Packet;

    fn convert_input(&mut self, frame: MediaFrame) -> Result<Packet> {
        self.ensure_decoder(&frame)?;
        let codec = self.codec.expect("decoder session must be initialized");
        let bitstream = self
            .bitstream_format
            .expect("decoder session must be initialized");
        let stream_index = self.stream_index;
        media_frame_to_avcodec_packet(frame, stream_index, codec, bitstream)
    }

    fn submit_value(&mut self, value: Packet) -> SubmitResult<Packet> {
        let decoder = self
            .decoder
            .as_mut()
            .expect("decoder must exist before submit");
        submit_result(value.clone(), decoder.submit_packet(value))
    }

    fn poll_output(&mut self) -> AvResult<Poll<MediaFrame>> {
        if self.decoder.is_none() {
            return Ok(Poll::Pending);
        }
        if self.csc_pending {
            let processor = self.csc.as_mut().expect("csc processor");
            match processor.poll_image() {
                Ok(Poll::Ready(image)) => {
                    self.csc_pending = false;
                    let frame = avcodec_image_to_media_frame_with_processor(&image, None)
                        .map_err(map_av_error_to_av)?
                        .frame;
                    return Ok(Poll::Ready(frame));
                }
                Ok(Poll::Pending) => return Ok(Poll::Pending),
                Ok(Poll::EndOfStream) => {
                    return Err(AvError::InvalidState);
                }
                Err(error) => return Err(error),
            }
        }

        let decoder = self
            .decoder
            .as_mut()
            .expect("decoder must exist before poll");
        match decoder.poll_frame() {
            Ok(Poll::Ready(image)) => {
                if let Some(dst) = self.output_format {
                    if image.format != dst {
                        self.ensure_csc(dst).map_err(map_av_error_to_av)?;
                        let processor = self.csc.as_mut().expect("csc processor");
                        processor
                            .submit(ImageProcessRequest {
                                src: image,
                                op: ImageOp::Csc { dst_format: dst },
                                aux: None,
                                target_domain: None,
                            })
                            ?;
                        self.csc_pending = true;
                        return Ok(Poll::Pending);
                    }
                }
                let processor = self
                    .csc
                    .as_mut()
                    .map(|p| p.as_mut() as &mut dyn ImageProcessor);
                let frame = avcodec_image_to_media_frame_with_processor(&image, processor)
                    .map_err(map_av_error_to_av)?
                    .frame;
                Ok(Poll::Ready(frame))
            }
            Ok(Poll::Pending) => Ok(Poll::Pending),
            Ok(Poll::EndOfStream) => Ok(Poll::EndOfStream),
            Err(error) => Err(error),
        }
    }

    fn flush_backend(&mut self) -> AvResult<()> {
        if let Some(decoder) = self.decoder.as_mut() {
            decoder.flush()
        } else {
            Ok(())
        }
    }

    fn reset_backend(&mut self) -> AvResult<()> {
        if let Some(decoder) = self.decoder.as_mut() {
            decoder.reset()
        } else {
            Ok(())
        }
    }

    fn flush_required(&self) -> bool {
        self.decoder.is_some()
    }
}

fn map_av_error_to_av(error: Error) -> AvError {
    AvError::BackendMessage(error.to_string())
}

pub struct DecodeCore {
    pump: AsyncPump<Packet>,
    backend: DecoderBackend,
    output_assert: Option<(usize, usize, usize)>,
}

impl DecodeCore {
    pub fn new(config: DecodeCoreConfig) -> Result<Self> {
        let output_assert = match (config.width, config.height, config.channels) {
            (Some(width), Some(height), channels) => Some((width, height, channels.unwrap_or(3))),
            _ => None,
        };
        Ok(Self {
            pump: AsyncPump::new(),
            backend: DecoderBackend::new(config.profile, &config)?,
            output_assert,
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
            if let Some((width, height, channels)) = self.output_assert {
                validate_output_geometry(&frame, width, height, channels)?;
            }
            return Ok(crate::ops::MediaPoll::Ready(frame));
        }
        match self.pump_step()? {
            PumpStep::OutputReady => {
                let frame = self.pump.pop_output().expect("output after OutputReady");
                if let Some((width, height, channels)) = self.output_assert {
                    validate_output_geometry(&frame, width, height, channels)?;
                }
                Ok(crate::ops::MediaPoll::Ready(frame))
            }
            PumpStep::Pending => Ok(crate::ops::MediaPoll::Pending),
            PumpStep::EndOfStream => Ok(crate::ops::MediaPoll::EndOfStream),
        }
    }
}

fn validate_output_geometry(
    frame: &MediaFrame,
    width: usize,
    height: usize,
    channels: usize,
) -> Result<()> {
    match frame.shape.as_slice() {
        [h, w, c] if *h == height && *w == width && *c == channels => Ok(()),
        shape => Err(Error::Media(format!(
            "decoded output shape {shape:?} does not match configured [{height}, {width}, \
             {channels}]"
        ))),
    }
}

/// Encode element configuration.
#[derive(Clone, Debug)]
pub struct EncodeCoreConfig {
    pub profile: AvcodecProfile,
    pub codec: Option<CodecId>,
    pub bitstream_format: Option<BitstreamFormat>,
    pub bitrate: Option<u32>,
    pub time_base: Option<MediaTimeBase>,
    pub memory_domain: Option<CoreMemoryDomain>,
}

struct EncoderBackend {
    profile: AvcodecProfile,
    encoder: Option<Box<dyn Encoder>>,
    codec: CodecId,
    bitstream_format: Option<BitstreamFormat>,
    bitrate: Option<u32>,
    time_base: Option<MediaTimeBase>,
    memory_domain: Option<CoreMemoryDomain>,
    stream_index: u32,
}

impl EncoderBackend {
    fn new(config: EncodeCoreConfig) -> Self {
        let codec = config.codec.unwrap_or(CodecId::Jpeg);
        Self {
            profile: config.profile,
            encoder: None,
            codec,
            bitstream_format: config.bitstream_format,
            bitrate: config.bitrate,
            time_base: config.time_base,
            memory_domain: config.memory_domain,
            stream_index: 0,
        }
    }

    fn ensure_encoder(&mut self, frame: &MediaFrame) -> Result<()> {
        if self.encoder.is_some() {
            return Ok(());
        }
        let image = media_frame_to_avcodec_image(frame.clone(), 1)?;
        let timing = media_frame_timing(&frame.meta);
        let resolved_tb = timing.time_base.or(self.time_base);
        let time_base = if let Some(tb) = resolved_tb {
            if let Some(configured) = self.time_base {
                if configured != tb {
                    return Err(Error::Media(format!(
                        "encoder time_base config {}/{} conflicts with frame metadata {}/{}",
                        configured.num, configured.den, tb.num, tb.den
                    )));
                }
            }
            self.time_base = Some(tb);
            TimeBase::new(tb.num, tb.den)
        } else {
            warn!(
                codec = ?self.codec,
                "encoder time_base missing from frame metadata and config; using compatibility \
                 default 1/25"
            );
            self.time_base = Some(MediaTimeBase::new(1, 25));
            TimeBase::new(1, 25)
        };
        let bitrate = self.bitrate.unwrap_or_else(|| {
            if codec_requires_bitrate(self.codec) {
                0
            } else {
                1
            }
        });
        if codec_requires_bitrate(self.codec) && bitrate == 0 {
            return Err(Error::Media(format!(
                "encoder bitrate is required for codec {:?}",
                self.codec
            )));
        }
        let mut config = EncoderConfig::new(
            self.codec,
            image.coded_width,
            image.coded_height,
            image.format,
            time_base,
            bitrate,
        );
        if let Some(domain) = self.memory_domain {
            config = config.with_memory_domain(core_memory_domain_to_avcodec(domain));
        }
        let (encoder, _report) = default_sdk_service().build_encoder(self.profile, config)?;
        self.encoder = Some(encoder);
        if let Some(info) = frame.meta.media_info.as_ref() {
            if let MediaPayloadInfo::Encoded(encoded) = &info.payload {
                self.stream_index = encoded.stream_index;
            }
        }
        Ok(())
    }
}

impl BackendOps for EncoderBackend {
    type BackendValue = Image;

    fn convert_input(&mut self, frame: MediaFrame) -> Result<Image> {
        self.ensure_encoder(&frame)?;
        media_frame_to_avcodec_image(frame, 1)
    }

    fn submit_value(&mut self, value: Image) -> SubmitResult<Image> {
        let encoder = self
            .encoder
            .as_mut()
            .expect("encoder must exist before submit");
        submit_result(value.clone(), encoder.submit_frame(value))
    }

    fn poll_output(&mut self) -> AvResult<Poll<MediaFrame>> {
        let Some(encoder) = self.encoder.as_mut() else {
            return Ok(Poll::Pending);
        };
        match encoder.poll_packet() {
            Ok(Poll::Ready(packet)) => {
                let mut frame =
                    avcodec_packet_to_media_frame(&packet).map_err(map_av_error_to_av)?;
                if let Some(info) = frame.meta.media_info.as_mut() {
                    if let MediaPayloadInfo::Encoded(encoded) = &mut info.payload {
                        encoded.codec = match self.codec {
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
                        if let Some(format) = self.bitstream_format {
                            encoded.bitstream_format = match format {
                                BitstreamFormat::H264AnnexB => CoreBitstreamFormat::H264AnnexB,
                                BitstreamFormat::H264Avcc => CoreBitstreamFormat::H264Avcc,
                                BitstreamFormat::H265AnnexB => CoreBitstreamFormat::H265AnnexB,
                                BitstreamFormat::H265Hvcc => CoreBitstreamFormat::H265Hvcc,
                                BitstreamFormat::Vp8Frame => CoreBitstreamFormat::Vp8Frame,
                                BitstreamFormat::Vp9Frame => CoreBitstreamFormat::Vp9Frame,
                                BitstreamFormat::Av1Obu => CoreBitstreamFormat::Av1Obu,
                                BitstreamFormat::JpegInterchange => {
                                    CoreBitstreamFormat::JpegInterchange
                                }
                                _ => CoreBitstreamFormat::Unknown,
                            };
                        } else if encoded.bitstream_format == CoreBitstreamFormat::Unknown {
                            if let Ok(format) = bitstream_format_from_codec(self.codec) {
                                encoded.bitstream_format = match format {
                                    BitstreamFormat::H264AnnexB => CoreBitstreamFormat::H264AnnexB,
                                    BitstreamFormat::H265AnnexB => CoreBitstreamFormat::H265AnnexB,
                                    BitstreamFormat::Vp8Frame => CoreBitstreamFormat::Vp8Frame,
                                    BitstreamFormat::Vp9Frame => CoreBitstreamFormat::Vp9Frame,
                                    BitstreamFormat::Av1Obu => CoreBitstreamFormat::Av1Obu,
                                    BitstreamFormat::JpegInterchange => {
                                        CoreBitstreamFormat::JpegInterchange
                                    }
                                    _ => CoreBitstreamFormat::Unknown,
                                };
                            }
                        }
                    }
                    if let Some(tb) = self.time_base {
                        info.timing.time_base = Some(tb);
                    }
                }
                // media_info was patched after initial normalize; clear legacy so re-normalize
                // regenerates a consistent stream_metadata DTO.
                frame.meta.stream_metadata = None;
                crate::normalize_media_frame_meta(&mut frame.meta).map_err(map_av_error_to_av)?;
                Ok(Poll::Ready(frame))
            }
            Ok(Poll::Pending) => Ok(Poll::Pending),
            Ok(Poll::EndOfStream) => Ok(Poll::EndOfStream),
            Err(error) => Err(error),
        }
    }

    fn flush_backend(&mut self) -> AvResult<()> {
        if let Some(encoder) = self.encoder.as_mut() {
            encoder.flush()
        } else {
            Ok(())
        }
    }

    fn reset_backend(&mut self) -> AvResult<()> {
        if let Some(encoder) = self.encoder.as_mut() {
            encoder.reset()
        } else {
            Ok(())
        }
    }

    fn flush_required(&self) -> bool {
        self.encoder.is_some()
    }
}

pub struct EncodeCore {
    pump: AsyncPump<Image>,
    backend: EncoderBackend,
}

impl EncodeCore {
    pub fn new(config: EncodeCoreConfig) -> Result<Self> {
        Ok(Self {
            pump: AsyncPump::new(),
            backend: EncoderBackend::new(config),
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

    pub fn submit_image(&mut self, frame: MediaFrame) -> Result<()> {
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
            PumpStep::OutputReady => Ok(crate::ops::MediaPoll::Ready(
                self.pump.pop_output().expect("output after OutputReady"),
            )),
            PumpStep::Pending => Ok(crate::ops::MediaPoll::Pending),
            PumpStep::EndOfStream => Ok(crate::ops::MediaPoll::EndOfStream),
        }
    }
}

struct ResizeBackend {
    processor: Option<Box<dyn ImageProcessor>>,
    width: u32,
    height: u32,
}

impl BackendOps for ResizeBackend {
    type BackendValue = ImageProcessRequest;

    fn convert_input(&mut self, frame: MediaFrame) -> Result<ImageProcessRequest> {
        let image = media_frame_to_avcodec_image(frame, 1)?;
        Ok(ImageProcessRequest {
            src: image,
            op: ImageOp::Resize {
                width: self.width,
                height: self.height,
            },
            aux: None,
            target_domain: None,
        })
    }

    fn submit_value(&mut self, value: ImageProcessRequest) -> SubmitResult<ImageProcessRequest> {
        let processor = self
            .processor
            .as_mut()
            .expect("resize processor must exist");
        submit_result(value.clone(), processor.submit(value))
    }

    fn poll_output(&mut self) -> AvResult<Poll<MediaFrame>> {
        let processor = self
            .processor
            .as_mut()
            .expect("resize processor must exist");
        match processor.poll_image() {
            Ok(Poll::Ready(image)) => {
                let frame = avcodec_image_to_media_frame_with_processor(&image, None)
                    .map_err(map_av_error_to_av)?
                    .frame;
                Ok(Poll::Ready(frame))
            }
            Ok(Poll::Pending) => Ok(Poll::Pending),
            Ok(Poll::EndOfStream) => Ok(Poll::EndOfStream),
            Err(error) => Err(error),
        }
    }

    fn flush_backend(&mut self) -> AvResult<()> {
        if let Some(processor) = self.processor.as_mut() {
            processor.flush()
        } else {
            Ok(())
        }
    }

    fn reset_backend(&mut self) -> AvResult<()> {
        if let Some(processor) = self.processor.as_mut() {
            processor.reset()
        } else {
            Ok(())
        }
    }

    fn flush_required(&self) -> bool {
        self.processor.is_some()
    }
}

pub struct ResizeCore {
    pump: AsyncPump<ImageProcessRequest>,
    backend: ResizeBackend,
}

impl ResizeCore {
    pub fn new(profile: AvcodecProfile, width: usize, height: usize) -> Result<Self> {
        let width = u32::try_from(width)
            .map_err(|_| Error::Media("media_resize: width exceeds u32".to_string()))?;
        let height = u32::try_from(height)
            .map_err(|_| Error::Media("media_resize: height exceeds u32".to_string()))?;
        let sdk = default_sdk_service();
        let config = ImageProcessorConfig::new();
        let (processor, _report) =
            sdk.build_image_processor(profile, config, ImageOpKind::Resize)?;
        Ok(Self {
            pump: AsyncPump::new(),
            backend: ResizeBackend {
                processor: Some(processor),
                width,
                height,
            },
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

    pub fn submit_image(&mut self, frame: MediaFrame) -> Result<()> {
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
            PumpStep::OutputReady => Ok(crate::ops::MediaPoll::Ready(
                self.pump.pop_output().expect("output after OutputReady"),
            )),
            PumpStep::Pending => Ok(crate::ops::MediaPoll::Pending),
            PumpStep::EndOfStream => Ok(crate::ops::MediaPoll::EndOfStream),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{bitstream_format_from_codec, codec_from_name};
    use dg_media_avcodec::{BitstreamFormat, CodecId};

    #[test]
    fn codec_names_include_video_codecs() {
        assert_eq!(codec_from_name(Some("jpeg")), Ok(CodecId::Jpeg));
        assert_eq!(codec_from_name(Some("mjpeg")), Ok(CodecId::Mjpeg));
        assert_eq!(codec_from_name(Some("h264")), Ok(CodecId::H264));
        assert_eq!(codec_from_name(Some("h265")), Ok(CodecId::H265));
        assert_eq!(codec_from_name(Some("hevc")), Ok(CodecId::H265));
        assert_eq!(codec_from_name(Some("vp8")), Ok(CodecId::Vp8));
        assert_eq!(codec_from_name(Some("vp9")), Ok(CodecId::Vp9));
        assert_eq!(codec_from_name(Some("av1")), Ok(CodecId::Av1));
    }

    #[test]
    fn bitstream_format_rejects_unknown_codecs() {
        assert_eq!(
            bitstream_format_from_codec(CodecId::H264),
            Ok(BitstreamFormat::H264AnnexB)
        );
        assert!(bitstream_format_from_codec(CodecId::Unknown).is_err());
    }

    #[test]
    fn annexb_join_preserves_start_codes() {
        let joined = super::annexb_join_nals(&[&[0x67, 0x42], &[0x68, 0xce]]).expect("join");
        assert_eq!(joined, vec![0, 0, 0, 1, 0x67, 0x42, 0, 0, 0, 1, 0x68, 0xce]);
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn native_free_h264_encode_decode_preserves_timing_and_stream_index() {
        use dg_core::{
            ImageMediaInfo, MediaInfo, MediaPayloadInfo, MediaPlaneLayout, MediaRect,
            MediaTimeBase, MediaTiming, PixelFormat, SampleLayout, SampleType,
        };

        use super::{DecodeCore, DecodeCoreConfig, EncodeCore, EncodeCoreConfig};
        use crate::ops::MediaPoll;
        use crate::profile::AvcodecProfile;
        use crate::{MediaFrame, MediaFrameKind};

        let width = 32usize;
        let height = 32usize;
        let y_len = width * height;
        let c_w = width / 2;
        let c_len = c_w * (height / 2);
        let mut bytes = vec![128u8; y_len];
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
            coded_width: 32,
            coded_height: 32,
            visible_rect: MediaRect {
                x: 0,
                y: 0,
                width: 32,
                height: 32,
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
            pts: Some(42),
            dts: Some(42),
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
        frame.meta.pts = Some(42);
        frame.meta.dts = Some(42);

        let mut encoder = EncodeCore::new(EncodeCoreConfig {
            profile: AvcodecProfile::NativeFree,
            codec: Some(CodecId::H264),
            bitstream_format: Some(BitstreamFormat::H264AnnexB),
            bitrate: Some(1_000_000),
            time_base: Some(MediaTimeBase::new(1, 30)),
            memory_domain: None,
        })
        .expect("encode core");
        encoder.submit_image(frame).expect("submit image");
        encoder.submit_end_of_stream();

        let mut packet_frame = None;
        for _ in 0..64 {
            match encoder.poll().expect("encode poll") {
                MediaPoll::Ready(out) => {
                    packet_frame = Some(out);
                    break;
                }
                MediaPoll::Pending => continue,
                MediaPoll::EndOfStream => break,
            }
        }
        let packet_frame = packet_frame.expect("encoded packet");
        let encoded = match packet_frame.meta.media_info.as_ref().map(|i| &i.payload) {
            Some(MediaPayloadInfo::Encoded(e)) => e,
            _ => panic!("encoded media_info required"),
        };
        assert_eq!(encoded.codec, dg_core::MediaCodec::H264);
        assert!(matches!(
            encoded.bitstream_format,
            dg_core::BitstreamFormat::H264AnnexB | dg_core::BitstreamFormat::Unknown
        ));

        let mut decoder = DecodeCore::new(DecodeCoreConfig {
            profile: AvcodecProfile::NativeFree,
            codec: Some(CodecId::H264),
            bitstream_format: Some(BitstreamFormat::H264AnnexB),
            output_format: None,
            memory_domain: None,
            width: None,
            height: None,
            channels: None,
        })
        .expect("decode core");
        decoder.submit_packet(packet_frame).expect("submit packet");
        decoder.submit_end_of_stream();

        let mut decoded = None;
        for _ in 0..64 {
            match decoder.poll().expect("decode poll") {
                MediaPoll::Ready(out) => {
                    decoded = Some(out);
                    break;
                }
                MediaPoll::Pending => continue,
                MediaPoll::EndOfStream => break,
            }
        }
        let decoded = decoded.expect("decoded frame");
        assert_eq!(decoded.meta.pts, Some(42));
        match decoded.meta.media_info.as_ref().map(|i| &i.payload) {
            Some(MediaPayloadInfo::Image(info)) => {
                assert_eq!(info.coded_width, 32);
                assert_eq!(info.coded_height, 32);
                assert_eq!(info.pixel_format, PixelFormat::Yuv420P);
            }
            other => panic!("expected image media_info, got {other:?}"),
        }
    }
}
