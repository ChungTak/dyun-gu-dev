use std::sync::Arc;

use dg_core::{Buffer, DataFormat, DataType, DeviceKind, MemoryDomain, Result, Tensor};
use dg_graph::{Packet, PacketPayload};

#[cfg(feature = "avcodec-sdk")]
use crate::{format_map, CopyPath, TransferMode, TransferPathKind};
use crate::{
    media_frame_timing, normalize_media_frame_meta, MediaFrame, MediaFrameKind, TransferReport,
};

#[cfg(feature = "avcodec-sdk")]
use dg_core::{BufferDesc, ExternalDropGuard};
#[cfg(feature = "avcodec-sdk")]
use tracing::debug;

#[cfg(feature = "avcodec-sdk")]
use crate::ZeroCopyPlanner;
#[cfg(feature = "avcodec-sdk")]
use crate::{FrameLayout, FrameTransferRequest, HandleKind, MemoryDtype, MemoryFormat};

pub fn tensor_to_frame(tensor: Tensor) -> MediaFrame {
    MediaFrame::from_tensor(tensor)
}

pub fn frame_to_tensor(frame: MediaFrame) -> Result<Tensor> {
    frame.into_tensor()
}

pub fn graph_packet_to_media_frame(packet: Packet) -> Result<MediaFrame> {
    let Packet { meta, payload } = packet;
    let tensor = match Arc::try_unwrap(payload) {
        Ok(PacketPayload::Tensor(tensor)) => Some(tensor),
        Ok(
            PacketPayload::Detections(_)
            | PacketPayload::Classifications(_)
            | PacketPayload::Faces(_)
            | PacketPayload::Tracks(_)
            | PacketPayload::Ocr(_)
            | PacketPayload::EndOfStream,
        ) => None,
        Err(payload) => match payload.as_ref() {
            PacketPayload::Tensor(tensor) => Some(tensor.clone()),
            PacketPayload::Detections(_)
            | PacketPayload::Classifications(_)
            | PacketPayload::Faces(_)
            | PacketPayload::Tracks(_)
            | PacketPayload::Ocr(_)
            | PacketPayload::EndOfStream => None,
        },
    };
    match tensor {
        Some(tensor) => {
            let mut frame = MediaFrame::from_tensor(tensor);
            let timing = media_frame_timing(&frame.meta);
            frame.meta.stream_id = meta.stream_id;
            frame.meta.tags = meta.tags;
            frame.meta.media_info = meta.media_info;
            if frame.meta.media_info.is_none() {
                frame.meta.pts = timing.pts;
                frame.meta.dts = timing.dts;
            }
            normalize_media_frame_meta(&mut frame.meta)?;
            Ok(frame)
        }
        None => Ok(MediaFrame::new(
            MediaFrameKind::EndOfStream,
            DataType::U8,
            DataFormat::Auto,
            Vec::new(),
            DeviceKind::Cpu,
            MemoryDomain::Host,
            Buffer::allocate_host(DeviceKind::Cpu, 0),
        )),
    }
}

pub fn media_frame_to_graph_packet(frame: MediaFrame) -> Result<Packet> {
    if frame.is_end_of_stream() {
        return Ok(Packet::eos());
    }
    let MediaFrame {
        kind: _,
        dtype,
        format,
        shape,
        device,
        domain: _,
        buffer,
        mut meta,
    } = frame;
    normalize_media_frame_meta(&mut meta)?;
    let timing = media_frame_timing(&meta);
    let desc = dg_core::TensorDesc::new(dg_core::Shape::new(shape), dtype, format, device);
    let tensor = Tensor::from_buffer(desc, buffer)?;
    Ok(Packet::tensor(tensor).with_meta(dg_graph::PacketMeta {
        sequence: 0,
        stream_id: meta.stream_id,
        tags: meta.tags,
        media_info: meta.media_info.or_else(|| {
            if timing.pts.is_some() || timing.dts.is_some() || timing.time_base.is_some() {
                Some(Box::new(dg_core::MediaInfo {
                    timing,
                    payload: dg_core::MediaPayloadInfo::Encoded(dg_core::EncodedMediaInfo {
                        stream_index: 0,
                        track_id: None,
                        media_kind: dg_core::MediaKind::Unknown,
                        codec: dg_core::MediaCodec::Unknown,
                        bitstream_format: dg_core::BitstreamFormat::Unknown,
                        flags: dg_core::EncodedPacketFlags::default(),
                        codec_configs: Vec::new(),
                    }),
                }))
            } else {
                None
            }
        }),
    }))
}

/// A bridged value together with the transfer actually used to produce it.
#[derive(Clone, Debug)]
pub struct BridgedMediaFrame {
    pub frame: MediaFrame,
    pub transfer: TransferReport,
}

#[cfg(feature = "avcodec-sdk")]
fn staged_host_transfer(source_domain: MemoryDomain) -> TransferReport {
    staged_host_transfer_with_copies(source_domain, 1)
}

#[cfg(feature = "avcodec-sdk")]
fn staged_host_transfer_with_copies(
    source_domain: MemoryDomain,
    copy_count: usize,
) -> TransferReport {
    TransferReport {
        source_domain,
        target_domain: MemoryDomain::Host,
        path: CopyPath {
            domains: vec![source_domain, MemoryDomain::Host],
            copy_count,
        },
        copy_count,
        mode: TransferMode::Staged,
        path_kind: if copy_count == 0 {
            TransferPathKind::OwnershipMove
        } else {
            TransferPathKind::DomainStaging
        },
        reason: None,
    }
}

