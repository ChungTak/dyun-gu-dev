use std::collections::BTreeMap;

use crate::stream_metadata::MediaStreamMetadata;
use dg_core::{
    Buffer, BufferDesc, DataFormat, DataType, DeviceKind, MediaInfo, MemoryDomain, Result, Shape,
    Tensor, TensorDesc,
};

/// Media-side frame kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaFrameKind {
    Tensor,
    Image,
    EndOfStream,
}

/// Metadata shared by media frames.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MediaFrameMeta {
    /// Deprecated top-level PTS; synchronized from [`Self::media_info`] when present.
    pub pts: Option<i64>,
    /// Deprecated top-level DTS; synchronized from [`Self::media_info`] when present.
    pub dts: Option<i64>,
    pub stream_id: Option<String>,
    pub tags: BTreeMap<String, String>,
    /// Authoritative encoded/image metadata for graph and backend transport.
    pub media_info: Option<Box<MediaInfo>>,
    /// Deprecated per-frame stream metadata; use [`Self::media_info`] for new producers.
    pub stream_metadata: Option<MediaStreamMetadata>,
}

/// Framework-native media frame envelope.
#[derive(Clone, Debug)]
pub struct MediaFrame {
    pub kind: MediaFrameKind,
    pub dtype: DataType,
    pub format: DataFormat,
    pub shape: Vec<usize>,
    pub device: DeviceKind,
    pub domain: MemoryDomain,
    pub buffer: Buffer,
    pub meta: MediaFrameMeta,
}

impl MediaFrame {
    pub fn new(
        kind: MediaFrameKind,
        dtype: DataType,
        format: DataFormat,
        shape: Vec<usize>,
        device: DeviceKind,
        domain: MemoryDomain,
        buffer: Buffer,
    ) -> Self {
        Self::with_meta(
            kind,
            dtype,
            format,
            shape,
            device,
            domain,
            buffer,
            MediaFrameMeta::default(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_meta(
        kind: MediaFrameKind,
        dtype: DataType,
        format: DataFormat,
        shape: Vec<usize>,
        device: DeviceKind,
        domain: MemoryDomain,
        buffer: Buffer,
        meta: MediaFrameMeta,
    ) -> Self {
        Self {
            kind,
            dtype,
            format,
            shape,
            device,
            domain,
            buffer,
            meta,
        }
    }

    pub fn from_host_bytes(
        kind: MediaFrameKind,
        dtype: DataType,
        format: DataFormat,
        shape: Vec<usize>,
        device: DeviceKind,
        bytes: Vec<u8>,
    ) -> Result<Self> {
        let buffer = Buffer::from_host_bytes(device, BufferDesc::new(bytes.len(), 1), bytes)?;
        Ok(Self::new(
            kind,
            dtype,
            format,
            shape,
            device,
            MemoryDomain::Host,
            buffer,
        ))
    }

    pub fn from_tensor(tensor: Tensor) -> Self {
        let (desc, buffer) = tensor.into_parts();
        Self {
            kind: MediaFrameKind::Tensor,
            dtype: desc.dtype(),
            format: desc.format(),
            shape: desc.shape().dims().to_vec(),
            device: desc.device(),
            domain: buffer.domain(),
            buffer,
            meta: MediaFrameMeta::default(),
        }
    }

    pub fn into_tensor(self) -> Result<Tensor> {
        let desc = TensorDesc::new(Shape::new(self.shape), self.dtype, self.format, self.device);
        Tensor::from_buffer(desc, self.buffer)
    }

    pub fn is_end_of_stream(&self) -> bool {
        self.kind == MediaFrameKind::EndOfStream
    }
}

impl From<Tensor> for MediaFrame {
    fn from(value: Tensor) -> Self {
        Self::from_tensor(value)
    }
}

impl TryFrom<MediaFrame> for Tensor {
    type Error = dg_core::Error;

    fn try_from(value: MediaFrame) -> core::result::Result<Self, Self::Error> {
        value.into_tensor()
    }
}
