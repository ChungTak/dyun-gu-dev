//! Registered graph elements wrapping the Sans-I/O media cores.
//!
//! Each element is a thin driver: it moves packets between graph ports and a
//! core's submit/poll state machine. All media logic lives in [`crate::ops`].

use std::time::{Duration, Instant};

use dg_graph::{
    CreatedElement, Element, ElementHandle, ElementIo, Error, NodeSpec, ParamField, ParamType,
    PortSchema, Result,
};
use serde_json::{Map, Value};
use tracing::trace;
#[cfg(feature = "avcodec-sdk")]
use tracing::warn;

#[cfg(feature = "avcodec-sdk")]
use crate::avcodec::{
    DecodeCore as AvcodecDecodeCore, DecodeCoreConfig, EncodeCore as AvcodecEncodeCore,
    EncodeCoreConfig, ResizeCore as AvcodecResizeCore,
};

use crate::bridge::{graph_packet_to_media_frame, media_frame_to_graph_packet};
use crate::ops::{DecodeCore, EncodeCore, MediaPoll, OsdBox, OsdCore, ResizeCore};
#[cfg(feature = "avcodec-sdk")]
use crate::profile::{reject_profile_hw_conflict, resolve_profile};
use crate::MediaFrame;

const MEDIA_INPUT: [PortSchema; 1] = [PortSchema {
    name: "in",
    dtype: None,
    required: true,
}];
const MEDIA_OUTPUT: [PortSchema; 1] = [PortSchema {
    name: "out",
    dtype: None,
    required: false,
}];
#[cfg(feature = "avcodec-sdk")]
const DECODE_PARAM_FIELDS: &[&str] = &[
    "width",
    "height",
    "channels",
    "codec",
    "profile",
    "hw",
    "bitstream_format",
    "output_format",
    "memory_domain",
    "drain_timeout_ms",
];
#[cfg(not(feature = "avcodec-sdk"))]
const DECODE_PARAM_FIELDS: &[&str] = &["width", "height", "channels"];
#[cfg(feature = "avcodec-sdk")]
const ENCODE_PARAM_FIELDS: &[&str] = &[
    "codec",
    "profile",
    "hw",
    "bitstream_format",
    "encoder_format",
    "memory_domain",
    "bitrate",
    "time_base_num",
    "time_base_den",
    "drain_timeout_ms",
];
const RESIZE_PARAM_FIELDS: &[&str] = &[
    "width",
    "height",
    #[cfg(feature = "avcodec-sdk")]
    "profile",
    #[cfg(feature = "avcodec-sdk")]
    "memory_domain",
    #[cfg(feature = "avcodec-sdk")]
    "drain_timeout_ms",
];
const OSD_PARAM_FIELDS: &[&str] = &["boxes", "color", "thickness"];

