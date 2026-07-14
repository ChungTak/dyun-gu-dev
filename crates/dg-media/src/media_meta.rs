//! Compatibility and normalization helpers for [`MediaFrameMeta`].
#![forbid(unsafe_code)]

use dg_core::{
    BitstreamFormat, EncodedMediaInfo, EncodedPacketFlags, Error, MediaCodec, MediaInfo, MediaKind,
    MediaPayloadInfo, MediaTimeBase, MediaTiming, Result,
};

use crate::stream_metadata::{
    MediaStreamCodec, MediaStreamFormat, MediaStreamKind, MediaStreamMetadata, MediaStreamTimebase,
};
use crate::MediaFrameMeta;

fn stream_kind_to_media(kind: MediaStreamKind) -> MediaKind {
    match kind {
        MediaStreamKind::Video => MediaKind::Video,
        MediaStreamKind::Audio => MediaKind::Audio,
        MediaStreamKind::Data => MediaKind::Data,
        MediaStreamKind::Subtitle => MediaKind::Subtitle,
    }
}

fn stream_codec_to_media(codec: MediaStreamCodec) -> MediaCodec {
    match codec {
        MediaStreamCodec::H264 => MediaCodec::H264,
        MediaStreamCodec::H265 => MediaCodec::H265,
        MediaStreamCodec::H266 => MediaCodec::H266,
        MediaStreamCodec::AV1 => MediaCodec::AV1,
        MediaStreamCodec::VP8 => MediaCodec::VP8,
        MediaStreamCodec::VP9 => MediaCodec::VP9,
        MediaStreamCodec::MJPEG => MediaCodec::MJPEG,
        MediaStreamCodec::AAC => MediaCodec::AAC,
        MediaStreamCodec::ADPCM => MediaCodec::ADPCM,
        MediaStreamCodec::Opus => MediaCodec::Opus,
        MediaStreamCodec::G711A => MediaCodec::G711A,
        MediaStreamCodec::G711U => MediaCodec::G711U,
        MediaStreamCodec::MP2 => MediaCodec::MP2,
        MediaStreamCodec::MP3 => MediaCodec::MP3,
        MediaStreamCodec::Unknown => MediaCodec::Unknown,
    }
}

fn stream_format_to_bitstream(format: MediaStreamFormat) -> BitstreamFormat {
    match format {
        MediaStreamFormat::CanonicalH26x => BitstreamFormat::H264AnnexB,
        MediaStreamFormat::CanonicalAv1Obu => BitstreamFormat::Av1Obu,
        MediaStreamFormat::CanonicalVp8Frame => BitstreamFormat::Vp8Frame,
        MediaStreamFormat::CanonicalVp9Frame => BitstreamFormat::Vp9Frame,
        MediaStreamFormat::MjpegFrame => BitstreamFormat::JpegInterchange,
        MediaStreamFormat::AacRaw => BitstreamFormat::AacRaw,
        MediaStreamFormat::AdpcmPacket
        | MediaStreamFormat::OpusPacket
        | MediaStreamFormat::G711Packet
        | MediaStreamFormat::Mp2Frame
        | MediaStreamFormat::Mp3Frame
        | MediaStreamFormat::DataPacket
        | MediaStreamFormat::Unknown => BitstreamFormat::Unknown,
    }
}

fn media_info_from_stream_metadata(
    legacy: &MediaStreamMetadata,
    timing: MediaTiming,
) -> Result<MediaInfo> {
    let encoded = EncodedMediaInfo {
        stream_index: 0,
        track_id: Some(legacy.track_id),
        media_kind: stream_kind_to_media(legacy.media_kind),
        codec: stream_codec_to_media(legacy.codec),
        bitstream_format: stream_format_to_bitstream(legacy.format),
        flags: EncodedPacketFlags {
            key: legacy.keyframe,
            lost: false,
            corrupt: false,
        },
        codec_configs: Vec::new(),
    };
    MediaInfo::encoded(encoded, timing)
}

