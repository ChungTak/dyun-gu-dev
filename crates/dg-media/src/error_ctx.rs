//! Structured media error context for diagnostics.
#![forbid(unsafe_code)]

use dg_core::{Error, MemoryDomain};

/// High-level media operation that failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaOperation {
    CreateDecoder,
    CreateEncoder,
    CreateProcessor,
    Submit,
    Poll,
    Flush,
    Bridge,
    Config,
    Select,
}

/// Neutral snapshot of a media failure for logging and C API export.
///
/// Stable field order for C API / golden tests:
/// `kind= profile= role= operation= backend= domain= … detail`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaErrorContext {
    pub kind: &'static str,
    pub operation: MediaOperation,
    pub node: Option<String>,
    pub role: Option<String>,
    pub profile: Option<String>,
    pub backend: Option<String>,
    pub codec: Option<String>,
    pub bitstream_format: Option<String>,
    pub pixel_format: Option<String>,
    pub source_domain: Option<MemoryDomain>,
    pub target_domain: Option<MemoryDomain>,
    pub allow_staging: Option<bool>,
    pub detail: String,
}

impl MediaErrorContext {
    #[must_use]
    pub fn new(kind: &'static str, operation: MediaOperation, detail: impl Into<String>) -> Self {
        Self {
            kind,
            operation,
            node: None,
            role: None,
            profile: None,
            backend: None,
            codec: None,
            bitstream_format: None,
            pixel_format: None,
            source_domain: None,
            target_domain: None,
            allow_staging: None,
            detail: detail.into(),
        }
    }

    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    #[must_use]
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.role = Some(role.into());
        self
    }

    #[must_use]
    pub fn with_backend(mut self, backend: impl Into<String>) -> Self {
        self.backend = Some(backend.into());
        self
    }

    #[must_use]
    pub fn with_node(mut self, node: impl Into<String>) -> Self {
        self.node = Some(node.into());
        self
    }

    #[must_use]
    pub fn with_codec(mut self, codec: impl Into<String>) -> Self {
        self.codec = Some(codec.into());
        self
    }

    #[must_use]
    pub fn with_domains(
        mut self,
        source: Option<MemoryDomain>,
        target: Option<MemoryDomain>,
        allow_staging: Option<bool>,
    ) -> Self {
        self.source_domain = source;
        self.target_domain = target;
        self.allow_staging = allow_staging;
        self
    }

    /// Stable, field-oriented summary (not free-form prose).
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("kind={}", self.kind)];
        if let Some(profile) = &self.profile {
            parts.push(format!("profile={profile}"));
        }
        if let Some(role) = &self.role {
            parts.push(format!("role={role}"));
        }
        parts.push(format!("operation={:?}", self.operation));
        if let Some(backend) = &self.backend {
            parts.push(format!("backend={backend}"));
        }
        if let Some(domain) = self.source_domain {
            parts.push(format!("source_domain={domain:?}"));
        }
        if let Some(domain) = self.target_domain {
            parts.push(format!("target_domain={domain:?}"));
        }
        if let Some(allow) = self.allow_staging {
            parts.push(format!("allow_staging={allow}"));
        }
        if let Some(codec) = &self.codec {
            parts.push(format!("codec={codec}"));
        }
        if let Some(format) = &self.bitstream_format {
            parts.push(format!("bitstream_format={format}"));
        }
        if let Some(format) = &self.pixel_format {
            parts.push(format!("pixel_format={format}"));
        }
        if let Some(node) = &self.node {
            parts.push(format!("node={node}"));
        }
        parts.push(format!("detail={}", self.detail));
        parts.join(" ")
    }
}

pub fn media_error_with_context(context: MediaErrorContext) -> Error {
    Error::Media(context.summary())
}

#[cfg(test)]
mod tests {
    use super::{media_error_with_context, MediaErrorContext, MediaOperation};

    #[test]
    fn summary_has_stable_field_prefix() {
        let ctx = MediaErrorContext::new("Unsupported", MediaOperation::Select, "no backend")
            .with_profile("rkmpp-host")
            .with_role("decoder")
            .with_backend("none");
        let summary = ctx.summary();
        assert!(summary.starts_with("kind=Unsupported"));
        assert!(summary.contains("profile=rkmpp-host"));
        assert!(summary.contains("role=decoder"));
        assert!(summary.contains("operation=Select"));
        assert!(summary.contains("backend=none"));
        assert!(summary.contains("detail=no backend"));
        let err = media_error_with_context(ctx);
        assert!(err.to_string().contains("kind=Unsupported"));
    }
}