const OSD_BOX_FIELDS: &[&str] = &["x", "y", "width", "height"];
#[cfg(not(feature = "avcodec-sdk"))]
const EMPTY_PARAMS: &[ParamField] = &[];
const DECODE_PARAMS: &[ParamField] = &[
    #[cfg(feature = "avcodec-sdk")]
    ParamField {
        name: "width",
        ty: ParamType::Uint,
        required: false,
    },
    #[cfg(not(feature = "avcodec-sdk"))]
    ParamField {
        name: "width",
        ty: ParamType::Uint,
        required: true,
    },
    #[cfg(feature = "avcodec-sdk")]
    ParamField {
        name: "height",
        ty: ParamType::Uint,
        required: false,
    },
    #[cfg(not(feature = "avcodec-sdk"))]
    ParamField {
        name: "height",
        ty: ParamType::Uint,
        required: true,
    },
    ParamField {
        name: "channels",
        ty: ParamType::Uint,
        required: false,
    },
    #[cfg(feature = "avcodec-sdk")]
    ParamField {
        name: "codec",
        ty: ParamType::Enum(&["jpeg", "mjpeg", "h264", "h265", "hevc", "vp8", "vp9", "av1"]),
        required: false,
    },
    #[cfg(feature = "avcodec-sdk")]
    ParamField {
        name: "profile",
        ty: ParamType::Str,
        required: false,
    },
    #[cfg(feature = "avcodec-sdk")]
    ParamField {
        name: "hw",
        ty: ParamType::Enum(&[
            "auto", "rk", "rockchip", "rknn", "rknpu", "nv", "nvidia", "cuda", "intel", "vaapi",
            "amd", "amf", "sw", "software", "cpu", "none",
        ]),
        required: false,
    },
    #[cfg(feature = "avcodec-sdk")]
    ParamField {
        name: "bitstream_format",
        ty: ParamType::Str,
        required: false,
    },
    #[cfg(feature = "avcodec-sdk")]
    ParamField {
        name: "output_format",
        ty: ParamType::Str,
        required: false,
    },
    #[cfg(feature = "avcodec-sdk")]
    ParamField {
        name: "memory_domain",
        ty: ParamType::Enum(&["host", "dmabuf", "drm_prime", "cuda_device", "mpp_buffer"]),
        required: false,
    },
    #[cfg(feature = "avcodec-sdk")]
    ParamField {
        name: "drain_timeout_ms",
        ty: ParamType::Uint,
        required: false,
    },
];
#[cfg(feature = "avcodec-sdk")]
const ENCODE_PARAMS: &[ParamField] = &[
    ParamField {
        name: "codec",
        ty: ParamType::Enum(&["jpeg", "mjpeg", "h264", "h265", "hevc", "vp8", "vp9", "av1"]),
        required: false,
    },
    ParamField {
        name: "profile",
        ty: ParamType::Str,
        required: false,
    },
    ParamField {
        name: "hw",
        ty: ParamType::Enum(&[
            "auto", "rk", "rockchip", "rknn", "rknpu", "nv", "nvidia", "cuda", "intel", "vaapi",
            "amd", "amf", "sw", "software", "cpu", "none",
        ]),
        required: false,
    },
    ParamField {
        name: "bitstream_format",
        ty: ParamType::Str,
        required: false,
    },
    ParamField {
        name: "encoder_format",
        ty: ParamType::Str,
        required: false,
    },
    ParamField {
        name: "memory_domain",
        ty: ParamType::Enum(&["host", "dmabuf", "drm_prime", "cuda_device", "mpp_buffer"]),
        required: false,
    },
    ParamField {
        name: "bitrate",
        ty: ParamType::Uint,
        required: false,
    },
    ParamField {
        name: "time_base_num",
        ty: ParamType::Uint,
        required: false,
    },
    ParamField {
        name: "time_base_den",
        ty: ParamType::Uint,
        required: false,
    },
    ParamField {
        name: "drain_timeout_ms",
        ty: ParamType::Uint,
        required: false,
    },
];
#[cfg(not(feature = "avcodec-sdk"))]
const ENCODE_PARAMS: &[ParamField] = EMPTY_PARAMS;
const RESIZE_PARAMS: &[ParamField] = &[
    ParamField {
        name: "width",
        ty: ParamType::Uint,
        required: true,
    },
    ParamField {
        name: "height",
        ty: ParamType::Uint,
        required: true,
    },
    #[cfg(feature = "avcodec-sdk")]
    ParamField {
        name: "profile",
        ty: ParamType::Str,
        required: false,
    },
    #[cfg(feature = "avcodec-sdk")]
    ParamField {
        name: "memory_domain",
        ty: ParamType::Enum(&["host", "dmabuf", "drm_prime", "cuda_device", "mpp_buffer"]),
        required: false,
    },
    #[cfg(feature = "avcodec-sdk")]
    ParamField {
        name: "drain_timeout_ms",
        ty: ParamType::Uint,
        required: false,
    },
];

const MAX_PUMP_STEPS_PER_ITERATION: usize = 64;
const DEFAULT_DRAIN_TIMEOUT_MS: u64 = 30_000;
const MIN_DRAIN_TIMEOUT_MS: u64 = 1;
const MAX_DRAIN_TIMEOUT_MS: u64 = 300_000;
const RECV_BACKOFF: Duration = Duration::from_millis(1);
const OSD_PARAMS: &[ParamField] = &[
    ParamField {
        name: "boxes",
        ty: ParamType::Array(&ParamType::Object),
        required: false,
    },
    ParamField {
        name: "color",
        ty: ParamType::Array(&ParamType::Uint),
        required: false,
    },
    ParamField {
        name: "thickness",
        ty: ParamType::Uint,
        required: false,
    },
];