/// Result of importing an avcodec [`dg_media_avcodec::BufferHandle`] into a
/// dg-core [`Buffer`], with the actual transfer path taken.
#[cfg(feature = "avcodec-sdk")]
#[derive(Clone, Debug)]
pub struct ImportedBuffer {
    pub buffer: Buffer,
    pub zero_copy: bool,
    pub path: CopyPath,
    pub transfer: TransferReport,
}

/// Imports an avcodec buffer handle into a dg-core [`Buffer`] targeting
/// `target_domain`.
///
/// - Source domain equals `target_domain`: the handle is **shared** via
///   [`Buffer::from_external`] + [`ExternalDropGuard`] — the guard keeps a
///   clone of the avcodec handle alive until every clone of the returned
///   buffer is dropped (`copy_count == 0`). Host-readable bytes are exposed
///   through the buffer; non-host device memory is represented by the shared
///   external handle and is not host-readable through `read_bytes`.
/// - Otherwise an explicit **staging fallback** is taken through
///   `stage_to_host` / `stage_to` (`copy_count == 1` per domain crossing).
///   Missing staging support surfaces as [`dg_core::Error::Unsupported`]
///   rather than silently degrading.
///
/// The chosen path and copy count are logged and returned for diagnostics.
#[cfg(feature = "avcodec-sdk")]
pub fn import_avcodec_handle(
    handle: &dg_media_avcodec::BufferHandle,
    device: DeviceKind,
    target_domain: MemoryDomain,
) -> Result<ImportedBuffer> {
    let source_domain = avcodec_memory_domain_to_core(handle.domain());
    let layout = FrameLayout {
        dims: vec![handle.size()],
        format: MemoryFormat::Packet,
        dtype: MemoryDtype::U8,
        plane_count: 1,
        strides: vec![handle.size()],
        subsampling: None,
        packed: true,
    };
    let transfer = ZeroCopyPlanner::new().plan_frame(&FrameTransferRequest {
        source_domain,
        target_domain,
        source_handle: HandleKind::Avcodec,
        target_handle: HandleKind::External,
        source_layout: layout.clone(),
        target_layout: layout,
        has_lifetime_guard: true,
        staging_supported: source_domain != target_domain,
        operation: format!("avcodec handle {}", handle.id()),
    })?;
    let buffer = if transfer.mode == TransferMode::Shared {
        share_avcodec_handle(handle, device, source_domain)?
    } else {
        let staged = handle
            .stage_to(core_memory_domain_to_avcodec(target_domain), handle.id())
            .map_err(|err| match err {
                dg_media_avcodec::AvError::Unsupported => dg_core::Error::Unsupported(format!(
                    "no staging path from {source_domain:?} to {target_domain:?} for avcodec handle {}",
                    handle.id()
                )),
                other => dg_core::Error::Buffer(format!(
                    "staging avcodec handle {} from {source_domain:?} to {target_domain:?} failed: {other:?}",
                    handle.id()
                )),
            })?;
        share_avcodec_handle(&staged, device, target_domain)?
    };
    debug!(
        handle_id = handle.id(),
        source_domain = ?source_domain,
        target_domain = ?target_domain,
        copy_count = transfer.copy_count,
        zero_copy = transfer.mode == TransferMode::Shared,
        path = ?transfer.path.domains,
        "imported avcodec buffer handle"
    );
    Ok(ImportedBuffer {
        buffer,
        zero_copy: transfer.mode == TransferMode::Shared,
        path: transfer.path.clone(),
        transfer,
    })
}

/// Wraps an avcodec handle as a dg-core [`Buffer`] in the same memory domain.
///
/// The returned buffer holds an [`ExternalDropGuard`] owning a clone of the
/// avcodec handle, so the underlying decoder/encoder memory outlives every
/// clone of the buffer. Host-readable handles expose their bytes; device
/// handles only carry the shared [`dg_core::ExternalHandle`] token.
#[cfg(feature = "avcodec-sdk")]
fn share_avcodec_handle(
    handle: &dg_media_avcodec::BufferHandle,
    device: DeviceKind,
    domain: MemoryDomain,
) -> Result<Buffer> {
    let external = avcodec_external_handle_to_core(handle.external());
    let keepalive = handle.clone();
    let guard = ExternalDropGuard::new(move || drop(keepalive));
    let desc = BufferDesc::new(handle.size(), 1);
    match handle.host_bytes() {
        Some(bytes) => Buffer::from_external_with_host_bytes(
            device,
            domain,
            desc,
            external,
            bytes.to_vec(),
            guard,
        ),
        None => Buffer::from_external(device, domain, desc, external, guard),
    }
}

#[cfg(feature = "avcodec-sdk")]
pub fn avcodec_handle_to_buffer(
    handle: &dg_media_avcodec::BufferHandle,
    device: DeviceKind,
) -> Result<Buffer> {
    let source_domain = avcodec_memory_domain_to_core(handle.domain());
    let target_domain = if source_domain == MemoryDomain::Host {
        MemoryDomain::Host
    } else {
        source_domain
    };
    Ok(import_avcodec_handle(handle, device, target_domain)?.buffer)
}

#[cfg(feature = "avcodec-sdk")]
pub fn buffer_to_avcodec_handle(buffer: &Buffer) -> Result<dg_media_avcodec::BufferHandle> {
    Ok(dg_media_avcodec::BufferHandle::from_host_bytes(
        0,
        buffer.try_read_bytes()?,
    ))
}

#[cfg(feature = "avcodec-sdk")]
pub fn avcodec_packet_to_media_frame(packet: &dg_media_avcodec::Packet) -> Result<MediaFrame> {
    Ok(avcodec_packet_to_media_frame_with_transfer(packet)?.frame)
}

