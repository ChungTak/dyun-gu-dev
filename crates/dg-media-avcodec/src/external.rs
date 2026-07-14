//! External memory import facade (UP-04 gated).
//!
//! Unsafe code is confined to this module when full external packet/image
//! constructors are wired to upstream Profile V2.

/// Result of attempting to export a buffer as an avcodec external handle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalExport {
    pub supported: bool,
    pub message: String,
}

/// Probes whether external buffer export is available for the active profile.
pub fn probe_external_export() -> ExternalExport {
    ExternalExport {
        supported: true,
        message: "ExternalPacketDescriptor and ExternalImageDescriptor are available via \
                  avcodec::core; dyun host bridge uses owned buffers unless an external handle \
                  is explicitly imported"
            .into(),
    }
}

/// Safe entry for external image construction through the curated facade.
pub fn try_import_external_image() -> Result<(), String> {
    Err(
        "external image import is not wired in dyun bridge yet; use host buffers or extend \
         bridge::import_avcodec_handle for device handles"
            .to_string(),
    )
}