fn legacy_from_media_info(info: &MediaInfo) -> Option<MediaStreamMetadata> {
    let MediaPayloadInfo::Encoded(encoded) = &info.payload else {
        return None;
    };
    let codec = match encoded.codec {
        MediaCodec::H264 => MediaStreamCodec::H264,
        MediaCodec::H265 => MediaStreamCodec::H265,
        MediaCodec::H266 => MediaStreamCodec::H266,
        MediaCodec::AV1 => MediaStreamCodec::AV1,
        MediaCodec::VP8 => MediaStreamCodec::VP8,
        MediaCodec::VP9 => MediaStreamCodec::VP9,
        MediaCodec::MJPEG => MediaStreamCodec::MJPEG,
        MediaCodec::AAC => MediaStreamCodec::AAC,
        MediaCodec::ADPCM => MediaStreamCodec::ADPCM,
        MediaCodec::Opus => MediaStreamCodec::Opus,
        MediaCodec::G711A => MediaStreamCodec::G711A,
        MediaCodec::G711U => MediaStreamCodec::G711U,
        MediaCodec::MP2 => MediaStreamCodec::MP2,
        MediaCodec::MP3 => MediaStreamCodec::MP3,
        MediaCodec::Jpeg | MediaCodec::Unknown => MediaStreamCodec::Unknown,
    };
    let format = match encoded.bitstream_format {
        BitstreamFormat::H264AnnexB | BitstreamFormat::H265AnnexB => {
            MediaStreamFormat::CanonicalH26x
        }
        BitstreamFormat::Av1Obu => MediaStreamFormat::CanonicalAv1Obu,
        BitstreamFormat::Vp8Frame => MediaStreamFormat::CanonicalVp8Frame,
        BitstreamFormat::Vp9Frame => MediaStreamFormat::CanonicalVp9Frame,
        BitstreamFormat::JpegInterchange => MediaStreamFormat::MjpegFrame,
        BitstreamFormat::AacRaw | BitstreamFormat::AacAdts => MediaStreamFormat::AacRaw,
        _ => MediaStreamFormat::Unknown,
    };
    let timebase = info.timing.time_base.unwrap_or(MediaTimeBase::new(1, 1));
    Some(MediaStreamMetadata {
        track_id: encoded.track_id.unwrap_or(0),
        media_kind: match encoded.media_kind {
            MediaKind::Video => MediaStreamKind::Video,
            MediaKind::Audio => MediaStreamKind::Audio,
            MediaKind::Data => MediaStreamKind::Data,
            MediaKind::Subtitle => MediaStreamKind::Subtitle,
            MediaKind::Unknown => MediaStreamKind::Video,
        },
        codec,
        format,
        timebase: MediaStreamTimebase::new(timebase.num, timebase.den),
        keyframe: encoded.flags.key,
    })
}

fn legacy_conflicts_with_media_info(legacy: &MediaStreamMetadata, info: &MediaInfo) -> Result<()> {
    let MediaPayloadInfo::Encoded(encoded) = &info.payload else {
        return Ok(());
    };
    if let Some(track_id) = encoded.track_id {
        if track_id != legacy.track_id {
            return Err(Error::Media(format!(
                "stream_metadata track_id {} conflicts with media_info track_id {track_id}",
                legacy.track_id
            )));
        }
    }
    if stream_codec_to_media(legacy.codec) != encoded.codec
        && legacy.codec != MediaStreamCodec::Unknown
    {
        return Err(Error::Media(format!(
            "stream_metadata codec {:?} conflicts with media_info codec {:?}",
            legacy.codec, encoded.codec
        )));
    }
    if legacy.keyframe != encoded.flags.key {
        return Err(Error::Media(
            "stream_metadata keyframe conflicts with media_info flags.key".into(),
        ));
    }
    if let Some(tb) = info.timing.time_base {
        if legacy.timebase.num != tb.num || legacy.timebase.den != tb.den {
            return Err(Error::Media(format!(
                "stream_metadata timebase {}/{} conflicts with media_info {}/{}",
                legacy.timebase.num, legacy.timebase.den, tb.num, tb.den
            )));
        }
    }
    Ok(())
}