inventory::submit! {
    dg_graph::ElementDescriptor {
        kind: "media_decode",
        input_ports: &MEDIA_INPUT,
        output_ports: &MEDIA_OUTPUT,
        params: DECODE_PARAMS,
        validate: Some(validate_decode),
        create: create_decode,
    }
}
inventory::submit! {
    dg_graph::ElementDescriptor {
        kind: "media_encode",
        input_ports: &MEDIA_INPUT,
        output_ports: &MEDIA_OUTPUT,
        params: ENCODE_PARAMS,
        validate: Some(validate_encode),
        create: create_encode,
    }
}
inventory::submit! {
    dg_graph::ElementDescriptor {
        kind: "media_resize",
        input_ports: &MEDIA_INPUT,
        output_ports: &MEDIA_OUTPUT,
        params: RESIZE_PARAMS,
        validate: Some(validate_resize),
        create: create_resize,
    }
}
inventory::submit! {
    dg_graph::ElementDescriptor {
        kind: "media_osd",
        input_ports: &MEDIA_INPUT,
        output_ports: &MEDIA_OUTPUT,
        params: OSD_PARAMS,
        validate: Some(validate_osd),
        create: create_osd,
    }
}
trait MediaCore: Send {
    fn can_accept_input(&self) -> bool {
        true
    }
    fn has_in_flight(&self) -> bool {
        false
    }
    fn is_flushing(&self) -> bool {
        false
    }
    fn submit(&mut self, frame: MediaFrame) -> dg_core::Result<()>;
    fn submit_end_of_stream(&mut self);
    fn poll(&mut self) -> core::result::Result<MediaPoll, dg_core::Error>;
}

impl MediaCore for DecodeCore {
    fn submit(&mut self, frame: MediaFrame) -> dg_core::Result<()> {
        self.submit_packet(frame)
    }
    fn submit_end_of_stream(&mut self) {
        Self::submit_end_of_stream(self);
    }
    fn poll(&mut self) -> core::result::Result<MediaPoll, dg_core::Error> {
        Ok(Self::poll(self))
    }
}

impl MediaCore for EncodeCore {
    fn submit(&mut self, frame: MediaFrame) -> dg_core::Result<()> {
        self.submit_image(frame)
    }
    fn submit_end_of_stream(&mut self) {
        Self::submit_end_of_stream(self);
    }
    fn poll(&mut self) -> core::result::Result<MediaPoll, dg_core::Error> {
        Ok(Self::poll(self))
    }
}

impl MediaCore for ResizeCore {
    fn submit(&mut self, frame: MediaFrame) -> dg_core::Result<()> {
        self.submit_image(frame)
    }
    fn submit_end_of_stream(&mut self) {
        Self::submit_end_of_stream(self);
    }
    fn poll(&mut self) -> core::result::Result<MediaPoll, dg_core::Error> {
        Ok(Self::poll(self))
    }
}

impl MediaCore for OsdCore {
    fn submit(&mut self, frame: MediaFrame) -> dg_core::Result<()> {
        self.submit_image(frame)
    }
    fn submit_end_of_stream(&mut self) {
        Self::submit_end_of_stream(self);
    }
    fn poll(&mut self) -> core::result::Result<MediaPoll, dg_core::Error> {
        Ok(Self::poll(self))
    }
}

#[cfg(feature = "avcodec-sdk")]
impl MediaCore for AvcodecDecodeCore {
    fn can_accept_input(&self) -> bool {
        self.can_accept_input()
    }

    fn has_in_flight(&self) -> bool {
        self.has_in_flight()
    }

    fn is_flushing(&self) -> bool {
        self.is_flushing()
    }

    fn submit(&mut self, frame: MediaFrame) -> dg_core::Result<()> {
        self.submit_packet(frame)
    }

    fn submit_end_of_stream(&mut self) {
        Self::submit_end_of_stream(self);
    }

    fn poll(&mut self) -> core::result::Result<MediaPoll, dg_core::Error> {
        Self::poll(self)
    }
}

#[cfg(feature = "avcodec-sdk")]
impl MediaCore for AvcodecEncodeCore {
    fn can_accept_input(&self) -> bool {
        self.can_accept_input()
    }

    fn has_in_flight(&self) -> bool {
        self.has_in_flight()
    }

    fn is_flushing(&self) -> bool {
        self.is_flushing()
    }

    fn submit(&mut self, frame: MediaFrame) -> dg_core::Result<()> {
        self.submit_image(frame)
    }

    fn submit_end_of_stream(&mut self) {
        Self::submit_end_of_stream(self);
    }

    fn poll(&mut self) -> core::result::Result<MediaPoll, dg_core::Error> {
        Self::poll(self)
    }
}

#[cfg(feature = "avcodec-sdk")]
impl MediaCore for AvcodecResizeCore {
    fn can_accept_input(&self) -> bool {
        self.can_accept_input()
    }

    fn has_in_flight(&self) -> bool {
        self.has_in_flight()
    }

    fn is_flushing(&self) -> bool {
        self.is_flushing()
    }

    fn submit(&mut self, frame: MediaFrame) -> dg_core::Result<()> {
        self.submit_image(frame)
    }

