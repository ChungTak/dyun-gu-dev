//! Vendor-neutral media metadata value objects.
//!
//! These types carry encoded bitstream and decoded image metadata across graph
//! elements without depending on avcodec, cheetah, or any backend SDK.

use std::collections::BTreeMap;

use crate::{Error, Result};

/// Maximum number of codec configuration blobs per encoded frame.
pub const MAX_CODEC_CONFIG_COUNT: usize = 8;
/// Maximum size of a single codec configuration blob.
pub const MAX_CODEC_CONFIG_ITEM_BYTES: usize = 1 << 20;
/// Maximum total size of all codec configuration blobs.
pub const MAX_CODEC_CONFIG_TOTAL_BYTES: usize = 4 << 20;
/// Maximum number of image planes.
pub const MAX_IMAGE_PLANES: usize = 4;

/// Rational time base for media timestamps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MediaTimeBase {
    pub num: u32,
    pub den: u32,
}

impl MediaTimeBase {
    pub const fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }

    pub fn validate(self) -> Result<()> {
        if self.den == 0 {
            return Err(Error::InvalidArgument(format!(
                "media timebase denominator must be non-zero, got {}/{}",
                self.num, self.den
            )));
        }
        Ok(())
    }
}

/// Timing metadata shared by encoded and image payloads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MediaTiming {
    pub pts: Option<i64>,
    pub dts: Option<i64>,
    pub time_base: Option<MediaTimeBase>,
}

impl MediaTiming {
    pub fn validate(&self) -> Result<()> {
        if let Some(tb) = self.time_base {
            tb.validate()?;
        }
        Ok(())
    }
}

/// Media kind carried by an encoded payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum MediaKind {
    #[default]
    Video,
    Audio,
    Data,
    Subtitle,
    Unknown,
}

/// Codec identity for encoded media.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum MediaCodec {
    H264,
    H265,
    H266,
    AV1,
    VP8,
    VP9,
    MJPEG,
    Jpeg,
    AAC,
    ADPCM,
    Opus,
    G711A,
    G711U,
    MP2,
    MP3,
    #[default]
    Unknown,
}

/// Canonical bitstream format for encoded payloads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum BitstreamFormat {
    H264AnnexB,
    H264Avcc,
    H265AnnexB,
    H265Hvcc,
    Vp8Frame,
    Vp9Frame,
    Av1Obu,
    JpegInterchange,
    AacRaw,
    AacAdts,
    #[default]
    Unknown,
}

/// Flags carried by encoded packet metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EncodedPacketFlags {
    pub key: bool,
    pub lost: bool,
    pub corrupt: bool,
}

/// A single codec configuration blob (e.g. SPS/PPS, HVCC record).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaCodecConfig {
    pub format: BitstreamFormat,
    pub data: Vec<u8>,
}

impl MediaCodecConfig {
    pub fn new(format: BitstreamFormat, data: Vec<u8>) -> Result<Self> {
        if data.len() > MAX_CODEC_CONFIG_ITEM_BYTES {
            return Err(Error::InvalidArgument(format!(
                "codec config item exceeds {MAX_CODEC_CONFIG_ITEM_BYTES} bytes"
            )));
        }
        Ok(Self { format, data })
    }
}

/// Metadata for encoded (compressed) media payloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedMediaInfo {
    pub stream_index: u32,
    pub track_id: Option<u64>,
    pub media_kind: MediaKind,
    pub codec: MediaCodec,
    pub bitstream_format: BitstreamFormat,
    pub flags: EncodedPacketFlags,
    pub codec_configs: Vec<MediaCodecConfig>,
}