/// Normalizes timing fields and reconciles legacy `stream_metadata` with `media_info`.
pub fn normalize_media_frame_meta(meta: &mut MediaFrameMeta) -> Result<()> {
    let timing_from_top = MediaTiming {
        pts: meta.pts,
        dts: meta.dts,
        time_base: meta
            .stream_metadata
            .map(|legacy| MediaTimeBase::new(legacy.timebase.num, legacy.timebase.den)),
    };

    match (&meta.media_info, &meta.stream_metadata) {
        (None, Some(legacy)) => {
            meta.media_info = Some(Box::new(media_info_from_stream_metadata(
                legacy,
                timing_from_top,
            )?));
        }
        (Some(info), None) => {
            meta.stream_metadata = legacy_from_media_info(info.as_ref());
        }
        (Some(info), Some(legacy)) => {
            legacy_conflicts_with_media_info(legacy, info.as_ref())?;
        }
        (None, None) if meta.pts.is_some() || meta.dts.is_some() => {
            meta.media_info = Some(Box::new(MediaInfo {
                timing: timing_from_top,
                payload: MediaPayloadInfo::Encoded(EncodedMediaInfo {
                    stream_index: 0,
                    track_id: None,
                    media_kind: MediaKind::Unknown,
                    codec: MediaCodec::Unknown,
                    bitstream_format: BitstreamFormat::Unknown,
                    flags: EncodedPacketFlags::default(),
                    codec_configs: Vec::new(),
                }),
            }));
        }
        (None, None) => {}
    }

    if let Some(info) = meta.media_info.as_deref() {
        meta.pts = info.timing.pts;
        meta.dts = info.timing.dts;
    }
    Ok(())
}

/// Extracts authoritative timing from frame metadata.
pub fn media_frame_timing(meta: &MediaFrameMeta) -> MediaTiming {
    if let Some(info) = meta.media_info.as_deref() {
        info.timing
    } else {
        MediaTiming {
            pts: meta.pts,
            dts: meta.dts,
            time_base: meta
                .stream_metadata
                .map(|legacy| MediaTimeBase::new(legacy.timebase.num, legacy.timebase.den)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_metadata_generates_media_info() {
        let mut meta = MediaFrameMeta {
            pts: Some(90_000),
            dts: Some(-1),
            stream_id: Some("7".into()),
            tags: Default::default(),
            media_info: None,
            stream_metadata: Some(MediaStreamMetadata {
                track_id: 7,
                media_kind: MediaStreamKind::Video,
                codec: MediaStreamCodec::H264,
                format: MediaStreamFormat::CanonicalH26x,
                timebase: MediaStreamTimebase::new(1, 90_000),
                keyframe: true,
            }),
        };
        normalize_media_frame_meta(&mut meta).expect("normalize");
        let info = meta.media_info.as_deref().expect("generated");
        assert_eq!(info.timing.pts, Some(90_000));
        assert!(info.is_keyframe());
    }

    #[test]
    fn conflicting_legacy_and_media_info_fails() {
        let timing = MediaTiming {
            pts: Some(1),
            dts: None,
            time_base: Some(MediaTimeBase::new(1, 25)),
        };
        let media_info = MediaInfo::encoded(
            EncodedMediaInfo {
                stream_index: 0,
                track_id: Some(9),
                media_kind: MediaKind::Video,
                codec: MediaCodec::H264,
                bitstream_format: BitstreamFormat::H264AnnexB,
                flags: EncodedPacketFlags::default(),
                codec_configs: Vec::new(),
            },
            timing,
        )
        .expect("info");
        let mut meta = MediaFrameMeta {
            pts: Some(1),
            dts: None,
            stream_id: None,
            tags: Default::default(),
            media_info: Some(Box::new(media_info)),
            stream_metadata: Some(MediaStreamMetadata {
                track_id: 7,
                media_kind: MediaStreamKind::Video,
                codec: MediaStreamCodec::H264,
                format: MediaStreamFormat::CanonicalH26x,
                timebase: MediaStreamTimebase::new(1, 90_000),
                keyframe: false,
            }),
        };
        assert!(normalize_media_frame_meta(&mut meta).is_err());
    }
}
