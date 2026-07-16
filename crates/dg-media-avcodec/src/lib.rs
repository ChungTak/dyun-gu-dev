//! Curated avcodec facade for dyun-gu-dev.
//!
//! Production code must only depend on this crate for codec integration (not backend/codec
//! crates). Profile Cargo features forward upstream `profile-*` presets.
//!
//! `unsafe` is denied by default and only allowed inside [`external`] for thin wrappers around
//! avcodec `from_external_descriptor` (media FFI adapter boundary).

#![deny(unsafe_code)]

#[allow(unsafe_code)]
mod external;

pub use external::{probe_external_export, ExternalExport};

#[allow(deprecated)]
pub use external::try_import_external_image;

#[cfg(feature = "avcodec-sdk")]
pub use external::{
    host_packed_image_descriptor, import_external_image, import_external_packet,
    import_host_image_gray8, import_host_image_rgb24, import_host_packet, HostPacketMeta,
};

// High-level V3 VideoSdk facade (owned sessions + requests + reports).
#[cfg(feature = "avcodec-sdk")]
pub use avcodec::{
    CreatedDecoder, CreatedEncoder, CreatedImageProcessor, CreatedTranscoder,
    ImageProcessorRequest, OwnedVideoBuildReport, VideoBuildError, VideoBuildStage,
    VideoDecoderRequest, VideoDecoderSession, VideoEncoderRequest, VideoEncoderSession,
    VideoImageProcessorSession, VideoIntent, VideoProcessingSpec, VideoProfile, VideoRole,
    VideoRuntimeDiagnostics, VideoRuntimeError, VideoRuntimeRole, VideoSdk, VideoTranscodeOptions,
    VideoTranscodeRequest, VideoTranscoderSession,
};

#[cfg(feature = "avcodec-sdk")]
pub use avcodec::{VideoTranscodeModeReport, VideoTranscoderBuildReport};

// Core media types required by dg-media bridges (Packet/Image/buffer domains).
// Intentionally does NOT re-export low-level assembly types (Registry, DecoderConfig,
// EncoderConfig, BackendSelectionPolicy, raw Decoder/Encoder/ImageProcessor traits).
#[cfg(feature = "avcodec-sdk")]
pub use avcodec::core::{
    AvError, AvErrorContext, AvErrorDetail, AvErrorKind, BitstreamFormat, BufferHandle,
    BufferSlice, CodecId, CodecParameters, ExternalBufferDescriptor, ExternalDropGuard,
    ExternalHandle, ExternalImageDescriptor, ExternalPacketDescriptor, ExternalPlaneDescriptor,
    Image, ImageInfo, ImageOp, ImagePlane, ImageProcessRequest, MemoryDomain, Packet, PacketFlags,
    Poll, TimeBase,
};