impl EncodedMediaInfo {
    pub fn validate(&self) -> Result<()> {
        if self.codec_configs.len() > MAX_CODEC_CONFIG_COUNT {
            return Err(Error::InvalidArgument(format!(
                "codec config count {} exceeds maximum {MAX_CODEC_CONFIG_COUNT}",
                self.codec_configs.len()
            )));
        }
        let mut total = 0usize;
        for config in &self.codec_configs {
            config.data.len(); // already bounded at construction
            total = total
                .checked_add(config.data.len())
                .ok_or_else(|| Error::InvalidArgument("codec config total size overflow".into()))?;
            if total > MAX_CODEC_CONFIG_TOTAL_BYTES {
                return Err(Error::InvalidArgument(format!(
                    "codec config total size exceeds {MAX_CODEC_CONFIG_TOTAL_BYTES} bytes"
                )));
            }
        }
        if self.bitstream_format == BitstreamFormat::Unknown && self.codec != MediaCodec::Unknown {
            return Err(Error::InvalidArgument(
                "bitstream format Unknown is only valid when codec is Unknown".into(),
            ));
        }
        Ok(())
    }
}

/// Pixel format for decoded image metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    Yuv420P,
    Yuv422P,
    Yuv444P,
    Nv12,
    Nv21,
    Rgb24,
    Bgr24,
    Rgba,
    Bgra,
    Gray8,
    Unknown,
}

/// Color primaries for image metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ColorPrimaries {
    #[default]
    Unknown,
    Bt709,
    Bt601,
    Bt2020,
}

/// Color transfer characteristic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ColorTransfer {
    #[default]
    Unknown,
    Bt709,
    Linear,
    Srgb,
}

/// Color matrix coefficients.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ColorMatrix {
    #[default]
    Unknown,
    Bt709,
    Bt601,
    Bt2020,
}

/// Color range for image metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ColorRange {
    #[default]
    Unknown,
    Limited,
    Full,
}

/// Sample storage layout for image planes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SampleLayout {
    #[default]
    Planar,
    SemiPlanar,
    Interleaved,
}

/// Sample element type for image metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum SampleType {
    #[default]
    Uint8,
    Uint16,
    Float16,
    Float32,
}

/// Flags carried by image metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ImageFlags {
    pub key: bool,
}

/// Axis-aligned rectangle in pixel coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MediaRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl MediaRect {
    pub fn validate(self) -> Result<()> {
        self.width
            .checked_add(self.x)
            .ok_or_else(|| Error::InvalidArgument("rect width + x overflows u32".into()))?;
        self.height
            .checked_add(self.y)
            .ok_or_else(|| Error::InvalidArgument("rect height + y overflows u32".into()))?;
        Ok(())
    }
}

/// Layout of a single image plane within a buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MediaPlaneLayout {
    pub offset: usize,
    pub stride: usize,
    pub len: usize,
}

impl MediaPlaneLayout {
    pub fn end_offset(self) -> Result<usize> {
        self.offset
            .checked_add(self.len)
            .ok_or_else(|| Error::InvalidArgument("plane offset + len overflows".into()))
    }
}

/// Metadata for decoded image payloads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageMediaInfo {
    pub pixel_format: PixelFormat,
    pub coded_width: u32,
    pub coded_height: u32,
    pub visible_rect: MediaRect,
    pub crop_rect: Option<MediaRect>,
    pub color_primaries: ColorPrimaries,
    pub color_transfer: ColorTransfer,
    pub color_matrix: ColorMatrix,
    pub color_range: ColorRange,
    pub flags: ImageFlags,
    pub sample_type: SampleType,
    pub sample_layout: SampleLayout,
    pub planes: Vec<MediaPlaneLayout>,
    pub fence_id: Option<u64>,
}

impl ImageMediaInfo {
    pub fn validate(&self, buffer_size: usize) -> Result<()> {
        if self.coded_width == 0 || self.coded_height == 0 {
            return Err(Error::InvalidArgument(
                "coded width and height must be non-zero".into(),
            ));
        }
        self.visible_rect.validate()?;
        if let Some(crop) = self.crop_rect {
            crop.validate()?;
        }

        let expected_planes = expected_plane_count(self.pixel_format)?;
        if self.planes.len() != expected_planes {
            return Err(Error::InvalidArgument(format!(
                "pixel format {:?} requires {expected_planes} planes, got {}",
                self.pixel_format,
                self.planes.len()
            )));
        }

        for (index, plane) in self.planes.iter().enumerate() {
            if plane.stride == 0 {
                return Err(Error::InvalidArgument(
                    "plane stride must be non-zero".into(),
                ));
            }
            let end = plane.end_offset()?;
            if end > buffer_size {
                return Err(Error::InvalidArgument(format!(
                    "plane end offset {end} exceeds buffer size {buffer_size}"
                )));
            }
            validate_plane_stride(
                self.pixel_format,
                plane,
                index,
                self.coded_width,
                self.coded_height,
            )?;
        }
        Ok(())
    }
}

