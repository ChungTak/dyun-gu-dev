#![deny(unsafe_code)]

mod external;

pub use external::{probe_external_export, try_import_external_image, ExternalExport};

#[cfg(feature = "avcodec-sdk")]
pub use avcodec::core::*;

#[cfg(feature = "avcodec-sdk")]
pub use avcodec::default_registry_builder;