    fn submit_end_of_stream(&mut self) {
        Self::submit_end_of_stream(self);
    }

    fn poll(&mut self) -> core::result::Result<MediaPoll, dg_core::Error> {
        Self::poll(self)
    }
}

struct MediaElement<C: MediaCore> {
    core: C,
    drain_timeout: Duration,
    /// Graph transport sequence; independent of PTS/DTS (plan 04).
    sequence: u64,
}

enum EmitStatus {
    Emitted,
    Pending,
    EndOfStream,
}

impl<C: MediaCore> MediaElement<C> {
    fn try_emit_one(&mut self, io: &ElementIo) -> std::result::Result<EmitStatus, Error> {
        match self.core.poll().map_err(|err| Error::Element {
            element: io.name.clone(),
            message: err.to_string(),
        })? {
            MediaPoll::Ready(frame) => {
                let mut packet = media_frame_to_graph_packet(frame)?;
                // sequence is a graph transport counter, never PTS.
                packet.meta.sequence = self.sequence;
                self.sequence = self.sequence.saturating_add(1);
                io.send("out", packet)?;
                Ok(EmitStatus::Emitted)
            }
            MediaPoll::Pending => Ok(EmitStatus::Pending),
            MediaPoll::EndOfStream => Ok(EmitStatus::EndOfStream),
        }
    }
}

impl<C: MediaCore> Element for MediaElement<C> {
    fn run(mut self: Box<Self>, io: ElementIo) -> Result<()> {
        trace!(node = %io.name, "running media element");
        let mut drain_deadline = Instant::now() + self.drain_timeout;
        let mut input_closed = false;
        loop {
            let mut made_progress = false;
            for _ in 0..MAX_PUMP_STEPS_PER_ITERATION {
                match self.try_emit_one(&io)? {
                    EmitStatus::Emitted => made_progress = true,
                    EmitStatus::Pending => break,
                    EmitStatus::EndOfStream => {
                        io.broadcast_eos()?;
                        return Ok(());
                    }
                }
            }

            if self.core.can_accept_input() && !input_closed {
                match io.recv("in") {
                    Ok(Some(packet)) => {
                        if packet.is_eos() {
                            self.core.submit_end_of_stream();
                            input_closed = true;
                            drain_deadline = Instant::now() + self.drain_timeout;
                            made_progress = true;
                        } else {
                            let frame = graph_packet_to_media_frame(packet).map_err(|err| {
                                Error::Element {
                                    element: io.name.clone(),
                                    message: err.to_string(),
                                }
                            })?;
                            self.core.submit(frame).map_err(|err| Error::Element {
                                element: io.name.clone(),
                                message: err.to_string(),
                            })?;
                            made_progress = true;
                        }
                    }
                    Ok(None) => {
                        if io.stop.load(std::sync::atomic::Ordering::Relaxed) {
                            return Err(Error::NotRunning);
                        }
                    }
                    Err(err) => return Err(err),
                }
            }

            if !made_progress {
                if self.core.is_flushing() && Instant::now() >= drain_deadline {
                    return Err(Error::Runtime(format!(
                        "media element `{}` drain exceeded {} ms",
                        io.name,
                        self.drain_timeout.as_millis()
                    )));
                }
                if !self.core.has_in_flight() && input_closed {
                    return Err(Error::Runtime(format!(
                        "media element `{}` did not reach end of stream after input eos",
                        io.name
                    )));
                }
                std::thread::sleep(RECV_BACKOFF);
            }
        }
    }
}

fn create_decode(node: &NodeSpec) -> Result<CreatedElement> {
    let drain_timeout = parse_drain_timeout(node)?;
    #[cfg(feature = "avcodec-sdk")]
    let core = AvcodecDecodeCore::new(parse_decode_config(node)?)?;
    #[cfg(not(feature = "avcodec-sdk"))]
    let core = {
        let (width, height, channels) = parse_decode(node)?;
        DecodeCore::new(width, height, channels)
    };
    Ok(CreatedElement {
        element: Box::new(MediaElement {
            core,
            drain_timeout,
            sequence: 0,
        }),
        handle: ElementHandle::None,
    })
}

fn create_encode(node: &NodeSpec) -> Result<CreatedElement> {
    validate_encode(node)?;
    let drain_timeout = parse_drain_timeout(node)?;
    #[cfg(feature = "avcodec-sdk")]
    let core = AvcodecEncodeCore::new(parse_encode_config(node)?)?;
    #[cfg(not(feature = "avcodec-sdk"))]
    let core = EncodeCore::new();
    Ok(CreatedElement {
        element: Box::new(MediaElement {
            core,
            drain_timeout,
            sequence: 0,
        }),
        handle: ElementHandle::None,
    })
}

