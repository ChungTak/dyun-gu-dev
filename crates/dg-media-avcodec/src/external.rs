//! External memory import/export facade for avcodec `Image` / `Packet`.
//!
//! Host-owned buffers use safe constructors. Device / foreign handles go through
//! [`import_external_image`] / [`import_external_packet`], which wrap the upstream
//! `from_external_descriptor` APIs. `unsafe` is confined to those thin wrappers
//! (this crate is the media FFI adapter boundary).

#[cfg(feature = "avcodec-sdk")]
use avcodec::core::{
    AvError, BitstreamFormat, CodecId, ColorInfo, ExternalBufferDescriptor, ExternalDropGuard,
    ExternalHandle, ExternalImageDescriptor, ExternalPacketDescriptor, ExternalPlaneDescriptor,
    HdrStaticMetadata, Image, ImageInfo, MemoryDomain, Packet, PacketFlags, TimeBase,
};

/// Result of probing external buffer export availability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalExport {
    pub supported: bool,
    pub message: String,
}

/// Probes whether external buffer import/export APIs are available.
pub fn probe_external_export() -> ExternalExport {
    #[cfg(feature = "avcodec-sdk")]
    {
        ExternalExport {
            supported: true,
            message: "ExternalImageDescriptor / ExternalPacketDescriptor import is available; \
                      host-owned import is safe, device handles require import_external_* \
                      with caller-owned lifetime"
                .into(),
        }
    }
    #[cfg(not(feature = "avcodec-sdk"))]
    {
        ExternalExport {
            supported: false,
            message: "avcodec SDK is not enabled; enable an avcodec-profile-* feature".into(),
        }
    }
}

/// Legacy stub retained for callers; prefer [`import_host_image_gray8`] or
/// [`import_external_image`].
#[deprecated(
    since = "0.1.0",
    note = "use import_host_image_* or import_external_image instead"
)]
pub fn try_import_external_image() -> Result<(), String> {
    Err(
        "try_import_external_image is deprecated; use import_host_image_gray8 / \
         import_host_image_rgb24 for owned host buffers, or import_external_image for \
         ExternalImageDescriptor"
            .to_string(),
    )
}

/// Builds a packed Gray8 host [`Image`] from owned bytes (safe, zero external FD).
#[cfg(feature = "avcodec-sdk")]
pub fn import_host_image_gray8(
    width: u32,
    height: u32,
    stride: usize,
    bytes: Vec<u8>,
) -> Result<Image, AvError> {
    Image::new_host_packed(ImageInfo::Gray8, width, height, 0, stride, bytes, 1)
}

/// Builds a packed RGB24 host [`Image`] from owned bytes.
#[cfg(feature = "avcodec-sdk")]
pub fn import_host_image_rgb24(
    width: u32,
    height: u32,
    stride: usize,
    bytes: Vec<u8>,
) -> Result<Image, AvError> {
    Image::new_host_packed(ImageInfo::Rgb24, width, height, 0, stride, bytes, 1)
}

/// Timing and stream metadata for [`import_host_packet`].
#[cfg(feature = "avcodec-sdk")]
#[derive(Clone, Debug, Default)]
pub struct HostPacketMeta {
    pub stream_index: u32,
    pub pts: Option<i64>,
    pub dts: Option<i64>,
    pub time_base: Option<TimeBase>,
    pub keyframe: bool,
}

/// Builds a host [`Packet`] from owned payload bytes (safe).
#[cfg(feature = "avcodec-sdk")]
pub fn import_host_packet(
    codec: CodecId,
    bitstream_format: BitstreamFormat,
    bytes: Vec<u8>,
    meta: HostPacketMeta,
) -> Packet {
    let mut packet = Packet::from_host_bytes(0, codec, bitstream_format, bytes);
    packet.stream_index = meta.stream_index;
    packet.pts = meta.pts;
    packet.dts = meta.dts;
    packet.time_base = meta.time_base;
    if meta.keyframe {
        packet.flags = PacketFlags::KEY;
    }
    packet
}

