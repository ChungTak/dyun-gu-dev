//! Graph source/sink elements for stream pull (RTSP / HTTP-FLV) and push
//! (RTMP / WebRTC) endpoints.
//!
//! Elements are registered into the `dg-graph` element inventory under the
//! kinds `rtsp_src`, `httpflv_src`, `rtmp_sink`, and `webrtc_sink`. URL scheme
//! selection is delegated to [`crate::connector`]: `mock://` runs fully
//! in-process, protocol schemes require the feature-gated cheetah runtime.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use dg_core::{
    BitstreamFormat, DataType, EncodedMediaInfo, EncodedPacketFlags, MediaCodec, MediaCodecConfig,
    MediaInfo, MediaKind as CoreMediaKind, MediaPayloadInfo, MediaTimeBase, MediaTiming,
};
use dg_graph::{
    CreatedElement, Element, ElementDescriptor, ElementHandle, ElementIo, NodeSpec, Packet,
    PacketMeta, ParamField, ParamType, PortSchema,
};
use serde_json::{Map, Value};
use tracing::{debug, warn};

use crate::connector::{open_pull, open_push, validate_endpoint_url, PullEndpoint, StreamProtocol};
use crate::hub::{KEYFRAME_TAG, MEDIA_TAG};
use crate::stream::SubscriberSourceSyncExt;
use crate::stream::{
    BackpressurePolicy, DispatchResult, MediaFilter, PublisherOptions, PublisherSink,
    SubscriberOptions,
};
use crate::track::{CodecExtradata, CodecId as TrackCodec, TrackInfo, TrackReadiness};
use dg_media::{MediaFrame, MediaFrameKind};

const PULL_OUTPUT_PORT: PortSchema = PortSchema {
    name: "out",
    dtype: Some(DataType::U8),
    required: false,
};
const PUSH_INPUT_PORT: PortSchema = PortSchema {
    name: "in",
    dtype: None,
    required: true,
};

const PTS_TAG: &str = "pts";
const DTS_TAG: &str = "dts";
const PULL_PARAM_FIELDS: &[&str] = &[
    "url",
    "queue_capacity",
    "backpressure",
    "enable_video",
    "enable_audio",
];
const PUSH_PARAM_FIELDS: &[&str] = &["url", "announce_tracks", "tracks"];
const TRACK_FIELDS: &[&str] = &[
    "track_id",
    "media_kind",
    "codec",
    "aac_rtp_packetization",
    "aac_latm_config_in_band",
    "payload_type",
    "clock_rate",
    "sample_rate",
    "channels",
    "width",
    "height",
    "fps",
    "bitrate",
    "extradata",
    "readiness",
];
const BACKPRESSURE_VALUES: &[&str] = &[
    "drop_droppable_first",
    "drop_until_next_keyframe",
    "disconnect_on_overflow",
];
const PULL_PARAMS: &[ParamField] = &[
    ParamField {
        name: "url",
        ty: ParamType::Str,
        required: true,
    },
    ParamField {
        name: "queue_capacity",
        ty: ParamType::Uint,
        required: false,
    },
    ParamField {
        name: "backpressure",
        ty: ParamType::Enum(BACKPRESSURE_VALUES),
        required: false,
    },
    ParamField {
        name: "enable_video",
        ty: ParamType::Bool,
        required: false,
    },
    ParamField {
        name: "enable_audio",
        ty: ParamType::Bool,
        required: false,
    },
];
const PUSH_PARAMS: &[ParamField] = &[
    ParamField {
        name: "url",
        ty: ParamType::Str,
        required: true,
    },
    ParamField {
        name: "announce_tracks",
        ty: ParamType::Bool,
        required: false,
    },
    ParamField {
        name: "tracks",
        ty: ParamType::Array(&ParamType::Object),
        required: false,
    },
];

inventory::submit! {
    ElementDescriptor {
        kind: "rtsp_src",
        input_ports: &[],
        output_ports: &[PULL_OUTPUT_PORT],
        params: PULL_PARAMS,
        validate: Some(validate_rtsp_src),
        create: create_rtsp_src,
    }
}