#[cfg(feature = "avcodec-sdk")]
pub fn avcodec_packet_to_media_frame_with_transfer(
    packet: &dg_media_avcodec::Packet,
) -> Result<BridgedMediaFrame> {
    let host = packet
        .host_bytes()
        .map_err(|err| dg_core::Error::Buffer(format!("avcodec packet host_bytes: {err:?}")))?;
    let slice = host.ok_or_else(|| {
        dg_core::Error::Buffer("avcodec packet is not host-readable; domain mismatch".to_string())
    })?;
    let bytes = slice.to_vec();
    let shape = vec![bytes.len()];
    let mut frame = MediaFrame::from_host_bytes(
        MediaFrameKind::Tensor,
        DataType::U8,
        DataFormat::N,
        shape,
        DeviceKind::Cpu,
        bytes,
    )?;
    frame.meta.media_info = Some(Box::new(format_map::packet_to_media_info(packet)?));
    normalize_media_frame_meta(&mut frame.meta)?;
    let copy_count = 1;
    Ok(BridgedMediaFrame {
        frame,
        transfer: staged_host_transfer_with_copies(
            avcodec_memory_domain_to_core(packet.data.handle.domain()),
            copy_count,
        ),
    })
}

#[cfg(feature = "avcodec-sdk")]
pub fn media_frame_to_avcodec_packet(
    frame: MediaFrame,
    stream_index: u32,
    codec: dg_media_avcodec::CodecId,
    bitstream_format: dg_media_avcodec::BitstreamFormat,
) -> Result<dg_media_avcodec::Packet> {
    Ok(
        media_frame_to_avcodec_packet_with_transfer(frame, stream_index, codec, bitstream_format)?
            .0,
    )
}

#[cfg(feature = "avcodec-sdk")]
pub fn media_frame_to_avcodec_packet_with_transfer(
    frame: MediaFrame,
    stream_index: u32,
    codec: dg_media_avcodec::CodecId,
    bitstream_format: dg_media_avcodec::BitstreamFormat,
) -> Result<(dg_media_avcodec::Packet, TransferReport)> {
    let mut meta = frame.meta.clone();
    normalize_media_frame_meta(&mut meta)?;
    let timing = media_frame_timing(&meta);
    let (resolved_codec, resolved_format, resolved_stream_index) =
        if let Some(info) = meta.media_info.as_deref() {
            let encoded = format_map::encoded_info_from_media_info(info)?;
            (
                format_map::core_codec_to_avcodec(encoded.codec).unwrap_or(codec),
                format_map::core_bitstream_to_avcodec(encoded.bitstream_format)
                    .unwrap_or(bitstream_format),
                encoded.stream_index,
            )
        } else {
            (codec, bitstream_format, stream_index)
        };
    let source_domain = frame.domain;
    if !frame.buffer.is_host_readable() {
        return Err(dg_core::Error::Buffer(
            "media frame buffer is not host-readable; host packet bridge requires Host domain \
             or allow_staging staging path"
                .to_string(),
        ));
    }
    let copy_count = usize::from(frame.buffer.ref_count() > 1);
    let bytes = frame.buffer.try_into_host_bytes()?;
    // from_host_bytes first argument is buffer_id, not stream_index.
    let mut packet =
        dg_media_avcodec::Packet::from_host_bytes(0, resolved_codec, resolved_format, bytes);
    packet.stream_index = resolved_stream_index;
    packet.pts = timing.pts;
    packet.dts = timing.dts;
    packet.time_base = timing
        .time_base
        .map(|tb| dg_media_avcodec::TimeBase::new(tb.num, tb.den));
    if let Some(info) = meta.media_info.as_deref() {
        if let Ok(encoded) = format_map::encoded_info_from_media_info(info) {
            let mut flags = dg_media_avcodec::PacketFlags::NONE;
            if encoded.flags.key {
                flags |= dg_media_avcodec::PacketFlags::KEY;
            }
            if encoded.flags.lost {
                flags |= dg_media_avcodec::PacketFlags::LOST;
            }
            if encoded.flags.corrupt {
                flags |= dg_media_avcodec::PacketFlags::CORRUPT;
            }
            packet.flags = flags;
        }
    }
    Ok((
        packet,
        TransferReport {
            source_domain,
            target_domain: MemoryDomain::Host,
            path: CopyPath {
                domains: vec![source_domain, MemoryDomain::Host],
                copy_count,
            },
            copy_count,
            mode: if copy_count == 0 {
                TransferMode::Shared
            } else {
                TransferMode::Staged
            },
            path_kind: if copy_count == 0 {
                TransferPathKind::OwnershipMove
            } else {
                TransferPathKind::HostClone
            },
            reason: None,
        },
    ))
}

#[cfg(feature = "avcodec-sdk")]
pub fn avcodec_image_to_media_frame(image: &dg_media_avcodec::Image) -> Result<MediaFrame> {
    Ok(avcodec_image_to_media_frame_with_processor(image, None)?.frame)
}

