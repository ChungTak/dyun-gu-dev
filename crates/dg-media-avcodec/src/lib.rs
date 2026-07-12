#![forbid(unsafe_code)]

#[cfg(feature = "avcodec")]
pub use avcodec::core::*;

#[cfg(feature = "avcodec")]
pub use avcodec::default_registry_builder;