inventory::submit! {
    ElementDescriptor {
        kind: "httpflv_src",
        input_ports: &[],
        output_ports: &[PULL_OUTPUT_PORT],
        params: PULL_PARAMS,
        validate: Some(validate_httpflv_src),
        create: create_httpflv_src,
    }
}

inventory::submit! {
    ElementDescriptor {
        kind: "rtmp_sink",
        input_ports: &[PUSH_INPUT_PORT],
        output_ports: &[],
        params: PUSH_PARAMS,
        validate: Some(validate_rtmp_sink),
        create: create_rtmp_sink,
    }
}

inventory::submit! {
    ElementDescriptor {
        kind: "webrtc_sink",
        input_ports: &[PUSH_INPUT_PORT],
        output_ports: &[],
        params: PUSH_PARAMS,
        validate: Some(validate_webrtc_sink),
        create: create_webrtc_sink,
    }
}

struct StreamPullElement {
    endpoint: PullEndpoint,
}

impl Element for StreamPullElement {
    fn run(mut self: Box<Self>, io: ElementIo) -> dg_graph::Result<()> {
        let mut tracks_by_id: HashMap<u64, TrackInfo> = HashMap::new();
        for track in &self.endpoint.tracks {
            if track.readiness != TrackReadiness::Ready {
                let _ = self.endpoint.source.close_blocking();
                return Err(dg_graph::Error::Runtime(format!(
                    "track {} is not ready ({:?})",
                    track.track_id, track.readiness
                )));
            }
            if let Err(err) = track.validate_codec_config() {
                let _ = self.endpoint.source.close_blocking();
                return Err(dg_graph::Error::Runtime(format!(
                    "track codec config invalid: {err}"
                )));
            }
            tracks_by_id.insert(track.track_id, track.clone());
        }
        let mut sequence = 0u64;
        loop {
            if io.should_stop() {
                let _ = self.endpoint.source.close_blocking();
                return Err(dg_graph::Error::NotRunning);
            }
            match self.endpoint.source.recv_blocking() {
                Ok(Some(frame)) if frame.is_end_of_stream() => break,
                Ok(Some(frame)) => {
                    let packet = media_frame_to_packet(&frame, sequence, &tracks_by_id)?;
                    sequence = sequence.saturating_add(1);
                    io.send("out", packet)?;
                }
                Ok(None) => break,
                Err(err) => {
                    let _ = self.endpoint.source.close_blocking();
                    return Err(dg_graph::Error::Runtime(format!(
                        "stream source error: {err}"
                    )));
                }
            }
        }
        let _ = self.endpoint.source.close_blocking();
        io.broadcast_eos()
    }
}

struct StreamPushElement {
    sink: Box<dyn PublisherSink>,
    tracks: Vec<TrackInfo>,
    announce_tracks: bool,
}

impl Element for StreamPushElement {
    fn run(self: Box<Self>, io: ElementIo) -> dg_graph::Result<()> {
        if self.announce_tracks && !self.tracks.is_empty() {
            for track in &self.tracks {
                if track.readiness == TrackReadiness::Ready {
                    track.validate_codec_config().map_err(|err| {
                        dg_graph::Error::Runtime(format!("track codec config invalid: {err}"))
                    })?;
                }
            }
            self.sink
                .update_tracks(self.tracks.clone())
                .map_err(|err| {
                    dg_graph::Error::Runtime(format!("track announcement failed: {err}"))
                })?;
        }
        loop {
            let packet = match io.recv("in") {
                Ok(Some(packet)) => packet,
                Ok(None) => {
                    if io.should_stop() {
                        let _ = self.sink.close();
                        return Err(dg_graph::Error::NotRunning);
                    }
                    continue;
                }
                Err(err) => {
                    let _ = self.sink.close();
                    return Err(err);
                }
            };
            if packet.is_eos() {
                self.sink.close().map_err(|err| {
                    dg_graph::Error::Runtime(format!("publisher close failed: {err}"))
                })?;
                return Ok(());
            }
            let frame = packet_to_media_frame(packet)?;
            match self.sink.push_frame(Arc::new(frame)) {
                Ok(DispatchResult::Accepted) => {}
                Ok(DispatchResult::DroppedByPolicy) => {
                    debug!(node = %io.name, "frame dropped by backpressure policy");
                    io.drop_packet()?;
                }
                Ok(DispatchResult::RejectedClosed) => {
                    return Err(dg_graph::Error::Runtime(
                        "publisher rejected frame: stream closed".to_string(),
                    ));
                }
                Err(err) => {
                    let _ = self.sink.close();
                    return Err(dg_graph::Error::Runtime(format!(
                        "stream sink error: {err}"
                    )));
                }
            }
            io.finish_packet()?;
            let keyframe_requests = self.sink.take_keyframe_requests();
            if keyframe_requests > 0 {
                debug!(node = %io.name, keyframe_requests, "keyframe requested by remote peer");
            }
        }
    }
}