fn expected_plane_count(format: PixelFormat) -> Result<usize> {
    match format {
        PixelFormat::Yuv420P | PixelFormat::Yuv422P | PixelFormat::Yuv444P => Ok(3),
        PixelFormat::Nv12 | PixelFormat::Nv21 => Ok(2),
        PixelFormat::Rgb24
        | PixelFormat::Bgr24
        | PixelFormat::Rgba
        | PixelFormat::Bgra
        | PixelFormat::Gray8 => Ok(1),
        PixelFormat::Unknown => Err(Error::Unsupported(
            "cannot validate plane layout for Unknown pixel format".into(),
        )),
    }
}

fn bytes_per_pixel_row(format: PixelFormat) -> Result<usize> {
    match format {
        PixelFormat::Rgb24 | PixelFormat::Bgr24 => Ok(3),
        PixelFormat::Rgba | PixelFormat::Bgra => Ok(4),
        PixelFormat::Gray8 => Ok(1),
        PixelFormat::Yuv420P | PixelFormat::Yuv422P | PixelFormat::Yuv444P => Ok(1),
        PixelFormat::Nv12 | PixelFormat::Nv21 => Ok(1),
        PixelFormat::Unknown => Err(Error::Unsupported(
            "cannot compute row bytes for Unknown pixel format".into(),
        )),
    }
}

fn plane_row_count(format: PixelFormat, plane_index: usize, coded_height: u32) -> Result<usize> {
    let height = usize::try_from(coded_height)
        .map_err(|_| Error::InvalidArgument("coded height does not fit in usize".into()))?;
    match format {
        PixelFormat::Yuv420P if plane_index > 0 => Ok(height.div_ceil(2)),
        PixelFormat::Nv12 | PixelFormat::Nv21 if plane_index > 0 => Ok(height.div_ceil(2)),
        _ => Ok(height),
    }
}

fn validate_plane_stride(
    format: PixelFormat,
    plane: &MediaPlaneLayout,
    plane_index: usize,
    coded_width: u32,
    coded_height: u32,
) -> Result<()> {
    let bpp = bytes_per_pixel_row(format)?;
    let min_stride = match format {
        PixelFormat::Yuv420P if plane_index > 0 => usize::try_from(coded_width)
            .map_err(|_| Error::InvalidArgument("coded width does not fit in usize".into()))?
            .div_ceil(2),
        PixelFormat::Nv12 | PixelFormat::Nv21 => usize::try_from(coded_width)
            .map_err(|_| Error::InvalidArgument("coded width does not fit in usize".into()))?,
        _ => usize::try_from(coded_width)
            .map_err(|_| Error::InvalidArgument("coded width does not fit in usize".into()))?
            .saturating_mul(bpp),
    };
    if plane.stride < min_stride {
        return Err(Error::InvalidArgument(format!(
            "plane stride {} is less than minimum {min_stride} for format {format:?}",
            plane.stride
        )));
    }
    let rows = plane_row_count(format, plane_index, coded_height)?;
    let last_row_start = plane
        .offset
        .checked_add(plane.stride.saturating_mul(rows.saturating_sub(1)))
        .ok_or_else(|| Error::InvalidArgument("last row offset overflow".into()))?;
    let last_row_end = last_row_start
        .checked_add(min_stride)
        .ok_or_else(|| Error::InvalidArgument("last row end overflow".into()))?;
    let plane_end = plane.end_offset()?;
    if last_row_end > plane_end {
        return Err(Error::InvalidArgument(
            "last valid row extends beyond plane length".into(),
        ));
    }
    Ok(())
}

/// Payload-specific media metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaPayloadInfo {
    Encoded(EncodedMediaInfo),
    Image(ImageMediaInfo),
}