fn create_resize(node: &NodeSpec) -> Result<CreatedElement> {
    let drain_timeout = parse_drain_timeout(node)?;
    let (width, height) = parse_resize(node)?;
    #[cfg(feature = "avcodec-sdk")]
    let core = AvcodecResizeCore::new(parse_resize_profile(node)?, width, height)?;
    #[cfg(not(feature = "avcodec-sdk"))]
    let core = ResizeCore::new(width, height);
    Ok(CreatedElement {
        element: Box::new(MediaElement {
            core,
            drain_timeout,
            sequence: 0,
        }),
        handle: ElementHandle::None,
    })
}

fn create_osd(node: &NodeSpec) -> Result<CreatedElement> {
    let (boxes, color, thickness) = parse_osd(node)?;
    Ok(CreatedElement {
        element: Box::new(MediaElement {
            core: OsdCore::new(boxes, color, thickness),
            drain_timeout: Duration::from_millis(DEFAULT_DRAIN_TIMEOUT_MS),
            sequence: 0,
        }),
        handle: ElementHandle::None,
    })
}

fn validate_decode(node: &NodeSpec) -> Result<()> {
    #[cfg(feature = "avcodec-sdk")]
    {
        parse_decode_config(node)?;
        parse_drain_timeout(node)?;
    }
    #[cfg(not(feature = "avcodec-sdk"))]
    parse_decode(node)?;
    Ok(())
}

fn validate_encode(node: &NodeSpec) -> Result<()> {
    if node.params.is_null() {
        return Ok(());
    }
    let params = params_object(node)?;
    #[cfg(feature = "avcodec-sdk")]
    {
        reject_unknown_fields(params, ENCODE_PARAM_FIELDS)?;
        parse_encode_config(node)?;
        parse_drain_timeout(node)?;
    }
    #[cfg(not(feature = "avcodec-sdk"))]
    reject_unknown_fields(params, &[])?;
    Ok(())
}

#[cfg(not(feature = "avcodec-sdk"))]
fn parse_decode(node: &NodeSpec) -> Result<(usize, usize, usize)> {
    let params = params_object(node)?;
    reject_unknown_fields(params, DECODE_PARAM_FIELDS)?;
    let width = required_nonzero(params, "width", &node.name)?;
    let height = required_nonzero(params, "height", &node.name)?;
    let channels = read_usize(params, "channels")?.unwrap_or(3);
    ensure_nonzero(channels, "channels")?;
    height
        .checked_mul(width)
        .and_then(|pixels| pixels.checked_mul(channels))
        .ok_or_else(|| Error::Config("image dimensions overflow".to_string()))?;
    Ok((width, height, channels))
}

#[cfg(feature = "avcodec-sdk")]
fn parse_decode_config(node: &NodeSpec) -> Result<DecodeCoreConfig> {
    let params = if node.params.is_null() {
        &Map::new()
    } else {
        params_object(node)?
    };
    reject_unknown_fields(params, DECODE_PARAM_FIELDS)?;
    let profile = resolve_avcodec_profile(params, cfg!(feature = "avcodec"))?;
    let codec = match params.get("codec").and_then(Value::as_str) {
        Some(name) => Some(crate::avcodec::codec_from_name(Some(name))?),
        None => None,
    };
    let width = read_usize(params, "width")?;
    let height = read_usize(params, "height")?;
    let channels = read_usize(params, "channels")?;
    if let Some(width) = width {
        ensure_nonzero(width, "width")?;
    }
    if let Some(height) = height {
        ensure_nonzero(height, "height")?;
    }
    if let Some(channels) = channels {
        ensure_nonzero(channels, "channels")?;
    }
    Ok(DecodeCoreConfig {
        profile,
        codec,
        bitstream_format: parse_bitstream_format(params.get("bitstream_format"))?,
        output_format: parse_image_format(params.get("output_format"))?,
        memory_domain: parse_memory_domain(params.get("memory_domain"))?,
        width,
        height,
        channels,
    })
}