fn media_frame_to_packet(
    frame: &Arc<MediaFrame>,
    sequence: u64,
    tracks_by_id: &HashMap<u64, TrackInfo>,
) -> dg_graph::Result<Packet> {
    let mut frame = match Arc::try_unwrap(Arc::clone(frame)) {
        Ok(frame) => frame,
        Err(shared) => shared.as_ref().clone(),
    };
    if frame.shape.is_empty() {
        frame.shape = vec![frame.buffer.len()];
    }
    enrich_frame_media_info_from_tracks(&mut frame, tracks_by_id)?;
    let mut frame_meta = frame.meta.clone();
    dg_media::normalize_media_frame_meta(&mut frame_meta).map_err(|err| {
        dg_graph::Error::Runtime(format!("media frame metadata normalization failed: {err}"))
    })?;
    // sequence is graph transport order; PTS lives only in media_info.timing.
    let meta = PacketMeta {
        sequence,
        stream_id: frame_meta.stream_id.clone(),
        tags: frame_meta.tags.clone(),
        media_info: frame_meta.media_info.clone(),
    };
    let tensor = frame.into_tensor()?;
    Ok(Packet::tensor(tensor).with_meta(meta))
}

fn packet_to_media_frame(packet: Packet) -> dg_graph::Result<MediaFrame> {
    let meta = packet.meta.clone();
    let tensor = packet
        .into_tensor()
        .ok_or_else(|| dg_graph::Error::Runtime("expected tensor payload".to_string()))?;
    let mut frame = MediaFrame::from_tensor(tensor);
    if meta.tags.get(MEDIA_TAG).map(String::as_str) == Some("video") {
        // Compressed video is transported as Tensor payload with encoded media_info;
        // Image kind is reserved for decoded pixel frames.
        let is_image = meta
            .media_info
            .as_ref()
            .is_some_and(|info| matches!(info.payload, MediaPayloadInfo::Image(_)));
        if is_image {
            frame.kind = MediaFrameKind::Image;
        }
    }
    let legacy_pts = meta
        .tags
        .get(PTS_TAG)
        .and_then(|value| value.parse::<i64>().ok());
    let legacy_dts = meta
        .tags
        .get(DTS_TAG)
        .and_then(|value| value.parse::<i64>().ok());
    if meta.media_info.is_none() && (legacy_pts.is_some() || legacy_dts.is_some()) {
        warn!("stream push reading pts/dts from tags is deprecated; producers must set media_info");
    }
    frame.meta.stream_id = meta.stream_id;
    frame.meta.tags = meta.tags;
    frame.meta.media_info = meta.media_info;
    if frame.meta.media_info.is_none() {
        frame.meta.pts = legacy_pts;
        frame.meta.dts = legacy_dts;
    }
    dg_media::normalize_media_frame_meta(&mut frame.meta).map_err(|err| {
        dg_graph::Error::Runtime(format!("media frame metadata normalization failed: {err}"))
    })?;
    Ok(frame)
}