impl MediaPayloadInfo {
    pub fn validate(&self, buffer_size: Option<usize>) -> Result<()> {
        match self {
            Self::Encoded(info) => info.validate(),
            Self::Image(info) => {
                let size = buffer_size.ok_or_else(|| {
                    Error::InvalidArgument(
                        "buffer size required to validate image media info".into(),
                    )
                })?;
                info.validate(size)
            }
        }
    }
}

/// Unified media metadata envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaInfo {
    pub timing: MediaTiming,
    pub payload: MediaPayloadInfo,
}

impl MediaInfo {
    pub fn encoded(info: EncodedMediaInfo, timing: MediaTiming) -> Result<Self> {
        timing.validate()?;
        info.validate()?;
        Ok(Self {
            timing,
            payload: MediaPayloadInfo::Encoded(info),
        })
    }

    pub fn image(info: ImageMediaInfo, timing: MediaTiming, buffer_size: usize) -> Result<Self> {
        timing.validate()?;
        info.validate(buffer_size)?;
        Ok(Self {
            timing,
            payload: MediaPayloadInfo::Image(info),
        })
    }

    pub fn validate(&self, buffer_size: Option<usize>) -> Result<()> {
        self.timing.validate()?;
        self.payload.validate(buffer_size)
    }

    pub fn is_keyframe(&self) -> bool {
        match &self.payload {
            MediaPayloadInfo::Encoded(e) => e.flags.key,
            MediaPayloadInfo::Image(i) => i.flags.key,
        }
    }
}