#[cfg(feature = "avcodec-sdk")]
pub(crate) fn avcodec_image_to_media_frame_with_processor(
    image: &dg_media_avcodec::Image,
    csc_processor: Option<&mut dyn dg_media_avcodec::ImageProcessor>,
) -> Result<BridgedMediaFrame> {
    let requires_csc = csc_processor.is_some();
    let image = if let Some(processor) = csc_processor {
        processor
            .submit(dg_media_avcodec::ImageProcessRequest {
                src: image.clone(),
                op: dg_media_avcodec::ImageOp::Csc {
                    dst_format: dg_media_avcodec::ImageInfo::Rgb24,
                },
                aux: None,
                target_domain: None,
            })
            .map_err(crate::avcodec::map_av_error)?;
        match processor.poll_image() {
            Ok(dg_media_avcodec::Poll::Ready(image)) => image,
            Ok(dg_media_avcodec::Poll::Pending) => {
                return Err(dg_core::Error::Media(
                    "avcodec CSC processor did not produce an output frame".to_string(),
                ))
            }
            Ok(dg_media_avcodec::Poll::EndOfStream) => {
                return Err(dg_core::Error::Media(
                    "avcodec CSC processor ended before producing an output frame".to_string(),
                ))
            }
            Err(error) => return Err(crate::avcodec::map_av_error(error)),
        }
    } else {
        image.clone()
    };

    let (host, path_kind, copy_count) = materialize_avcodec_image_host_bytes(&image)?;
    let host_len = host.len();
    let height = usize::try_from(image.coded_height)
        .map_err(|_| dg_core::Error::Media("image height overflow".to_string()))?;
    let width = usize::try_from(image.coded_width)
        .map_err(|_| dg_core::Error::Media("image width overflow".to_string()))?;
    let pixel = format_map::avcodec_image_to_core_pixel(image.format);
    let (shape, data_format) = if format_map::is_multiplane_pixel(pixel) {
        // Planar/semi-planar: shape is coded geometry only; layout lives in media_info.
        (vec![height, width], DataFormat::Auto)
    } else {
        let channels = match image.format {
            dg_media_avcodec::ImageInfo::Gray8 => 1,
            dg_media_avcodec::ImageInfo::Rgb24 | dg_media_avcodec::ImageInfo::Bgr24 => 3,
            dg_media_avcodec::ImageInfo::Rgba | dg_media_avcodec::ImageInfo::Bgra => 4,
            other => {
                return Err(dg_core::Error::Media(format!(
                    "unsupported packed avcodec image format {other:?}"
                )))
            }
        };
        (vec![height, width, channels], DataFormat::NHWC)
    };
    let mut frame = MediaFrame::from_host_bytes(
        MediaFrameKind::Image,
        DataType::U8,
        data_format,
        shape,
        DeviceKind::Cpu,
        host,
    )?;
    frame.meta.media_info = Some(Box::new(format_map::image_to_media_info(&image, host_len)?));
    // Rebuild plane layouts after potential tight repack so they match the buffer.
    if let Some(info) = frame.meta.media_info.as_mut() {
        if let dg_core::MediaPayloadInfo::Image(image_info) = &mut info.payload {
            if path_kind == TransferPathKind::RowRepack {
                image_info.planes = tight_plane_layouts_for_format(pixel, width, height)?;
            }
        }
    }
    normalize_media_frame_meta(&mut frame.meta)?;
    let mut transfer = staged_host_transfer_with_copies(
        avcodec_memory_domain_to_core(image.memory.domain()),
        if requires_csc {
            copy_count.saturating_add(1)
        } else {
            copy_count
        },
    );
    transfer.path_kind = if requires_csc {
        TransferPathKind::DomainStaging
    } else {
        path_kind
    };
    Ok(BridgedMediaFrame { frame, transfer })
}

/// Copies avcodec image host data, preserving padded strides when possible.
#[cfg(feature = "avcodec-sdk")]
fn materialize_avcodec_image_host_bytes(
    image: &dg_media_avcodec::Image,
) -> Result<(Vec<u8>, TransferPathKind, usize)> {
    // Prefer the whole host buffer when present (keeps original plane offsets/strides).
    if let Some(bytes) = image.memory.host_bytes() {
        if !bytes.is_empty() {
            return Ok((bytes.to_vec(), TransferPathKind::HostClone, 1));
        }
    }

    let pixel = format_map::avcodec_image_to_core_pixel(image.format);
    if !format_map::is_multiplane_pixel(pixel) && image.plane_count <= 1 {
        let host = if let Some(bytes) = image
            .plane_host_bytes(0)
            .map_err(|err| dg_core::Error::Buffer(format!("{err:?}")))?
        {
            bytes.to_vec()
        } else {
            let staged = image
                .memory
                .stage_to_host(0)
                .map_err(|err| dg_core::Error::Buffer(format!("{err:?}")))?;
            staged
                .host_bytes()
                .ok_or_else(|| {
                    dg_core::Error::Buffer(
                        "avcodec image staging did not produce host-readable bytes".to_string(),
                    )
                })?
                .to_vec()
        };
        return Ok((host, TransferPathKind::HostClone, 1));
    }

    // Multi-plane: row-wise tight copy so padded strides never leak as opaque blobs.
    let width = usize::try_from(image.coded_width)
        .map_err(|_| dg_core::Error::Media("image width overflow".to_string()))?;
    let height = usize::try_from(image.coded_height)
        .map_err(|_| dg_core::Error::Media("image height overflow".to_string()))?;
    let mut out = Vec::new();
    let plane_specs = plane_geometry(pixel, width, height)?;
    for (index, (rows, row_bytes)) in plane_specs.iter().enumerate() {
        let src = image
            .plane_host_bytes(index)
            .map_err(|err| dg_core::Error::Buffer(format!("{err:?}")))?
            .ok_or_else(|| {
                dg_core::Error::Buffer(format!(
                    "avcodec image plane {index} is not host-readable; domain mismatch"
                ))
            })?;
        let plane = image.planes[index].ok_or_else(|| {
            dg_core::Error::Media(format!("avcodec image missing plane descriptor {index}"))
        })?;
        if plane.stride < *row_bytes {
            return Err(dg_core::Error::Media(format!(
                "plane {index} stride {} is less than effective row bytes {row_bytes}",
                plane.stride
            )));
        }
        for row in 0..*rows {
            let start = row
                .checked_mul(plane.stride)
                .and_then(|o| o.checked_add(0))
                .ok_or_else(|| dg_core::Error::Media("plane row offset overflow".into()))?;
            let end = start
                .checked_add(*row_bytes)
                .ok_or_else(|| dg_core::Error::Media("plane row end overflow".into()))?;
            if end > src.len() {
                return Err(dg_core::Error::Media(format!(
                    "plane {index} row {row} exceeds host bytes"
                )));
            }
            out.extend_from_slice(&src[start..end]);
        }
    }
    Ok((out, TransferPathKind::RowRepack, 1))
}