/// Attaches track codec configs and validates the frame track against announced tracks.
fn enrich_frame_media_info_from_tracks(
    frame: &mut MediaFrame,
    tracks_by_id: &HashMap<u64, TrackInfo>,
) -> dg_graph::Result<()> {
    if tracks_by_id.is_empty() {
        return Ok(());
    }
    let track_id = match resolve_frame_track_id(frame) {
        Ok(id) => id,
        Err(_) if tracks_by_id.len() == 1 => {
            // Single-track streams may omit per-frame track identity; bind to the only track.
            *tracks_by_id.keys().next().expect("single track present")
        }
        Err(err) => return Err(err),
    };
    let track = tracks_by_id.get(&track_id).ok_or_else(|| {
        dg_graph::Error::Runtime(format!(
            "frame track_id {track_id} is not in announced TrackInfo set"
        ))
    })?;
    if track.clock_rate == 0 {
        return Err(dg_graph::Error::Runtime(format!(
            "announced track {track_id} has invalid zero clock_rate"
        )));
    }

    let configs = track_codec_configs(track).map_err(|err| {
        dg_graph::Error::Runtime(format!("track {track_id} codec config build failed: {err}"))
    })?;
    let timing = MediaTiming {
        pts: frame.meta.pts.or_else(|| {
            frame
                .meta
                .media_info
                .as_ref()
                .and_then(|info| info.timing.pts)
        }),
        dts: frame.meta.dts.or_else(|| {
            frame
                .meta
                .media_info
                .as_ref()
                .and_then(|info| info.timing.dts)
        }),
        time_base: Some(MediaTimeBase::new(1, track.clock_rate)),
    };
    let key = frame
        .meta
        .media_info
        .as_ref()
        .map(|info| info.is_keyframe())
        .or_else(|| frame.meta.stream_metadata.map(|legacy| legacy.keyframe))
        .unwrap_or_else(|| {
            frame
                .meta
                .tags
                .get(KEYFRAME_TAG)
                .is_some_and(|value| value == "true")
        });

    let encoded = EncodedMediaInfo {
        stream_index: u32::try_from(track_id).unwrap_or(u32::MAX),
        track_id: Some(track_id),
        media_kind: track_media_kind(track),
        codec: track_media_codec(track.codec),
        bitstream_format: track_bitstream_format(track.codec),
        flags: EncodedPacketFlags {
            key,
            lost: false,
            corrupt: false,
        },
        codec_configs: configs,
    };

    if let Some(existing) = frame.meta.media_info.as_ref() {
        if let MediaPayloadInfo::Encoded(existing_enc) = &existing.payload {
            if existing_enc.codec != MediaCodec::Unknown && existing_enc.codec != encoded.codec {
                return Err(dg_graph::Error::Runtime(format!(
                    "frame media_info codec {:?} conflicts with track codec {:?}",
                    existing_enc.codec, encoded.codec
                )));
            }
            if let Some(existing_track) = existing_enc.track_id {
                if existing_track != track_id {
                    return Err(dg_graph::Error::Runtime(format!(
                        "frame media_info track_id {existing_track} conflicts with resolved \
                         track_id {track_id}"
                    )));
                }
            }
        }
    }

    let info = MediaInfo::encoded(encoded, timing).map_err(|err| {
        dg_graph::Error::Runtime(format!("invalid media_info constructed from track: {err}"))
    })?;
    frame.meta.media_info = Some(Box::new(info));
    frame.meta.pts = timing.pts;
    frame.meta.dts = timing.dts;
    Ok(())
}

fn resolve_frame_track_id(frame: &MediaFrame) -> dg_graph::Result<u64> {
    if let Some(info) = frame.meta.media_info.as_ref() {
        if let MediaPayloadInfo::Encoded(encoded) = &info.payload {
            if let Some(track_id) = encoded.track_id {
                return Ok(track_id);
            }
        }
    }
    if let Some(legacy) = frame.meta.stream_metadata {
        return Ok(legacy.track_id);
    }
    if let Some(stream_id) = frame.meta.stream_id.as_deref() {
        return stream_id.parse::<u64>().map_err(|_| {
            dg_graph::Error::Runtime(format!(
                "stream frame stream_id `{stream_id}` is not a valid track id"
            ))
        });
    }
    Err(dg_graph::Error::Runtime(
        "stream frame has no track_id in media_info, stream_metadata, or stream_id".into(),
    ))
}