/// Optional string tags that remain transport-only and must not carry codec config.
pub type MediaTags = BTreeMap<String, String>;

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn valid_timebase() -> MediaTimeBase {
        MediaTimeBase::new(1, 90_000)
    }

    #[test]
    fn zero_timebase_denominator_fails() {
        let err = MediaTimeBase::new(1, 0).validate().unwrap_err();
        assert!(matches!(err, Error::InvalidArgument(_)));
    }

    #[test]
    fn codec_config_size_limits() {
        let oversized = vec![0u8; MAX_CODEC_CONFIG_ITEM_BYTES + 1];
        assert!(MediaCodecConfig::new(BitstreamFormat::H264AnnexB, oversized).is_err());
    }

    #[test]
    fn encoded_info_rejects_excess_configs() {
        let configs: Vec<_> = (0..MAX_CODEC_CONFIG_COUNT + 1)
            .map(|_| {
                MediaCodecConfig::new(BitstreamFormat::H264AnnexB, vec![1, 2, 3]).expect("cfg")
            })
            .collect();
        let info = EncodedMediaInfo {
            stream_index: 0,
            track_id: None,
            media_kind: MediaKind::Video,
            codec: MediaCodec::H264,
            bitstream_format: BitstreamFormat::H264AnnexB,
            flags: EncodedPacketFlags::default(),
            codec_configs: configs,
        };
        assert!(info.validate().is_err());
    }

    #[test]
    fn h264_annex_b_round_trip() {
        let info = EncodedMediaInfo {
            stream_index: 2,
            track_id: Some(7),
            media_kind: MediaKind::Video,
            codec: MediaCodec::H264,
            bitstream_format: BitstreamFormat::H264AnnexB,
            flags: EncodedPacketFlags {
                key: true,
                lost: false,
                corrupt: false,
            },
            codec_configs: vec![
                MediaCodecConfig::new(BitstreamFormat::H264AnnexB, vec![0, 0, 0, 1, 103])
                    .expect("sps"),
                MediaCodecConfig::new(BitstreamFormat::H264AnnexB, vec![0, 0, 0, 1, 104])
                    .expect("pps"),
            ],
        };
        let timing = MediaTiming {
            pts: Some(90_000),
            dts: Some(-1),
            time_base: Some(valid_timebase()),
        };
        let media = MediaInfo::encoded(info, timing).expect("media");
        assert!(media.is_keyframe());
        assert_eq!(media.timing.pts, Some(90_000));
    }

    #[test]
    fn h265_hvcc_config_accepted() {
        let info = EncodedMediaInfo {
            stream_index: 0,
            track_id: None,
            media_kind: MediaKind::Video,
            codec: MediaCodec::H265,
            bitstream_format: BitstreamFormat::H265Hvcc,
            flags: EncodedPacketFlags::default(),
            codec_configs: vec![
                MediaCodecConfig::new(BitstreamFormat::H265Hvcc, vec![1, 2, 3, 4]).expect("hvcc"),
            ],
        };
        assert!(MediaInfo::encoded(info, MediaTiming::default()).is_ok());
    }

    #[test]
    fn nv12_padded_stride_validates() {
        let width = 1920u32;
        let height = 1080u32;
        let y_stride = 2048usize;
        let y_size = y_stride * usize::try_from(height).expect("height");
        let uv_stride = 2048usize;
        let uv_height = usize::try_from(height).expect("height") / 2;
        let uv_size = uv_stride * uv_height;
        let buffer_size = y_size + uv_size;
        let info = ImageMediaInfo {
            pixel_format: PixelFormat::Nv12,
            coded_width: width,
            coded_height: height,
            visible_rect: MediaRect {
                x: 0,
                y: 0,
                width,
                height,
            },
            crop_rect: None,
            color_primaries: ColorPrimaries::Bt709,
            color_transfer: ColorTransfer::Bt709,
            color_matrix: ColorMatrix::Bt709,
            color_range: ColorRange::Limited,
            flags: ImageFlags::default(),
            sample_type: SampleType::Uint8,
            sample_layout: SampleLayout::SemiPlanar,
            planes: vec![
                MediaPlaneLayout {
                    offset: 0,
                    stride: y_stride,
                    len: y_size,
                },
                MediaPlaneLayout {
                    offset: y_size,
                    stride: uv_stride,
                    len: uv_size,
                },
            ],
            fence_id: None,
        };
        assert!(MediaInfo::image(info, MediaTiming::default(), buffer_size).is_ok());
    }

    #[test]
    fn nv12_stride_too_small_fails() {
        let info = ImageMediaInfo {
            pixel_format: PixelFormat::Nv12,
            coded_width: 64,
            coded_height: 64,
            visible_rect: MediaRect {
                x: 0,
                y: 0,
                width: 64,
                height: 64,
            },
            crop_rect: None,
            color_primaries: ColorPrimaries::default(),
            color_transfer: ColorTransfer::default(),
            color_matrix: ColorMatrix::default(),
            color_range: ColorRange::default(),
            flags: ImageFlags::default(),
            sample_type: SampleType::Uint8,
            sample_layout: SampleLayout::SemiPlanar,
            planes: vec![
                MediaPlaneLayout {
                    offset: 0,
                    stride: 32,
                    len: 32 * 64,
                },
                MediaPlaneLayout {
                    offset: 32 * 64,
                    stride: 32,
                    len: 32 * 32,
                },
            ],
            fence_id: None,
        };
        assert!(info.validate(32 * 64 + 32 * 32).is_err());
    }

    proptest! {
        #[test]
        fn timebase_den_nonzero(num in 1u32..10_000, den in 1u32..1_000_000) {
            MediaTimeBase::new(num, den).validate().expect("valid tb");
        }

        #[test]
        fn codec_config_total_within_limit(
            sizes in prop::collection::vec(0usize..1024, 1..=MAX_CODEC_CONFIG_COUNT)
        ) {
            let total: usize = sizes.iter().sum();
            prop_assume!(total <= MAX_CODEC_CONFIG_TOTAL_BYTES);
            let configs: Vec<_> = sizes
                .into_iter()
                .map(|n| MediaCodecConfig::new(BitstreamFormat::H264AnnexB, vec![0u8; n]).expect("cfg"))
                .collect();
            let info = EncodedMediaInfo {
                stream_index: 0,
                track_id: None,
                media_kind: MediaKind::Video,
                codec: MediaCodec::H264,
                bitstream_format: BitstreamFormat::H264AnnexB,
                flags: EncodedPacketFlags::default(),
                codec_configs: configs,
            };
            prop_assert!(info.validate().is_ok());
        }

        #[test]
        fn rect_overflow_rejected(width in 1u32..1000, x in 1u32..=u32::MAX / 2) {
            let rect = MediaRect { x, y: 0, width, height: 1 };
            if x.checked_add(width).is_none() {
                prop_assert!(rect.validate().is_err());
            }
        }
    }
}
