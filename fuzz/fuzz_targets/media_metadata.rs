#![no_main]

use dg_core::{
    BitstreamFormat, ColorMatrix, ColorPrimaries, ColorRange, ColorTransfer, EncodedMediaInfo,
    EncodedPacketFlags, ImageFlags, ImageMediaInfo, MediaCodec, MediaCodecConfig, MediaInfo,
    MediaKind, MediaPlaneLayout, MediaRect, MediaTimeBase, MediaTiming, PixelFormat, SampleLayout,
    SampleType,
};
use dg_media::{media_frame_timing, normalize_media_frame_meta, MediaFrameMeta};
use libfuzzer_sys::fuzz_target;

fn take_u64(bytes: &mut &[u8]) -> Option<u64> {
    if bytes.len() < 8 {
        return None;
    }
    let (head, tail) = bytes.split_at(8);
    *bytes = tail;
    Some(u64::from_le_bytes(head.try_into().unwrap()))
}

fn take_u32(bytes: &mut &[u8]) -> Option<u32> {
    if bytes.len() < 4 {
        return None;
    }
    let (head, tail) = bytes.split_at(4);
    *bytes = tail;
    Some(u32::from_le_bytes(head.try_into().unwrap()))
}

fn take_u8(bytes: &mut &[u8]) -> Option<u8> {
    if bytes.is_empty() {
        return None;
    }
    let (head, tail) = bytes.split_at(1);
    *bytes = tail;
    Some(head[0])
}

fn media_kind(kind: u8) -> MediaKind {
    match kind % 5 {
        0 => MediaKind::Video,
        1 => MediaKind::Audio,
        2 => MediaKind::Data,
        3 => MediaKind::Subtitle,
        _ => MediaKind::Unknown,
    }
}

fn media_codec(codec: u8) -> MediaCodec {
    match codec % 16 {
        0 => MediaCodec::H264,
        1 => MediaCodec::H265,
        2 => MediaCodec::H266,
        3 => MediaCodec::AV1,
        4 => MediaCodec::VP8,
        5 => MediaCodec::VP9,
        6 => MediaCodec::MJPEG,
        7 => MediaCodec::AAC,
        8 => MediaCodec::ADPCM,
        9 => MediaCodec::Opus,
        10 => MediaCodec::G711A,
        11 => MediaCodec::G711U,
        12 => MediaCodec::MP2,
        13 => MediaCodec::MP3,
        14 => MediaCodec::Jpeg,
        _ => MediaCodec::Unknown,
    }
}

fn bitstream_format(fmt: u8) -> BitstreamFormat {
    match fmt % 10 {
        0 => BitstreamFormat::H264AnnexB,
        1 => BitstreamFormat::H264Avcc,
        2 => BitstreamFormat::H265AnnexB,
        3 => BitstreamFormat::H265Hvcc,
        4 => BitstreamFormat::Vp8Frame,
        5 => BitstreamFormat::Vp9Frame,
        6 => BitstreamFormat::Av1Obu,
        7 => BitstreamFormat::JpegInterchange,
        8 => BitstreamFormat::AacRaw,
        _ => BitstreamFormat::AacAdts,
    }
}

fn pixel_format(fmt: u8) -> PixelFormat {
    match fmt % 11 {
        0 => PixelFormat::Yuv420P,
        1 => PixelFormat::Yuv422P,
        2 => PixelFormat::Yuv444P,
        3 => PixelFormat::Nv12,
        4 => PixelFormat::Nv21,
        5 => PixelFormat::Rgb24,
        6 => PixelFormat::Bgr24,
        7 => PixelFormat::Rgba,
        8 => PixelFormat::Bgra,
        9 => PixelFormat::Gray8,
        _ => PixelFormat::Unknown,
    }
}