#[cfg(feature = "avcodec-sdk")]
fn plane_geometry(
    format: dg_core::PixelFormat,
    width: usize,
    height: usize,
) -> Result<Vec<(usize, usize)>> {
    match format {
        dg_core::PixelFormat::Yuv420P => {
            let cw = width.div_ceil(2);
            let ch = height.div_ceil(2);
            Ok(vec![(height, width), (ch, cw), (ch, cw)])
        }
        dg_core::PixelFormat::Yuv422P => {
            let cw = width.div_ceil(2);
            Ok(vec![(height, width), (height, cw), (height, cw)])
        }
        dg_core::PixelFormat::Yuv444P => {
            Ok(vec![(height, width), (height, width), (height, width)])
        }
        dg_core::PixelFormat::Nv12 | dg_core::PixelFormat::Nv21 => {
            let ch = height.div_ceil(2);
            Ok(vec![(height, width), (ch, width)])
        }
        dg_core::PixelFormat::Gray8 => Ok(vec![(height, width)]),
        dg_core::PixelFormat::Rgb24 | dg_core::PixelFormat::Bgr24 => {
            Ok(vec![(height, width.saturating_mul(3))])
        }
        dg_core::PixelFormat::Rgba | dg_core::PixelFormat::Bgra => {
            Ok(vec![(height, width.saturating_mul(4))])
        }
        other => Err(dg_core::Error::Unsupported(format!(
            "no host plane geometry for pixel format {other:?}"
        ))),
    }
}

#[cfg(feature = "avcodec-sdk")]
fn tight_plane_layouts_for_format(
    format: dg_core::PixelFormat,
    width: usize,
    height: usize,
) -> Result<Vec<dg_core::MediaPlaneLayout>> {
    let specs = plane_geometry(format, width, height)?;
    let mut offset = 0usize;
    let mut planes = Vec::with_capacity(specs.len());
    for (rows, row_bytes) in specs {
        let len = rows
            .checked_mul(row_bytes)
            .ok_or_else(|| dg_core::Error::Media("plane length overflow".into()))?;
        planes.push(dg_core::MediaPlaneLayout {
            offset,
            stride: row_bytes,
            len,
        });
        offset = offset
            .checked_add(len)
            .ok_or_else(|| dg_core::Error::Media("plane offset overflow".into()))?;
    }
    Ok(planes)
}

#[cfg(feature = "avcodec-sdk")]
pub fn media_frame_to_avcodec_image(
    frame: MediaFrame,
    stride_alignment: usize,
) -> Result<dg_media_avcodec::Image> {
    Ok(media_frame_to_avcodec_image_with_transfer(frame, stride_alignment)?.0)
}

#[cfg(feature = "avcodec-sdk")]
pub fn media_frame_to_avcodec_image_with_transfer(
    frame: MediaFrame,
    stride_alignment: usize,
) -> Result<(dg_media_avcodec::Image, TransferReport)> {
    let mut meta = frame.meta.clone();
    normalize_media_frame_meta(&mut meta)?;
    let timing = media_frame_timing(&meta);
    let pts = timing.pts;
    let dts = timing.dts;
    let source_domain = frame.domain;

    if !frame.buffer.is_host_readable() {
        return Err(dg_core::Error::Buffer(
            "media frame buffer is not host-readable for host image bridge".to_string(),
        ));
    }

    // Prefer authoritative ImageMediaInfo (planar / semi-planar / packed).
    if let Some(info) = meta.media_info.as_ref() {
        if let dg_core::MediaPayloadInfo::Image(image_info) = &info.payload {
            return media_frame_to_avcodec_image_from_layout(
                frame,
                image_info,
                pts,
                dts,
                source_domain,
                stride_alignment,
            );
        }
    }

    let height = *frame
        .shape
        .first()
        .ok_or_else(|| dg_core::Error::Media("image height is missing".to_string()))?;
    let width = *frame
        .shape
        .get(1)
        .ok_or_else(|| dg_core::Error::Media("image width is missing".to_string()))?;
    let coded_width = u32::try_from(width)
        .map_err(|_| dg_core::Error::Media("image width overflow".to_string()))?;
    let coded_height = u32::try_from(height)
        .map_err(|_| dg_core::Error::Media("image height overflow".to_string()))?;
    let channels = *frame
        .shape
        .get(2)
        .or_else(|| frame.shape.last())
        .ok_or_else(|| dg_core::Error::Media("image channels are missing".to_string()))?;
    if frame.shape.len() == 2 {
        return Err(dg_core::Error::Media(
            "planar image frames must carry ImageMediaInfo plane layout in media_info".into(),
        ));
    }
    let format = match channels {
        3 => dg_media_avcodec::ImageInfo::Rgb24,
        4 => dg_media_avcodec::ImageInfo::Rgba,
        1 => dg_media_avcodec::ImageInfo::Gray8,
        other => {
            return Err(dg_core::Error::Media(format!(
                "unsupported image channel count {other}"
            )))
        }
    };
    let bytes_per_pixel = match format {
        dg_media_avcodec::ImageInfo::Rgb24 => 3,
        dg_media_avcodec::ImageInfo::Rgba => 4,
        _ => 1,
    };
    let stride = width
        .checked_mul(bytes_per_pixel)
        .ok_or_else(|| dg_core::Error::Media("image stride overflow".to_string()))?;
    let bytes = frame.buffer.try_into_host_bytes()?;
    let mut image = dg_media_avcodec::Image::new_host_packed(
        format,
        coded_width,
        coded_height,
        0,
        stride,
        bytes,
        stride_alignment,
    )
    .map_err(|err| dg_core::Error::Buffer(format!("{err:?}")))?;
    image.pts = pts;
    image.dts = dts;
    Ok((image, staged_host_transfer(source_domain)))
}