/// Imports an [`Image`] from an external descriptor (device or foreign host pointer).
///
/// # Safety
/// Same invariants as [`Image::from_external_descriptor`]: the handle must identify a valid
/// allocation of at least `descriptor.buffer.size` bytes in `descriptor.buffer.domain` and remain
/// valid while the returned `Image` and any clones are alive. Prefer providing a
/// [`ExternalDropGuard`] so the resource is released exactly once.
#[cfg(feature = "avcodec-sdk")]
pub unsafe fn import_external_image(descriptor: ExternalImageDescriptor) -> Result<Image, AvError> {
    // SAFETY: forwarded to the caller of this function.
    unsafe { Image::from_external_descriptor(descriptor) }
}

/// Imports a [`Packet`] from an external descriptor.
///
/// # Safety
/// Same invariants as [`Packet::from_external_descriptor`].
#[cfg(feature = "avcodec-sdk")]
pub unsafe fn import_external_packet(
    descriptor: ExternalPacketDescriptor,
) -> Result<Packet, AvError> {
    // SAFETY: forwarded to the caller of this function.
    unsafe { Packet::from_external_descriptor(descriptor) }
}

/// Convenience: build a host [`ExternalImageDescriptor`] for a single packed plane.
///
/// The returned descriptor does **not** take ownership of `raw`; pair with
/// [`import_external_image`] only when the allocation outlives the `Image`, or attach a drop guard.
#[cfg(feature = "avcodec-sdk")]
pub fn host_packed_image_descriptor(
    info: ImageInfo,
    coded_width: u32,
    coded_height: u32,
    size: usize,
    handle: ExternalHandle,
    plane: ExternalPlaneDescriptor,
    drop_guard: Option<ExternalDropGuard>,
) -> ExternalImageDescriptor {
    ExternalImageDescriptor {
        info,
        coded_width,
        coded_height,
        color: ColorInfo::unspecified(),
        hdr: None::<HdrStaticMetadata>,
        buffer: ExternalBufferDescriptor {
            domain: MemoryDomain::Host,
            size,
            handle,
            drop_guard,
        },
        planes: vec![plane],
    }
}

#[cfg(all(test, feature = "avcodec-sdk"))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn host_gray8_import_is_readable() {
        let image = import_host_image_gray8(2, 2, 2, vec![1, 2, 3, 4]).expect("image");
        assert_eq!(image.coded_width, 2);
        assert_eq!(image.coded_height, 2);
        assert_eq!(image.format, ImageInfo::Gray8);
        assert_eq!(image.memory.domain(), MemoryDomain::Host);
    }

    #[test]
    fn host_packet_import_preserves_codec_and_stream() {
        let packet = import_host_packet(
            CodecId::H264,
            BitstreamFormat::H264AnnexB,
            vec![0, 0, 0, 1, 0x65],
            HostPacketMeta {
                stream_index: 7,
                pts: Some(10),
                dts: Some(8),
                time_base: Some(TimeBase::new(1, 30)),
                keyframe: true,
            },
        );
        assert_eq!(packet.codec, CodecId::H264);
        assert_eq!(packet.stream_index, 7);
        assert_eq!(packet.pts, Some(10));
        assert!(packet.flags.contains(PacketFlags::KEY));
    }

    #[test]
    fn external_device_image_drop_guard_runs_once() {
        let flag = Box::leak(Box::new(AtomicBool::new(false)));
        let token = flag as *mut AtomicBool as usize as u64;
        unsafe fn mark(token: u64) {
            let flag = unsafe { &*(token as *const AtomicBool) };
            flag.store(true, Ordering::SeqCst);
        }
        let descriptor = ExternalImageDescriptor {
            info: ImageInfo::Gray8,
            coded_width: 2,
            coded_height: 2,
            buffer: ExternalBufferDescriptor {
                domain: MemoryDomain::CudaDevice,
                size: 4,
                handle: ExternalHandle {
                    fd: None,
                    raw: 0xDEAD_BEEF,
                },
                drop_guard: Some(ExternalDropGuard::new(token, mark)),
            },
            planes: vec![ExternalPlaneDescriptor {
                offset: 0,
                stride: 2,
                len: 4,
            }],
        };
        let image = unsafe { import_external_image(descriptor) }.expect("image");
        assert_eq!(image.memory.domain(), MemoryDomain::CudaDevice);
        drop(image);
        assert!(flag.load(Ordering::SeqCst));
    }

    #[test]
    fn probe_reports_supported_when_sdk_enabled() {
        let probe = probe_external_export();
        assert!(probe.supported);
    }
}