#[cfg(feature = "avcodec-sdk")]
fn parse_encode_config(node: &NodeSpec) -> Result<EncodeCoreConfig> {
    let params = if node.params.is_null() {
        &Map::new()
    } else {
        params_object(node)?
    };
    let profile = resolve_avcodec_profile(params, cfg!(feature = "avcodec"))?;
    let codec = match params.get("codec").and_then(Value::as_str) {
        Some(name) => Some(
            crate::avcodec::codec_from_name(Some(name))
                .map_err(|err| Error::Config(err.to_string()))?,
        ),
        None => None,
    };
    let bitrate = read_u32(params, "bitrate")?;
    if let Some(codec) = codec {
        let needs_bitrate = matches!(
            codec,
            dg_media_avcodec::CodecId::H264
                | dg_media_avcodec::CodecId::H265
                | dg_media_avcodec::CodecId::Vp8
                | dg_media_avcodec::CodecId::Vp9
                | dg_media_avcodec::CodecId::Av1
        );
        if needs_bitrate {
            match bitrate {
                None => {
                    return Err(Error::Config(format!(
                        "encoder bitrate is required for codec {codec:?}"
                    )));
                }
                Some(0) => {
                    return Err(Error::Config(format!(
                        "encoder bitrate must be non-zero for codec {codec:?}"
                    )));
                }
                Some(_) => {}
            }
        } else if bitrate == Some(0) {
            return Err(Error::Config(
                "encoder bitrate must be non-zero when provided".into(),
            ));
        }
    }
    let time_base = read_time_base(params)?;
    Ok(EncodeCoreConfig {
        profile,
        codec,
        bitstream_format: parse_bitstream_format(params.get("bitstream_format"))?,
        bitrate,
        time_base,
        memory_domain: parse_memory_domain(params.get("memory_domain"))?,
    })
}

#[cfg(feature = "avcodec-sdk")]
fn parse_resize_profile(node: &NodeSpec) -> Result<crate::profile::AvcodecProfile> {
    let params = params_object(node)?;
    resolve_avcodec_profile(params, cfg!(feature = "avcodec"))
}

#[cfg(feature = "avcodec-sdk")]
fn resolve_avcodec_profile(
    params: &Map<String, Value>,
    legacy_avcodec_only: bool,
) -> Result<crate::profile::AvcodecProfile> {
    let profile_name = params.get("profile").and_then(Value::as_str);
    let hw_name = params.get("hw").and_then(Value::as_str);
    reject_profile_hw_conflict(profile_name, hw_name)
        .map_err(|err| Error::Config(err.to_string()))?;
    if let Some(hw) = hw_name {
        #[allow(deprecated)]
        let hw = crate::legacy::HwPreference::parse(Some(hw))
            .map_err(|err| Error::Config(err.to_string()))?;
        let (mapped, warning) = crate::legacy::resolve_legacy_hw(hw);
        if let Some(profile) = mapped {
            warn!(profile = %profile.name(), "{warning}");
            if profile.is_compiled() {
                return Ok(profile);
            }
            // Compatibility: when only native-free is built, `hw=auto` → software would
            // hard-fail; fall through to the single compiled default with an extra warning.
            if matches!(
                profile,
                crate::profile::AvcodecProfile::Software
                    | crate::profile::AvcodecProfile::NativeFree
            ) {
                let compiled = crate::profile::compiled_profiles();
                if compiled.len() == 1 {
                    warn!(
                        requested = %profile.name(),
                        fallback = %compiled[0].name(),
                        "legacy hw mapped profile is not compiled; using sole compiled profile"
                    );
                    return Ok(compiled[0]);
                }
            }
            return Err(Error::Config(format!(
                "legacy hw maps to profile `{}` which is not compiled; enable `{}`",
                profile.name(),
                profile.cargo_feature()
            )));
        }
    }
    resolve_profile(profile_name, legacy_avcodec_only).map_err(|err| Error::Config(err.to_string()))
}

#[cfg(feature = "avcodec-sdk")]
fn parse_bitstream_format(
    value: Option<&Value>,
) -> Result<Option<dg_media_avcodec::BitstreamFormat>> {
    let Some(value) = value.and_then(Value::as_str) else {
        return Ok(None);
    };
    let format = match value.to_ascii_lowercase().as_str() {
        "h264_annexb" | "h264-annexb" => dg_media_avcodec::BitstreamFormat::H264AnnexB,
        "h264_avcc" | "h264-avcc" => dg_media_avcodec::BitstreamFormat::H264Avcc,
        "h265_annexb" | "h265-annexb" => dg_media_avcodec::BitstreamFormat::H265AnnexB,
        "h265_hvcc" | "h265-hvcc" => dg_media_avcodec::BitstreamFormat::H265Hvcc,
        "vp8_frame" | "vp8-frame" => dg_media_avcodec::BitstreamFormat::Vp8Frame,
        "vp9_frame" | "vp9-frame" => dg_media_avcodec::BitstreamFormat::Vp9Frame,
        "av1_obu" | "av1-obu" => dg_media_avcodec::BitstreamFormat::Av1Obu,
        "jpeg_interchange" | "jpeg-interchange" => {
            dg_media_avcodec::BitstreamFormat::JpegInterchange
        }
        other => {
            return Err(Error::Config(format!("unknown bitstream_format `{other}`")));
        }
    };
    Ok(Some(format))
}

