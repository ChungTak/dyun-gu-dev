#![deny(unsafe_code)]

mod external;

pub use external::{probe_external_export, try_import_external_image, ExternalExport};

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

#[cfg(feature = "avcodec-sdk")]
pub use avcodec::core::{
    AvError, AvErrorContext, AvErrorDetail, AvErrorKind, AvOperation, AvResult,
    BackendSelectionPolicy, BitstreamFormat, BufferHandle, BufferSlice, CodecId, CodecParameters,
    Decoder, DecoderConfig, Encoder, EncoderConfig, ExternalBufferDescriptor, ExternalHandle,
    ExternalImageDescriptor, ExternalPacketDescriptor, Image, ImageInfo, ImageOp, ImageOpKind,
    ImagePlane, ImageProcessRequest, ImageProcessor, ImageProcessorConfig, MemoryDomain, Packet,
    PacketFlags, Poll, ProfileName, Registry, SelectionFailureDiagnosis, SelectionFailureReport,
    SelectionTrace, TimeBase,
};