#[cfg(feature = "avcodec-sdk")]
fn media_frame_to_avcodec_image_from_layout(
    frame: MediaFrame,
    image_info: &dg_core::ImageMediaInfo,
    pts: Option<i64>,
    dts: Option<i64>,
    source_domain: MemoryDomain,
    stride_alignment: usize,
) -> Result<(dg_media_avcodec::Image, TransferReport)> {
    let format = format_map::core_pixel_to_avcodec(image_info.pixel_format)?;
    let bytes = frame.buffer.try_into_host_bytes()?;
    image_info
        .validate(bytes.len())
        .map_err(|err| dg_core::Error::Media(err.to_string()))?;

    if format_map::is_multiplane_pixel(image_info.pixel_format) {
        if image_info.pixel_format == dg_core::PixelFormat::Yuv420P && image_info.planes.len() == 3
        {
            let y = &image_info.planes[0];
            let u = &image_info.planes[1];
            let v = &image_info.planes[2];
            let y_end = y.end_offset()?;
            let u_end = u.end_offset()?;
            let v_end = v.end_offset()?;
            let mut image = dg_media_avcodec::Image::from_host_i420(
                image_info.coded_width,
                image_info.coded_height,
                &bytes[y.offset..y_end],
                y.stride,
                &bytes[u.offset..u_end],
                u.stride,
                &bytes[v.offset..v_end],
                v.stride,
            )
            .map_err(|err| dg_core::Error::Buffer(format!("from_host_i420: {err:?}")))?;
            image.pts = pts;
            image.dts = dts;
            return Ok((
                image,
                TransferReport {
                    source_domain,
                    target_domain: MemoryDomain::Host,
                    path: CopyPath {
                        domains: vec![source_domain, MemoryDomain::Host],
                        copy_count: 1,
                    },
                    copy_count: 1,
                    mode: TransferMode::Staged,
                    path_kind: TransferPathKind::HostClone,
                    reason: None,
                },
            ));
        }

        let handle = dg_media_avcodec::BufferHandle::from_host_bytes(0, bytes);
        let planes: Vec<dg_media_avcodec::ImagePlane> = image_info
            .planes
            .iter()
            .map(|p| dg_media_avcodec::ImagePlane {
                offset: p.offset,
                stride: p.stride,
                len: p.len,
            })
            .collect();
        let mut image = dg_media_avcodec::Image::new_with_planes(
            format,
            image_info.coded_width,
            image_info.coded_height,
            handle,
            &planes,
            stride_alignment,
        )
        .map_err(|err| dg_core::Error::Buffer(format!("new_with_planes: {err:?}")))?;
        image.pts = pts;
        image.dts = dts;
        return Ok((image, staged_host_transfer(source_domain)));
    }

    // Packed formats with explicit layout.
    let stride = image_info
        .planes
        .first()
        .map(|p| p.stride)
        .ok_or_else(|| dg_core::Error::Media("packed image missing plane layout".into()))?;
    let mut image = dg_media_avcodec::Image::new_host_packed(
        format,
        image_info.coded_width,
        image_info.coded_height,
        0,
        stride,
        bytes,
        stride_alignment,
    )
    .map_err(|err| dg_core::Error::Buffer(format!("{err:?}")))?;
    image.pts = pts;
    image.dts = dts;
    Ok((image, staged_host_transfer(source_domain)))
}

#[cfg(feature = "avcodec-sdk")]
pub fn avcodec_memory_domain_to_core(value: dg_media_avcodec::MemoryDomain) -> MemoryDomain {
    match value {
        dg_media_avcodec::MemoryDomain::Host => MemoryDomain::Host,
        dg_media_avcodec::MemoryDomain::DmaBuf => MemoryDomain::DmaBuf,
        dg_media_avcodec::MemoryDomain::DrmPrime => MemoryDomain::DrmPrime,
        dg_media_avcodec::MemoryDomain::VaapiSurface => MemoryDomain::VaapiSurface,
        dg_media_avcodec::MemoryDomain::CudaDevice => MemoryDomain::CudaDevice,
        dg_media_avcodec::MemoryDomain::MppBuffer => MemoryDomain::MppBuffer,
        dg_media_avcodec::MemoryDomain::OpaqueBackend => MemoryDomain::Opaque,
    }
}

#[cfg(feature = "avcodec-sdk")]
pub fn core_memory_domain_to_avcodec(value: MemoryDomain) -> dg_media_avcodec::MemoryDomain {
    match value {
        MemoryDomain::Host => dg_media_avcodec::MemoryDomain::Host,
        MemoryDomain::DmaBuf => dg_media_avcodec::MemoryDomain::DmaBuf,
        MemoryDomain::DrmPrime => dg_media_avcodec::MemoryDomain::DrmPrime,
        MemoryDomain::VaapiSurface => dg_media_avcodec::MemoryDomain::VaapiSurface,
        MemoryDomain::CudaDevice => dg_media_avcodec::MemoryDomain::CudaDevice,
        MemoryDomain::MppBuffer => dg_media_avcodec::MemoryDomain::MppBuffer,
        MemoryDomain::SophonDevice | MemoryDomain::Opaque => {
            dg_media_avcodec::MemoryDomain::OpaqueBackend
        }
    }
}