fn track_media_kind(track: &TrackInfo) -> CoreMediaKind {
    match track.media_kind {
        crate::track::MediaKind::Video => CoreMediaKind::Video,
        crate::track::MediaKind::Audio => CoreMediaKind::Audio,
        crate::track::MediaKind::Data => CoreMediaKind::Data,
        crate::track::MediaKind::Subtitle => CoreMediaKind::Subtitle,
    }
}

fn track_media_codec(codec: TrackCodec) -> MediaCodec {
    match codec {
        TrackCodec::H264 => MediaCodec::H264,
        TrackCodec::H265 => MediaCodec::H265,
        TrackCodec::H266 => MediaCodec::H266,
        TrackCodec::AV1 => MediaCodec::AV1,
        TrackCodec::VP8 => MediaCodec::VP8,
        TrackCodec::VP9 => MediaCodec::VP9,
        TrackCodec::MJPEG => MediaCodec::MJPEG,
        TrackCodec::AAC => MediaCodec::AAC,
        TrackCodec::ADPCM => MediaCodec::ADPCM,
        TrackCodec::Opus => MediaCodec::Opus,
        TrackCodec::G711A => MediaCodec::G711A,
        TrackCodec::G711U => MediaCodec::G711U,
        TrackCodec::MP2 => MediaCodec::MP2,
        TrackCodec::MP3 => MediaCodec::MP3,
        TrackCodec::Unknown => MediaCodec::Unknown,
    }
}

fn track_bitstream_format(codec: TrackCodec) -> BitstreamFormat {
    match codec {
        TrackCodec::H264 => BitstreamFormat::H264AnnexB,
        TrackCodec::H265 | TrackCodec::H266 => BitstreamFormat::H265AnnexB,
        TrackCodec::AV1 => BitstreamFormat::Av1Obu,
        TrackCodec::VP8 => BitstreamFormat::Vp8Frame,
        TrackCodec::VP9 => BitstreamFormat::Vp9Frame,
        TrackCodec::MJPEG => BitstreamFormat::JpegInterchange,
        TrackCodec::AAC => BitstreamFormat::AacRaw,
        _ => BitstreamFormat::Unknown,
    }
}

