//! Mappers between dg-core media types and avcodec SDK types.
#![forbid(unsafe_code)]

use dg_core::{
    BitstreamFormat as CoreBitstream, EncodedMediaInfo, EncodedPacketFlags, Error, ImageMediaInfo,
    MediaCodec as CoreCodec, MediaInfo, MediaKind, MediaPayloadInfo, MediaPlaneLayout, MediaRect,
    MediaTimeBase, MediaTiming, PixelFormat as CorePixel, Result, SampleLayout, SampleType,
};

use dg_media_avcodec::{BitstreamFormat, CodecId, ImageInfo, Packet, PacketFlags};

pub fn core_codec_to_avcodec(codec: CoreCodec) -> Result<CodecId> {
    match codec {
        CoreCodec::H264 => Ok(CodecId::H264),
        CoreCodec::H265 => Ok(CodecId::H265),
        CoreCodec::VP8 => Ok(CodecId::Vp8),
        CoreCodec::VP9 => Ok(CodecId::Vp9),
        CoreCodec::AV1 => Ok(CodecId::Av1),
        CoreCodec::MJPEG => Ok(CodecId::Mjpeg),
        CoreCodec::Jpeg => Ok(CodecId::Jpeg),
        CoreCodec::Unknown => Err(Error::Unsupported(
            "cannot map Unknown media codec to avcodec".into(),
        )),
        other => Err(Error::Unsupported(format!(
            "codec {other:?} is not supported by video bridge"
        ))),
    }
}

pub fn avcodec_codec_to_core(codec: CodecId) -> CoreCodec {
    match codec {
        CodecId::H264 => CoreCodec::H264,
        CodecId::H265 => CoreCodec::H265,
        CodecId::Vp8 => CoreCodec::VP8,
        CodecId::Vp9 => CoreCodec::VP9,
        CodecId::Av1 => CoreCodec::AV1,
        CodecId::Mjpeg => CoreCodec::MJPEG,
        CodecId::Jpeg => CoreCodec::Jpeg,
        _ => CoreCodec::Unknown,
    }
}

pub fn core_bitstream_to_avcodec(format: CoreBitstream) -> Result<BitstreamFormat> {
    match format {
        CoreBitstream::H264AnnexB => Ok(BitstreamFormat::H264AnnexB),
        CoreBitstream::H264Avcc => Ok(BitstreamFormat::H264Avcc),
        CoreBitstream::H265AnnexB => Ok(BitstreamFormat::H265AnnexB),
        CoreBitstream::H265Hvcc => Ok(BitstreamFormat::H265Hvcc),
        CoreBitstream::Vp8Frame => Ok(BitstreamFormat::Vp8Frame),
        CoreBitstream::Vp9Frame => Ok(BitstreamFormat::Vp9Frame),
        CoreBitstream::Av1Obu => Ok(BitstreamFormat::Av1Obu),
        CoreBitstream::JpegInterchange => Ok(BitstreamFormat::JpegInterchange),
        CoreBitstream::Unknown => Err(Error::Unsupported(
            "cannot map Unknown bitstream format to avcodec".into(),
        )),
        other => Err(Error::Unsupported(format!(
            "bitstream format {other:?} is not supported by video bridge"
        ))),
    }
}

pub fn avcodec_bitstream_to_core(format: BitstreamFormat) -> CoreBitstream {
    match format {
        BitstreamFormat::H264AnnexB => CoreBitstream::H264AnnexB,
        BitstreamFormat::H264Avcc => CoreBitstream::H264Avcc,
        BitstreamFormat::H265AnnexB => CoreBitstream::H265AnnexB,
        BitstreamFormat::H265Hvcc => CoreBitstream::H265Hvcc,
        BitstreamFormat::Vp8Frame => CoreBitstream::Vp8Frame,
        BitstreamFormat::Vp9Frame => CoreBitstream::Vp9Frame,
        BitstreamFormat::Av1Obu => CoreBitstream::Av1Obu,
        BitstreamFormat::JpegInterchange => CoreBitstream::JpegInterchange,
        _ => CoreBitstream::Unknown,
    }
}

pub fn avcodec_image_to_core_pixel(format: ImageInfo) -> CorePixel {
    // Future upstream variants may appear before dyun adds explicit mappings.
    match format {
        ImageInfo::Yuv420p => CorePixel::Yuv420P,
        ImageInfo::Yuv422p => CorePixel::Yuv422P,
        ImageInfo::Yuv444p => CorePixel::Yuv444P,
        ImageInfo::Nv12 => CorePixel::Nv12,
        ImageInfo::Nv21 => CorePixel::Nv21,
        ImageInfo::Rgb24 => CorePixel::Rgb24,
        ImageInfo::Bgr24 => CorePixel::Bgr24,
        ImageInfo::Rgba => CorePixel::Rgba,
        ImageInfo::Bgra => CorePixel::Bgra,
        ImageInfo::Gray8 => CorePixel::Gray8,
        ImageInfo::Yuv420p10le | ImageInfo::P010 => CorePixel::Unknown,
    }
}

