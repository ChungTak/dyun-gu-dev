use std::fmt;

use thiserror::Error;

/// Result type used by stream abstractions.
pub type Result<T> = core::result::Result<T, Error>;

/// Direction of a stream endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointClass {
    Pull,
    Push,
}

impl fmt::Display for EndpointClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pull => write!(f, "pull"),
            Self::Push => write!(f, "push"),
        }
    }
}

/// Errors surfaced by stream adapters and in-memory tests.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum Error {
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("stream closed")]
    Closed,
    #[error("end of stream")]
    EndOfStream,
    #[error("backpressure overflow: {0}")]
    Overflow(String),
    #[error("buffer error: {0}")]
    Buffer(String),
    #[error("media error: {0}")]
    Media(String),
    #[error("sdk error: {0}")]
    Sdk(String),
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("connector {operation} failed for {class} {protocol}: {message}")]
    Connector {
        protocol: &'static str,
        operation: &'static str,
        retryable: bool,
        class: EndpointClass,
        status: Option<String>,
        message: String,
    },
    #[error("stream connector already installed")]
    AlreadyInstalled,
    #[error("configuration conflict: {0}")]
    Conflict(String),
}

impl Error {
    /// Returns `true` if the error is transient and the stream operation may be retried.
    pub fn retryable(&self) -> bool {
        match self {
            Self::Connector { retryable, .. } => *retryable,
            Self::Closed => false,
            Self::EndOfStream => false,
            Self::Overflow(_) => false,
            Self::InvalidArgument(_) => false,
            Self::Conflict(_) => false,
            Self::AlreadyInstalled => false,
            Self::Runtime(_) | Self::Sdk(_) | Self::Media(_) | Self::Buffer(_) => false,
        }
    }

    /// Returns the endpoint class associated with the error, if any.
    pub fn endpoint_class(&self) -> Option<EndpointClass> {
        match self {
            Self::Connector { class, .. } => Some(*class),
            _ => None,
        }
    }

    /// Returns the protocol label associated with the error, if any.
    pub fn protocol_label(&self) -> Option<&'static str> {
        match self {
            Self::Connector { protocol, .. } if *protocol != "unknown" => Some(*protocol),
            _ => None,
        }
    }

    /// Returns a redacted display string safe for logs and metrics labels.
    pub fn redacted(&self) -> String {
        match self {
            Self::Connector {
                protocol,
                operation,
                class,
                message,
                status,
                ..
            } => {
                let redacted_message = redact_url_tokens(message);
                let status = status.as_deref().unwrap_or("-");
                format!(
                    "connector {operation} failed for {class} {protocol}: {redacted_message} \
                     (status={status})"
                )
            }
            other => other.to_string(),
        }
    }
}

impl From<dg_core::Error> for Error {
    fn from(value: dg_core::Error) -> Self {
        Self::Buffer(value.to_string())
    }
}

/// Removes userinfo and query/fragment tokens from a URL-like string.
///
/// The scheme and host/path are preserved so operators can identify the
/// endpoint without leaking credentials or signed tokens.
pub fn redact_url(url: &str) -> String {
    redact_url_tokens(url)
}

fn redact_url_tokens(url: &str) -> String {
    // Find the scheme separator and, if present, the authority boundary.
    let Some((scheme, after_scheme)) = url.split_once("://") else {
        // Not a URL; redact query/fragment only.
        return strip_query_fragment(url);
    };
    let scheme_lower = scheme.to_lowercase();
    if scheme_lower != "rtsp"
        && scheme_lower != "rtsps"
        && scheme_lower != "rtmp"
        && scheme_lower != "rtmps"
        && scheme_lower != "http"
        && scheme_lower != "https"
        && scheme_lower != "webrtc"
        && scheme_lower != "whip"
        && scheme_lower != "mock"
    {
        return strip_query_fragment(url);
    }

    // The authority runs until the first '/' (or end of string). It may contain
    // userinfo before an '@'.
    let (authority, path_query) = match after_scheme.split_once('/') {
        Some((auth, rest)) => (auth, Some(rest)),
        None => (after_scheme, None),
    };

    let host = if let Some((_, after_at)) = authority.split_once('@') {
        after_at
    } else {
        authority
    };

    let mut out = format!("{scheme}://{host}");
    if let Some(path) = path_query {
        // Strip any query or fragment from the path portion.
        let path_only = strip_query_fragment(path);
        if !path_only.is_empty() {
            out.push('/');
            out.push_str(&path_only);
        }
    }
    out
}

fn strip_query_fragment(s: &str) -> String {
    let s = match s.split_once('?') {
        Some((left, _)) => left,
        None => s,
    };
    match s.split_once('#') {
        Some((left, _)) => left.to_string(),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_url_strips_credentials_and_query() {
        assert_eq!(
            redact_url("rtsp://user:pass@host:554/path?token=abc#frag"),
            "rtsp://host:554/path"
        );
    }

    #[test]
    fn redact_url_handles_no_userinfo() {
        assert_eq!(
            redact_url("http://example.com/stream?key=secret"),
            "http://example.com/stream"
        );
    }

    #[test]
    fn redact_url_leaves_mock_url_intact() {
        assert_eq!(redact_url("mock://input-a"), "mock://input-a");
    }
}