/// Builds Annex-B / OBU / ASC codec_config blobs from track extradata with size limits.
fn track_codec_configs(track: &TrackInfo) -> dg_core::Result<Vec<MediaCodecConfig>> {
    const START: &[u8] = &[0, 0, 0, 1];
    let mut configs = Vec::new();
    match &track.extradata {
        CodecExtradata::None | CodecExtradata::Raw(_) => {}
        CodecExtradata::H264 { sps, pps, avcc } => {
            if !sps.is_empty() || !pps.is_empty() {
                let mut annexb = Vec::new();
                for nal in sps.iter().chain(pps.iter()) {
                    annexb_append_nal(&mut annexb, START, nal.as_ref())?;
                }
                if !annexb.is_empty() {
                    configs.push(MediaCodecConfig::new(BitstreamFormat::H264AnnexB, annexb)?);
                }
            }
            if let Some(avcc) = avcc {
                configs.push(MediaCodecConfig::new(
                    BitstreamFormat::H264Avcc,
                    avcc.to_vec(),
                )?);
            }
        }
        CodecExtradata::H265 {
            vps,
            sps,
            pps,
            hvcc,
        } => {
            if !vps.is_empty() || !sps.is_empty() || !pps.is_empty() {
                let mut annexb = Vec::new();
                for nal in vps.iter().chain(sps.iter()).chain(pps.iter()) {
                    annexb_append_nal(&mut annexb, START, nal.as_ref())?;
                }
                if !annexb.is_empty() {
                    configs.push(MediaCodecConfig::new(BitstreamFormat::H265AnnexB, annexb)?);
                }
            }
            if let Some(hvcc) = hvcc {
                configs.push(MediaCodecConfig::new(
                    BitstreamFormat::H265Hvcc,
                    hvcc.to_vec(),
                )?);
            }
        }
        CodecExtradata::H266 { vps, sps, pps } => {
            let mut annexb = Vec::new();
            for nal in vps.iter().chain(sps.iter()).chain(pps.iter()) {
                annexb_append_nal(&mut annexb, START, nal.as_ref())?;
            }
            if !annexb.is_empty() {
                // H266 uses the same Annex-B packaging as H265 for transport.
                configs.push(MediaCodecConfig::new(BitstreamFormat::H265AnnexB, annexb)?);
            }
        }
        CodecExtradata::AAC { asc } => {
            if !asc.is_empty() {
                configs.push(MediaCodecConfig::new(
                    BitstreamFormat::AacRaw,
                    asc.to_vec(),
                )?);
            }
        }
        CodecExtradata::AV1 {
            sequence_header,
            codec_config,
        } => {
            if let Some(seq) = sequence_header {
                configs.push(MediaCodecConfig::new(
                    BitstreamFormat::Av1Obu,
                    seq.to_vec(),
                )?);
            }
            if let Some(cfg) = codec_config {
                configs.push(MediaCodecConfig::new(
                    BitstreamFormat::Av1Obu,
                    cfg.to_vec(),
                )?);
            }
        }
        CodecExtradata::VP8 { config } => {
            if let Some(cfg) = config {
                configs.push(MediaCodecConfig::new(
                    BitstreamFormat::Vp8Frame,
                    cfg.to_vec(),
                )?);
            }
        }
        CodecExtradata::VP9 { config } => {
            if let Some(cfg) = config {
                configs.push(MediaCodecConfig::new(
                    BitstreamFormat::Vp9Frame,
                    cfg.to_vec(),
                )?);
            }
        }
        CodecExtradata::MP3 { .. } | CodecExtradata::Opus { .. } => {}
    }
    Ok(configs)
}

fn annexb_append_nal(out: &mut Vec<u8>, start: &[u8], nal: &[u8]) -> dg_core::Result<()> {
    let added = start
        .len()
        .checked_add(nal.len())
        .ok_or_else(|| dg_core::Error::InvalidArgument("annex-b nal length overflow".into()))?;
    let new_len = out
        .len()
        .checked_add(added)
        .ok_or_else(|| dg_core::Error::InvalidArgument("annex-b join length overflow".into()))?;
    if new_len > dg_core::MAX_CODEC_CONFIG_TOTAL_BYTES {
        return Err(dg_core::Error::InvalidArgument(
            "annex-b parameter sets exceed total codec config budget".into(),
        ));
    }
    out.extend_from_slice(start);
    out.extend_from_slice(nal);
    Ok(())
}

fn create_rtsp_src(node: &NodeSpec) -> dg_graph::Result<CreatedElement> {
    create_pull(node, StreamProtocol::RtspPull)
}

fn create_httpflv_src(node: &NodeSpec) -> dg_graph::Result<CreatedElement> {
    create_pull(node, StreamProtocol::HttpFlvPull)
}

fn create_rtmp_sink(node: &NodeSpec) -> dg_graph::Result<CreatedElement> {
    create_push(node, StreamProtocol::RtmpPush)
}

fn create_webrtc_sink(node: &NodeSpec) -> dg_graph::Result<CreatedElement> {
    create_push(node, StreamProtocol::WebRtcPush)
}

fn validate_rtsp_src(node: &NodeSpec) -> dg_graph::Result<()> {
    parse_pull(node, StreamProtocol::RtspPull).map(|_| ())
}

fn validate_httpflv_src(node: &NodeSpec) -> dg_graph::Result<()> {
    parse_pull(node, StreamProtocol::HttpFlvPull).map(|_| ())
}

fn validate_rtmp_sink(node: &NodeSpec) -> dg_graph::Result<()> {
    parse_push(node, StreamProtocol::RtmpPush).map(|_| ())
}