pub fn core_pixel_to_avcodec(format: CorePixel) -> Result<ImageInfo> {
    match format {
        CorePixel::Yuv420P => Ok(ImageInfo::Yuv420p),
        CorePixel::Yuv422P => Ok(ImageInfo::Yuv422p),
        CorePixel::Yuv444P => Ok(ImageInfo::Yuv444p),
        CorePixel::Nv12 => Ok(ImageInfo::Nv12),
        CorePixel::Nv21 => Ok(ImageInfo::Nv21),
        CorePixel::Rgb24 => Ok(ImageInfo::Rgb24),
        CorePixel::Bgr24 => Ok(ImageInfo::Bgr24),
        CorePixel::Rgba => Ok(ImageInfo::Rgba),
        CorePixel::Bgra => Ok(ImageInfo::Bgra),
        CorePixel::Gray8 => Ok(ImageInfo::Gray8),
        CorePixel::Unknown => Err(Error::Unsupported(
            "cannot map Unknown pixel format to avcodec".into(),
        )),
    }
}

/// True when the pixel format uses more than one plane (planar/semi-planar).
#[must_use]
pub fn is_multiplane_pixel(format: CorePixel) -> bool {
    matches!(
        format,
        CorePixel::Yuv420P
            | CorePixel::Yuv422P
            | CorePixel::Yuv444P
            | CorePixel::Nv12
            | CorePixel::Nv21
    )
}

pub fn packet_to_encoded_media_info(packet: &Packet) -> Result<EncodedMediaInfo> {
    Ok(EncodedMediaInfo {
        stream_index: packet.stream_index,
        track_id: None,
        media_kind: MediaKind::Video,
        codec: avcodec_codec_to_core(packet.codec),
        bitstream_format: avcodec_bitstream_to_core(packet.bitstream_format),
        flags: EncodedPacketFlags {
            key: packet.flags.contains(PacketFlags::KEY),
            lost: packet.flags.contains(PacketFlags::LOST),
            corrupt: packet.flags.contains(PacketFlags::CORRUPT),
        },
        codec_configs: Vec::new(),
    })
}

pub fn packet_to_media_info(packet: &Packet) -> Result<MediaInfo> {
    let timing = MediaTiming {
        pts: packet.pts,
        dts: packet.dts,
        time_base: packet
            .time_base
            .map(|tb| MediaTimeBase::new(tb.num, tb.den)),
    };
    MediaInfo::encoded(packet_to_encoded_media_info(packet)?, timing)
}

pub fn image_to_media_info(
    image: &dg_media_avcodec::Image,
    buffer_size: usize,
) -> Result<MediaInfo> {
    let timing = MediaTiming {
        pts: image.pts,
        dts: image.dts,
        time_base: None,
    };
    let mut planes = Vec::new();
    for plane in image.planes.iter().flatten() {
        planes.push(MediaPlaneLayout {
            offset: plane.offset,
            stride: plane.stride,
            len: plane.len,
        });
    }
    let info = ImageMediaInfo {
        pixel_format: avcodec_image_to_core_pixel(image.format),
        coded_width: image.coded_width,
        coded_height: image.coded_height,
        visible_rect: MediaRect {
            x: 0,
            y: 0,
            width: image.coded_width,
            height: image.coded_height,
        },
        crop_rect: None,
        color_primaries: dg_core::ColorPrimaries::Unknown,
        color_transfer: dg_core::ColorTransfer::Unknown,
        color_matrix: dg_core::ColorMatrix::Unknown,
        color_range: dg_core::ColorRange::Unknown,
        flags: dg_core::ImageFlags::default(),
        sample_type: SampleType::Uint8,
        sample_layout: match image.format {
            ImageInfo::Nv12 | ImageInfo::Nv21 => SampleLayout::SemiPlanar,
            ImageInfo::Yuv420p | ImageInfo::Yuv422p | ImageInfo::Yuv444p => SampleLayout::Planar,
            _ => SampleLayout::Interleaved,
        },
        planes,
        fence_id: None,
    };
    MediaInfo::image(info, timing, buffer_size)
}

pub fn encoded_info_from_media_info(info: &MediaInfo) -> Result<&EncodedMediaInfo> {
    match &info.payload {
        MediaPayloadInfo::Encoded(encoded) => Ok(encoded),
        MediaPayloadInfo::Image(_) => Err(Error::Media(
            "expected encoded media metadata for packet bridge".into(),
        )),
    }
}