fuzz_target!(|data: &[u8]| {
    let mut input = data;

    // EncodedMediaInfo fuzzing.
    let stream_index = take_u32(&mut input).unwrap_or(0);
    let track_id = take_u64(&mut input);
    let kind = take_u8(&mut input).unwrap_or(0);
    let codec = take_u8(&mut input).unwrap_or(0);
    let fmt = take_u8(&mut input).unwrap_or(0);
    let flags = take_u8(&mut input).unwrap_or(0);
    let config_count = (take_u8(&mut input).unwrap_or(0) % 10) as usize;

    let mut codec_configs = Vec::new();
    for _ in 0..config_count {
        if input.is_empty() {
            break;
        }
        let len = (input[0] as usize).min(input.len().saturating_sub(1));
        let (_, rest) = input.split_at(1);
        input = rest;
        if input.len() < len {
            break;
        }
        let (config_data, rest) = input.split_at(len);
        let config_data = config_data.to_vec();
        input = rest;
        let config_format = bitstream_format(take_u8(&mut input).unwrap_or(0));
        if let Ok(config) = MediaCodecConfig::new(config_format, config_data) {
            codec_configs.push(config);
        }
    }

    let encoded = EncodedMediaInfo {
        stream_index,
        track_id,
        media_kind: media_kind(kind),
        codec: media_codec(codec),
        bitstream_format: bitstream_format(fmt),
        flags: EncodedPacketFlags {
            key: flags & 1 != 0,
            lost: flags & 2 != 0,
            corrupt: flags & 4 != 0,
        },
        codec_configs,
    };

    let num = take_u32(&mut input).unwrap_or(1);
    let den = take_u32(&mut input).unwrap_or(90_000);
    let timing = MediaTiming {
        pts: take_u64(&mut input).map(|v| v as i64),
        dts: take_u64(&mut input).map(|v| v as i64),
        time_base: Some(MediaTimeBase::new(num, den)),
    };
    let _ = MediaInfo::encoded(encoded, timing);

    // ImageMediaInfo fuzzing.
    let pixel_format = pixel_format(take_u8(&mut input).unwrap_or(0));
    let coded_width = take_u32(&mut input).unwrap_or(1);
    let coded_height = take_u32(&mut input).unwrap_or(1);
    let visible_rect = MediaRect {
        x: take_u32(&mut input).unwrap_or(0),
        y: take_u32(&mut input).unwrap_or(0),
        width: take_u32(&mut input).unwrap_or(coded_width),
        height: take_u32(&mut input).unwrap_or(coded_height),
    };
    let crop_rect = if take_u8(&mut input).unwrap_or(0) % 2 == 1 {
        Some(MediaRect {
            x: take_u32(&mut input).unwrap_or(0),
            y: take_u32(&mut input).unwrap_or(0),
            width: take_u32(&mut input).unwrap_or(coded_width),
            height: take_u32(&mut input).unwrap_or(coded_height),
        })
    } else {
        None
    };

    let plane_count = (take_u8(&mut input).unwrap_or(0) % 6) as usize;
    let mut planes = Vec::with_capacity(plane_count);
    for _ in 0..plane_count {
        if let (Some(offset), Some(stride), Some(len)) =
            (take_u64(&mut input), take_u64(&mut input), take_u64(&mut input))
        {
            planes.push(MediaPlaneLayout {
                offset: offset as usize,
                stride: stride as usize,
                len: len as usize,
            });
        }
    }

    let image = ImageMediaInfo {
        pixel_format,
        coded_width,
        coded_height,
        visible_rect,
        crop_rect,
        color_primaries: ColorPrimaries::Unknown,
        color_transfer: ColorTransfer::Unknown,
        color_matrix: ColorMatrix::Unknown,
        color_range: ColorRange::Unknown,
        flags: ImageFlags { key: flags & 1 != 0 },
        sample_type: SampleType::Uint8,
        sample_layout: SampleLayout::Planar,
        planes,
        fence_id: take_u64(&mut input),
    };

    let buffer_size = take_u64(&mut input).unwrap_or(0) as usize;
    let _ = image.validate(buffer_size);
    let _ = MediaInfo::image(image, timing, buffer_size);

    // dg-media MediaFrameMeta fuzzing.
    let mut meta = MediaFrameMeta {
        pts: take_u64(&mut input).map(|v| v as i64),
        dts: take_u64(&mut input).map(|v| v as i64),
        stream_id: None,
        tags: std::collections::BTreeMap::new(),
        media_info: None,
        stream_metadata: None,
    };
    let _ = normalize_media_frame_meta(&mut meta);
    let _ = media_frame_timing(&meta);
});