fn validate_webrtc_sink(node: &NodeSpec) -> dg_graph::Result<()> {
    parse_push(node, StreamProtocol::WebRtcPush).map(|_| ())
}

struct PullConfig {
    url: String,
    options: SubscriberOptions,
}

fn create_pull(node: &NodeSpec, protocol: StreamProtocol) -> dg_graph::Result<CreatedElement> {
    let config = parse_pull(node, protocol)?;
    let endpoint = open_pull(protocol, &config.url, config.options).map_err(create_error)?;
    Ok(CreatedElement {
        element: Box::new(StreamPullElement { endpoint }),
        handle: ElementHandle::None,
    })
}

fn parse_pull(node: &NodeSpec, protocol: StreamProtocol) -> dg_graph::Result<PullConfig> {
    let params = params_object(node)?;
    reject_unknown_fields(params, PULL_PARAM_FIELDS)?;
    let url = read_url(params, node)?;
    validate_endpoint_url(protocol, &url).map_err(create_error)?;
    let queue_capacity = read_usize(params, "queue_capacity", 150)?;
    if queue_capacity == 0 {
        return Err(dg_graph::Error::Config(
            "field queue_capacity must be non-zero".to_string(),
        ));
    }
    let enable_video = read_bool(params, "enable_video", true)?;
    let enable_audio = read_bool(params, "enable_audio", true)?;
    if !enable_video && !enable_audio {
        return Err(dg_graph::Error::Config(
            "at least one of enable_video or enable_audio must be true".to_string(),
        ));
    }
    let options = SubscriberOptions {
        queue_capacity,
        backpressure: read_backpressure(params)?,
        media_filter: MediaFilter {
            enable_video,
            enable_audio,
        },
        ..SubscriberOptions::default()
    };
    Ok(PullConfig { url, options })
}

struct PushConfig {
    url: String,
    announce_tracks: bool,
    tracks: Vec<TrackInfo>,
}

fn create_push(node: &NodeSpec, protocol: StreamProtocol) -> dg_graph::Result<CreatedElement> {
    let config = parse_push(node, protocol)?;
    let options = PublisherOptions {
        announce_tracks: config.announce_tracks,
    };
    let sink = open_push(protocol, &config.url, options).map_err(create_error)?;
    Ok(CreatedElement {
        element: Box::new(StreamPushElement {
            sink,
            tracks: config.tracks,
            announce_tracks: config.announce_tracks,
        }),
        handle: ElementHandle::None,
    })
}

fn parse_push(node: &NodeSpec, protocol: StreamProtocol) -> dg_graph::Result<PushConfig> {
    let params = params_object(node)?;
    reject_unknown_fields(params, PUSH_PARAM_FIELDS)?;
    let url = read_url(params, node)?;
    validate_endpoint_url(protocol, &url).map_err(create_error)?;
    let announce_tracks = read_bool(params, "announce_tracks", true)?;
    let tracks = read_tracks(params)?;
    validate_tracks(&tracks)?;
    Ok(PushConfig {
        url,
        announce_tracks,
        tracks,
    })
}

fn create_error(err: crate::error::Error) -> dg_graph::Error {
    match err {
        crate::error::Error::InvalidArgument(message) => dg_graph::Error::Config(message),
        other => dg_graph::Error::Runtime(other.to_string()),
    }
}

fn params_object(node: &NodeSpec) -> dg_graph::Result<&Map<String, Value>> {
    node.params.as_object().ok_or_else(|| {
        dg_graph::Error::Config(format!("node {} params must be an object", node.name))
    })
}

fn reject_unknown_fields(params: &Map<String, Value>, allowed: &[&str]) -> dg_graph::Result<()> {
    for key in params.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(dg_graph::Error::Config(format!(
                "unknown field `{key}`; expected one of {}",
                allowed.join(", ")
            )));
        }
    }
    Ok(())
}

fn read_url(params: &Map<String, Value>, node: &NodeSpec) -> dg_graph::Result<String> {
    params
        .get("url")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            dg_graph::Error::Config(format!(
                "node {} requires a string `url` parameter",
                node.name
            ))
        })
}

