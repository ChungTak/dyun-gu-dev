#![deny(unsafe_code)]

mod external;

pub use external::{probe_external_export, try_import_external_image, ExternalExport};

#[cfg(feature = "avcodec-sdk")]
pub use avcodec::{
    default_registry_builder, VideoBackendPolicy, VideoMemoryPath, VideoSessionBuildError,
    VideoSessionBuildReport, VideoSessionBundle, VideoSessionFactoryV2, VideoSessionRequest,
    VideoSessionRole, VideoTranscodeModeReport, VideoTranscodeOptions, VideoTranscoder,
    VideoTranscoderBuildReport, VideoTranscoderRequest,
};

#[cfg(feature = "avcodec-sdk")]
pub use avcodec::core::{
    register_stage_to_host_hook, AvError, AvErrorContext, AvErrorDetail, AvErrorKind, AvOperation,
    AvResult, BackendSelectionPolicy, BitstreamFormat, BufferHandle, BufferSlice, CodecId,
    CodecParameters, Decoder, DecoderConfig, Encoder, EncoderConfig, ExternalBufferDescriptor,
    ExternalHandle, ExternalImageDescriptor, ExternalPacketDescriptor, Image, ImageInfo, ImageOp,
    ImageOpKind, ImagePlane, ImageProcessRequest, ImageProcessor, ImageProcessorConfig,
    MemoryDomain, Packet, PacketFlags, Poll, ProfileName, Registry, SelectionFailureDiagnosis,
    SelectionFailureReport, SelectionTrace, TimeBase, VideoIoMemoryPlan, VideoProfileDescriptor,
};
