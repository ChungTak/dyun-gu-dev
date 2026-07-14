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
        supported: false,
        message: "external packet/image export requires UP-04 ExternalPacketDescriptor".into(),
    }
}

/// Safe entry for external image construction; returns error until UP-04 lands.
pub fn try_import_external_image() -> Result<(), String> {
    Err("external image import requires UP-04 ExternalPacketDescriptor and Profile V2".to_string())
}