fn read_usize(params: &Map<String, Value>, key: &str, default: usize) -> dg_graph::Result<usize> {
    match params.get(key) {
        Some(value) => value
            .as_u64()
            .ok_or_else(|| {
                dg_graph::Error::Config(format!("field {key} must be a non-negative integer"))
            })
            .and_then(|value| {
                usize::try_from(value)
                    .map_err(|_| dg_graph::Error::Config(format!("field {key} overflow")))
            }),
        None => Ok(default),
    }
}

fn read_bool(params: &Map<String, Value>, key: &str, default: bool) -> dg_graph::Result<bool> {
    match params.get(key) {
        Some(value) => value
            .as_bool()
            .ok_or_else(|| dg_graph::Error::Config(format!("field {key} must be a boolean"))),
        None => Ok(default),
    }
}

fn read_backpressure(params: &Map<String, Value>) -> dg_graph::Result<BackpressurePolicy> {
    match params.get("backpressure") {
        None => Ok(BackpressurePolicy::DropDroppableFirst),
        Some(value) => match value.as_str() {
            Some("drop_droppable_first") => Ok(BackpressurePolicy::DropDroppableFirst),
            Some("drop_until_next_keyframe") => Ok(BackpressurePolicy::DropUntilNextKeyframe),
            Some("disconnect_on_overflow") => Ok(BackpressurePolicy::DisconnectOnOverflow),
            _ => Err(dg_graph::Error::Config(
                "field backpressure must be one of drop_droppable_first, \
                 drop_until_next_keyframe, disconnect_on_overflow"
                    .to_string(),
            )),
        },
    }
}

fn read_tracks(params: &Map<String, Value>) -> dg_graph::Result<Vec<TrackInfo>> {
    match params.get("tracks") {
        None => Ok(Vec::new()),
        Some(value) => {
            let entries = value.as_array().ok_or_else(|| {
                dg_graph::Error::Config("field tracks must be an array".to_string())
            })?;
            for (index, entry) in entries.iter().enumerate() {
                let object = entry.as_object().ok_or_else(|| {
                    dg_graph::Error::Config(format!("field tracks[{index}] must be an object"))
                })?;
                reject_unknown_fields(object, TRACK_FIELDS).map_err(|err| {
                    dg_graph::Error::Config(format!("field tracks[{index}] is invalid: {err}"))
                })?;
            }
            serde_json::from_value(value.clone())
                .map_err(|err| dg_graph::Error::Config(format!("field tracks is invalid: {err}")))
        }
    }
}

fn validate_tracks(tracks: &[TrackInfo]) -> dg_graph::Result<()> {
    let mut ids = BTreeSet::new();
    for (index, track) in tracks.iter().enumerate() {
        if !ids.insert(track.track_id) {
            return Err(dg_graph::Error::Config(format!(
                "field tracks[{index}].track_id duplicates {}",
                track.track_id
            )));
        }
        if track.clock_rate == 0 {
            return Err(dg_graph::Error::Config(format!(
                "field tracks[{index}].clock_rate must be non-zero"
            )));
        }
        for (field, value) in [
            ("sample_rate", track.sample_rate),
            ("width", track.width),
            ("height", track.height),
        ] {
            if value == Some(0) {
                return Err(dg_graph::Error::Config(format!(
                    "field tracks[{index}].{field} must be non-zero"
                )));
            }
        }
        if track.channels == Some(0) {
            return Err(dg_graph::Error::Config(format!(
                "field tracks[{index}].channels must be non-zero"
            )));
        }
        if let Some(fps) = track.fps {
            if fps.num == 0 || fps.den == 0 {
                return Err(dg_graph::Error::Config(format!(
                    "field tracks[{index}].fps numerator and denominator must be non-zero"
                )));
            }
        }
        if track.readiness == TrackReadiness::Ready {
            track.validate_codec_config().map_err(|err| {
                dg_graph::Error::Config(format!("field tracks[{index}] is invalid: {err}"))
            })?;
        }
    }
    Ok(())
}