#[cfg(feature = "avcodec-sdk")]
pub fn avcodec_external_handle_to_core(
    value: dg_media_avcodec::ExternalHandle,
) -> crate::ExternalHandle {
    crate::ExternalHandle {
        fd: value.fd,
        raw: value.raw,
    }
}

#[cfg(feature = "avcodec-sdk")]
pub fn core_external_handle_to_avcodec(
    value: crate::ExternalHandle,
) -> dg_media_avcodec::ExternalHandle {
    dg_media_avcodec::ExternalHandle {
        fd: value.fd,
        raw: value.raw,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use dg_core::{CpuDevice, DataFormat, DataType, DeviceKind, Shape, Tensor, TensorDesc};
    use dg_graph::{Packet, PacketMeta};

    use super::graph_packet_to_media_frame;

    fn test_tensor() -> Tensor {
        let device = CpuDevice::new();
        let desc = TensorDesc::new(
            Shape::new([1, 4]),
            DataType::U8,
            DataFormat::NC,
            DeviceKind::Cpu,
        );
        let tensor = Tensor::allocate(&device, desc).expect("allocate test tensor");
        tensor
            .buffer()
            .write_from_slice(&[4, 3, 2, 1])
            .expect("write test tensor");
        tensor
    }

    #[cfg(feature = "avcodec-sdk")]
    mod avcodec {
        use dg_core::{DeviceKind, MemoryDomain};

        use crate::bridge::import_avcodec_handle;

        #[test]
        fn same_domain_device_handle_is_shared_without_copy() {
            let handle = dg_media_avcodec::BufferHandle::new(
                7,
                dg_media_avcodec::MemoryDomain::MppBuffer,
                16,
            );
            let imported =
                import_avcodec_handle(&handle, DeviceKind::RknnNpu, MemoryDomain::MppBuffer)
                    .expect("share device handle");
            assert!(imported.zero_copy);
            assert_eq!(imported.path.copy_count, 0);
            assert_eq!(imported.path.domains, vec![MemoryDomain::MppBuffer]);
            assert_eq!(imported.buffer.domain(), MemoryDomain::MppBuffer);
            assert_eq!(imported.buffer.len(), 16);

            // The imported buffer outlives the original avcodec handle.
            drop(handle);
            let clone = imported.buffer.clone();
            drop(imported);
            assert_eq!(clone.len(), 16);
        }

        #[test]
        fn host_handle_imports_host_bytes_without_staging() {
            let handle = dg_media_avcodec::BufferHandle::from_host_bytes(3, vec![1, 2, 3, 4]);
            let imported = import_avcodec_handle(&handle, DeviceKind::Cpu, MemoryDomain::Host)
                .expect("import host handle");
            assert!(imported.zero_copy);
            assert_eq!(imported.path.copy_count, 0);
            assert_eq!(imported.buffer.read_bytes(), vec![1, 2, 3, 4]);
        }

        #[test]
        fn missing_staging_path_fails_explicitly() {
            let handle = dg_media_avcodec::BufferHandle::new(
                9,
                dg_media_avcodec::MemoryDomain::MppBuffer,
                8,
            );
            let err = import_avcodec_handle(&handle, DeviceKind::Cpu, MemoryDomain::Host)
                .expect_err("expected unsupported staging path");
            assert!(matches!(err, dg_core::Error::Unsupported(message)
                if message.contains("MppBuffer") && message.contains("Host")));
        }
    }

    #[test]
    fn external_buffer_releases_ownership_once_after_last_clone() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let released = std::sync::Arc::new(AtomicUsize::new(0));
        let flag = released.clone();
        let guard = dg_core::ExternalDropGuard::new(move || {
            flag.fetch_add(1, Ordering::SeqCst);
        });
        let buffer = dg_core::Buffer::from_external_with_host_bytes(
            DeviceKind::Cpu,
            dg_core::MemoryDomain::DmaBuf,
            dg_core::BufferDesc::new(4, 1),
            dg_core::ExternalHandle::from_raw(42),
            vec![0; 4],
            guard,
        )
        .expect("import external buffer");

        let clone = buffer.clone();
        drop(buffer);
        assert_eq!(released.load(Ordering::SeqCst), 0);
        assert_eq!(clone.external().raw, 42);
        drop(clone);
        assert_eq!(released.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn graph_packet_bridge_preserves_shared_tensor_and_metadata() {
        use dg_core::{
            EncodedMediaInfo, EncodedPacketFlags, MediaCodec, MediaInfo, MediaKind, MediaTimeBase,
            MediaTiming,
        };

        let timing = MediaTiming {
            pts: Some(17),
            dts: Some(16),
            time_base: Some(MediaTimeBase::new(1, 90_000)),
        };
        let media_info = MediaInfo::encoded(
            EncodedMediaInfo {
                stream_index: 2,
                track_id: Some(7),
                media_kind: MediaKind::Video,
                codec: MediaCodec::H264,
                bitstream_format: dg_core::BitstreamFormat::H264AnnexB,
                flags: EncodedPacketFlags::default(),
                codec_configs: Vec::new(),
            },
            timing,
        )
        .expect("media info");
        let packet = Packet {
            meta: PacketMeta {
                sequence: 99,
                stream_id: Some("stream-a".to_string()),
                tags: BTreeMap::from([("kind".to_string(), "tensor".to_string())]),
                media_info: Some(Box::new(media_info)),
            },
            payload: std::sync::Arc::new(dg_graph::PacketPayload::Tensor(test_tensor())),
        };
        let cloned_packet = packet.clone();

        let frame = graph_packet_to_media_frame(cloned_packet).expect("bridge");

        assert!(!frame.is_end_of_stream());
        assert_eq!(frame.buffer.read_bytes(), vec![4, 3, 2, 1]);
        assert_eq!(frame.meta.pts, Some(17));
        assert_eq!(
            frame
                .meta
                .media_info
                .as_ref()
                .and_then(|info| info.timing.time_base)
                .map(|tb| tb.den),
            Some(90_000)
        );
        assert_eq!(frame.meta.stream_id.as_deref(), Some("stream-a"));
        assert_eq!(
            frame.meta.tags.get("kind").map(String::as_str),
            Some("tensor")
        );
    }

    #[cfg(feature = "avcodec-sdk")]
    #[test]
    fn packet_bridge_preserves_stream_index_and_flags() {
        use dg_core::{
            EncodedMediaInfo, EncodedPacketFlags, MediaCodec, MediaInfo, MediaKind, MediaTimeBase,
            MediaTiming,
        };

        use super::{avcodec_packet_to_media_frame, media_frame_to_avcodec_packet_with_transfer};
        use crate::{MediaFrame, MediaFrameKind};

        let timing = MediaTiming {
            pts: Some(-10),
            dts: Some(-20),
            time_base: Some(MediaTimeBase::new(1, 90_000)),
        };
        let media_info = MediaInfo::encoded(
            EncodedMediaInfo {
                stream_index: 7,
                track_id: Some(7),
                media_kind: MediaKind::Video,
                codec: MediaCodec::H264,
                bitstream_format: dg_core::BitstreamFormat::H264AnnexB,
                flags: EncodedPacketFlags {
                    key: true,
                    lost: false,
                    corrupt: true,
                },
                codec_configs: Vec::new(),
            },
            timing,
        )
        .expect("info");
        let mut frame = MediaFrame::from_host_bytes(
            MediaFrameKind::Tensor,
            DataType::U8,
            DataFormat::N,
            vec![4],
            DeviceKind::Cpu,
            vec![0, 0, 0, 1],
        )
        .expect("frame");
        frame.meta.media_info = Some(Box::new(media_info));

        let (packet, _transfer) = media_frame_to_avcodec_packet_with_transfer(
            frame,
            0,
            dg_media_avcodec::CodecId::H264,
            dg_media_avcodec::BitstreamFormat::H264AnnexB,
        )
        .expect("to packet");
        assert_eq!(packet.stream_index, 7);
        assert_eq!(packet.pts, Some(-10));
        assert_eq!(packet.dts, Some(-20));
        assert!(packet.flags.contains(dg_media_avcodec::PacketFlags::KEY));
        assert!(packet
            .flags
            .contains(dg_media_avcodec::PacketFlags::CORRUPT));

        let back = avcodec_packet_to_media_frame(&packet).expect("from packet");
        let encoded = match back.meta.media_info.as_ref().map(|i| &i.payload) {
            Some(dg_core::MediaPayloadInfo::Encoded(e)) => e,
            _ => panic!("expected encoded media_info"),
        };
        assert_eq!(encoded.stream_index, 7);
        assert!(encoded.flags.key);
        assert!(encoded.flags.corrupt);
        assert_eq!(back.meta.pts, Some(-10));
    }

    #[cfg(feature = "avcodec-sdk")]
    #[test]
    fn yuv420p_image_roundtrip_preserves_plane_layout() {
        use dg_core::{
            ImageMediaInfo, MediaInfo, MediaPayloadInfo, MediaPlaneLayout, MediaRect, MediaTiming,
            PixelFormat, SampleLayout, SampleType,
        };

        use super::{avcodec_image_to_media_frame, media_frame_to_avcodec_image_with_transfer};
        use crate::{MediaFrame, MediaFrameKind};

        let width = 4usize;
        let height = 4usize;
        let y_len = width * height;
        let c_len = (width / 2) * (height / 2);
        let mut bytes = vec![16u8; y_len];
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
                stride: width / 2,
                len: c_len,
            },
            MediaPlaneLayout {
                offset: y_len + c_len,
                stride: width / 2,
                len: c_len,
            },
        ];
        let image_info = ImageMediaInfo {
            pixel_format: PixelFormat::Yuv420P,
            coded_width: 4,
            coded_height: 4,
            visible_rect: MediaRect {
                x: 0,
                y: 0,
                width: 4,
                height: 4,
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
        let media_info = MediaInfo::image(
            image_info,
            MediaTiming {
                pts: Some(3),
                dts: Some(3),
                time_base: None,
            },
            bytes.len(),
        )
        .expect("image info");
        let mut frame = MediaFrame::from_host_bytes(
            MediaFrameKind::Image,
            DataType::U8,
            DataFormat::Auto,
            vec![height, width],
            DeviceKind::Cpu,
            bytes.clone(),
        )
        .expect("frame");
        frame.meta.media_info = Some(Box::new(media_info));

        let (image, transfer) =
            media_frame_to_avcodec_image_with_transfer(frame, 1).expect("to image");
        assert_eq!(image.format, dg_media_avcodec::ImageInfo::Yuv420p);
        assert_eq!(image.coded_width, 4);
        assert_eq!(image.coded_height, 4);
        assert_eq!(image.pts, Some(3));
        assert!(transfer.copy_count >= 1);

        let back = avcodec_image_to_media_frame(&image).expect("from image");
        assert_eq!(back.meta.pts, Some(3));
        let payload = back.meta.media_info.as_ref().expect("media_info");
        match &payload.payload {
            MediaPayloadInfo::Image(info) => {
                assert_eq!(info.pixel_format, PixelFormat::Yuv420P);
                assert_eq!(info.planes.len(), 3);
                assert_eq!(info.coded_width, 4);
            }
            _ => panic!("expected image payload"),
        }
        // Shape must not pretend to be packed RGB NHWC.
        assert_eq!(back.shape.len(), 2);
    }
}