#[cfg(feature = "avcodec-sdk")]
fn parse_image_format(value: Option<&Value>) -> Result<Option<dg_media_avcodec::ImageInfo>> {
    let Some(value) = value.and_then(Value::as_str) else {
        return Ok(None);
    };
    let format = match value.to_ascii_lowercase().as_str() {
        "rgb24" => dg_media_avcodec::ImageInfo::Rgb24,
        "bgr24" => dg_media_avcodec::ImageInfo::Bgr24,
        "rgba" => dg_media_avcodec::ImageInfo::Rgba,
        "bgra" => dg_media_avcodec::ImageInfo::Bgra,
        "yuv420p" => dg_media_avcodec::ImageInfo::Yuv420p,
        "nv12" => dg_media_avcodec::ImageInfo::Nv12,
        "gray8" => dg_media_avcodec::ImageInfo::Gray8,
        other => return Err(Error::Config(format!("unknown output_format `{other}`"))),
    };
    Ok(Some(format))
}

#[cfg(feature = "avcodec-sdk")]
fn parse_memory_domain(value: Option<&Value>) -> Result<Option<dg_core::MemoryDomain>> {
    let Some(value) = value.and_then(Value::as_str) else {
        return Ok(None);
    };
    let domain = match value.to_ascii_lowercase().as_str() {
        "host" => dg_core::MemoryDomain::Host,
        "dmabuf" => dg_core::MemoryDomain::DmaBuf,
        "drm_prime" | "drm-prime" => dg_core::MemoryDomain::DrmPrime,
        "cuda_device" | "cuda-device" => dg_core::MemoryDomain::CudaDevice,
        "mpp_buffer" | "mpp-buffer" => dg_core::MemoryDomain::MppBuffer,
        other => return Err(Error::Config(format!("unknown memory_domain `{other}`"))),
    };
    Ok(Some(domain))
}

fn parse_drain_timeout(node: &NodeSpec) -> Result<Duration> {
    if node.params.is_null() {
        return Ok(Duration::from_millis(DEFAULT_DRAIN_TIMEOUT_MS));
    }
    let params = params_object(node)?;
    let Some(value) = read_u64(params, "drain_timeout_ms")? else {
        return Ok(Duration::from_millis(DEFAULT_DRAIN_TIMEOUT_MS));
    };
    if !(MIN_DRAIN_TIMEOUT_MS..=MAX_DRAIN_TIMEOUT_MS).contains(&value) {
        return Err(Error::Config(format!(
            "drain_timeout_ms must be between {MIN_DRAIN_TIMEOUT_MS} and {MAX_DRAIN_TIMEOUT_MS}"
        )));
    }
    Ok(Duration::from_millis(value))
}

fn read_u64(params: &Map<String, Value>, key: &str) -> Result<Option<u64>> {
    match params.get(key) {
        Some(value) => value
            .as_u64()
            .ok_or_else(|| Error::Config(format!("field {key} must be a non-negative integer")))
            .map(Some),
        None => Ok(None),
    }
}

#[cfg(feature = "avcodec-sdk")]
fn read_u32(params: &Map<String, Value>, key: &str) -> Result<Option<u32>> {
    match read_u64(params, key)? {
        Some(value) => u32::try_from(value)
            .map(Some)
            .map_err(|_| Error::Config(format!("field {key} overflow"))),
        None => Ok(None),
    }
}

#[cfg(feature = "avcodec-sdk")]
fn read_time_base(params: &Map<String, Value>) -> Result<Option<dg_core::MediaTimeBase>> {
    let num = read_u32(params, "time_base_num")?;
    let den = read_u32(params, "time_base_den")?;
    match (num, den) {
        (None, None) => Ok(None),
        (Some(num), Some(den)) => Ok(Some(dg_core::MediaTimeBase::new(num, den))),
        _ => Err(Error::Config(
            "time_base_num and time_base_den must both be set".into(),
        )),
    }
}

fn validate_resize(node: &NodeSpec) -> Result<()> {
    parse_resize(node).map(|_| ())
}

fn parse_resize(node: &NodeSpec) -> Result<(usize, usize)> {
    let params = params_object(node)?;
    reject_unknown_fields(params, RESIZE_PARAM_FIELDS)?;
    let width = required_nonzero(params, "width", &node.name)?;
    let height = required_nonzero(params, "height", &node.name)?;
    height
        .checked_mul(width)
        .ok_or_else(|| Error::Config("image dimensions overflow".to_string()))?;
    Ok((width, height))
}

fn validate_osd(node: &NodeSpec) -> Result<()> {
    parse_osd(node).map(|_| ())
}

fn parse_osd(node: &NodeSpec) -> Result<(Vec<OsdBox>, Vec<u8>, usize)> {
    let params = params_object(node)?;
    reject_unknown_fields(params, OSD_PARAM_FIELDS)?;
    let boxes = read_boxes(params, &node.name)?;
    let color = read_u8_array(params, "color")?.unwrap_or_else(|| vec![255, 0, 0]);
    if color.is_empty() {
        return Err(Error::Config("field color must not be empty".to_string()));
    }
    let thickness = read_usize(params, "thickness")?.unwrap_or(1);
    ensure_nonzero(thickness, "thickness")?;
    Ok((boxes, color, thickness))
}

fn params_object(node: &NodeSpec) -> Result<&Map<String, Value>> {
    node.params
        .as_object()
        .ok_or_else(|| Error::Config(format!("node {} params must be an object", node.name)))
}

fn reject_unknown_fields(params: &Map<String, Value>, allowed: &[&str]) -> Result<()> {
    for key in params.keys() {
        if !allowed.contains(&key.as_str()) {
            let message = if allowed.is_empty() {
                format!("unknown field `{key}`; no parameters are supported")
            } else {
                format!(
                    "unknown field `{key}`; expected one of {}",
                    allowed.join(", ")
                )
            };
            return Err(Error::Config(message));
        }
    }
    Ok(())
}

fn required_nonzero(params: &Map<String, Value>, key: &str, node: &str) -> Result<usize> {
    let value = read_usize(params, key)?
        .ok_or_else(|| Error::Config(format!("node {node}: field {key} is required")))?;
    ensure_nonzero(value, key)
}

fn ensure_nonzero(value: usize, key: &str) -> Result<usize> {
    if value == 0 {
        return Err(Error::Config(format!("field {key} must be non-zero")));
    }
    Ok(value)
}

fn read_usize(params: &Map<String, Value>, key: &str) -> Result<Option<usize>> {
    match params.get(key) {
        Some(value) => value
            .as_u64()
            .ok_or_else(|| Error::Config(format!("field {key} must be a non-negative integer")))
            .and_then(|value| {
                usize::try_from(value).map_err(|_| Error::Config(format!("field {key} overflow")))
            })
            .map(Some),
        None => Ok(None),
    }
}

fn read_u8_array(params: &Map<String, Value>, key: &str) -> Result<Option<Vec<u8>>> {
    match params.get(key) {
        Some(value) => {
            let array = value
                .as_array()
                .ok_or_else(|| Error::Config(format!("field {key} must be an array")))?;
            let values = array
                .iter()
                .map(|value| {
                    value
                        .as_u64()
                        .ok_or_else(|| Error::Config(format!("field {key} must contain integers")))
                        .and_then(|v| {
                            u8::try_from(v)
                                .map_err(|_| Error::Config(format!("field {key} overflow")))
                        })
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(Some(values))
        }
        None => Ok(None),
    }
}

fn read_boxes(params: &Map<String, Value>, node: &str) -> Result<Vec<OsdBox>> {
    let Some(value) = params.get("boxes") else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or_else(|| Error::Config(format!("node {node}: field boxes must be an array")))?;
    array
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let object = entry
                .as_object()
                .ok_or_else(|| Error::Config(format!("node {node}: each box must be an object")))?;
            reject_unknown_fields(object, OSD_BOX_FIELDS).map_err(|err| match err {
                Error::Config(message) => {
                    Error::Config(format!("node {node}: field boxes[{index}]: {message}"))
                }
                other => other,
            })?;
            let field = |key: &str| -> Result<usize> {
                read_usize(object, key)?.ok_or_else(|| {
                    Error::Config(format!("node {node}: box field {key} is required"))
                })
            };
            let x = field("x")?;
            let y = field("y")?;
            let width = ensure_nonzero(field("width")?, "boxes[].width")?;
            let height = ensure_nonzero(field("height")?, "boxes[].height")?;
            x.checked_add(width)
                .ok_or_else(|| Error::Config("box horizontal extent overflow".to_string()))?;
            y.checked_add(height)
                .ok_or_else(|| Error::Config("box vertical extent overflow".to_string()))?;
            Ok(OsdBox {
                x,
                y,
                width,
                height,
            })
        })
        .collect()
}
