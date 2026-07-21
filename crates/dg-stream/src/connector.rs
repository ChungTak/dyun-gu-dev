use crate::error::{redact_url, Error, Result};
use crate::hub::MemoryStreamHub;
use crate::stream::{PublisherOptions, PublisherSink, SubscriberOptions, SubscriberSource};
use crate::track::TrackInfo;

#[cfg(feature = "cheetah")]
use std::sync::OnceLock;

#[cfg(feature = "cheetah")]
use crate::bridge::{CheetahPublisherSinkAdapter, CheetahSubscriberSourceAdapter};

/// Stream protocol handled by a source or sink element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamProtocol {
    RtspPull,
    HttpFlvPull,
    RtmpPush,
    WebRtcPush,
}

impl StreamProtocol {
    pub const fn is_pull(self) -> bool {
        matches!(self, Self::RtspPull | Self::HttpFlvPull)
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::RtspPull => "rtsp",
            Self::HttpFlvPull => "http-flv",
            Self::RtmpPush => "rtmp",
            Self::WebRtcPush => "webrtc",
        }
    }

    const fn network_schemes(self) -> &'static [&'static str] {
        match self {
            Self::RtspPull => &["rtsp", "rtsps"],
            Self::HttpFlvPull => &["http", "https"],
            Self::RtmpPush => &["rtmp", "rtmps"],
            Self::WebRtcPush => &["webrtc", "whip"],
        }
    }
}

/// Opened pull endpoint: announced tracks plus the frame source.
pub struct PullEndpoint {
    pub tracks: Vec<TrackInfo>,
    pub source: Box<dyn SubscriberSource>,
}

fn scheme_of(url: &str) -> Result<&str> {
    match url.split_once("://") {
        Some((scheme, rest)) if !scheme.is_empty() && !rest.is_empty() => Ok(scheme),
        _ => Err(Error::InvalidArgument(format!(
            "url `{}` must be of the form scheme://path",
            redact_url(url)
        ))),
    }
}

pub(crate) fn validate_endpoint_url(protocol: StreamProtocol, url: &str) -> Result<()> {
    let scheme = scheme_of(url)?;
    if scheme == "mock" || protocol.network_schemes().contains(&scheme) {
        return Ok(());
    }
    Err(Error::InvalidArgument(format!(
        "scheme `{scheme}` is not supported by the {} protocol",
        protocol.label()
    )))
}

/// Opens a pull endpoint for `protocol` at `url`.
///
/// `mock://` URLs resolve to the in-process [`MemoryStreamHub`]; protocol
/// schemes (`rtsp://`, `http://`, ...) require the `cheetah` feature and an
/// installed runtime connector.
///
/// Uses default resource policy for pre-copy frame checks. Prefer
/// [`open_pull_with_policy`] on product paths that carry a process/effective policy.
pub fn open_pull(
    protocol: StreamProtocol,
    url: &str,
    options: SubscriberOptions,
) -> Result<PullEndpoint> {
    open_pull_with_policy(protocol, url, options, dg_core::ResourcePolicy::default())
}

/// Opens a pull endpoint and applies `frame_policy` before any host frame copy.
pub fn open_pull_with_policy(
    protocol: StreamProtocol,
    url: &str,
    options: SubscriberOptions,
    frame_policy: dg_core::ResourcePolicy,
) -> Result<PullEndpoint> {
    if !protocol.is_pull() {
        return Err(Error::InvalidArgument(format!(
            "{} is not a pull protocol",
            protocol.label()
        )));
    }
    validate_endpoint_url(protocol, url)?;
    let scheme = scheme_of(url)?;
    if scheme == "mock" {
        let hub = MemoryStreamHub::global();
        let tracks = hub.tracks(url);
        let source = hub.subscribe(url, options)?;
        return Ok(PullEndpoint {
            tracks,
            source: Box::new(source),
        });
    }
    if protocol.network_schemes().contains(&scheme) {
        return open_cheetah_pull(protocol, url, options, frame_policy);
    }
    Err(Error::InvalidArgument(format!(
        "scheme `{scheme}` is not supported by the {} protocol",
        protocol.label()
    )))
}

/// Opens a push endpoint for `protocol` at `url`. See [`open_pull`] for scheme rules.
pub fn open_push(
    protocol: StreamProtocol,
    url: &str,
    options: PublisherOptions,
) -> Result<Box<dyn PublisherSink>> {
    open_push_with_policy(protocol, url, options, dg_core::ResourcePolicy::default())
}

/// Opens a push endpoint and applies `frame_policy` before host payload materialization.
pub fn open_push_with_policy(
    protocol: StreamProtocol,
    url: &str,
    options: PublisherOptions,
    frame_policy: dg_core::ResourcePolicy,
) -> Result<Box<dyn PublisherSink>> {
    if protocol.is_pull() {
        return Err(Error::InvalidArgument(format!(
            "{} is not a push protocol",
            protocol.label()
        )));
    }
    validate_endpoint_url(protocol, url)?;
    let scheme = scheme_of(url)?;
    if scheme == "mock" {
        let sink = MemoryStreamHub::global().publish(url, options)?;
        return Ok(Box::new(sink));
    }
    if protocol.network_schemes().contains(&scheme) {
        return open_cheetah_push(protocol, url, options, frame_policy);
    }
    Err(Error::InvalidArgument(format!(
        "scheme `{scheme}` is not supported by the {} protocol",
        protocol.label()
    )))
}

/// Runtime connector bridging protocol endpoints onto the cheetah media server.
#[cfg(feature = "cheetah")]
pub trait CheetahRuntimeConnector: Send + Sync {
    fn open_pull(
        &self,
        protocol: StreamProtocol,
        url: &str,
        options: SubscriberOptions,
    ) -> Result<(Vec<TrackInfo>, Box<dyn dg_stream_cheetah::SubscriberSource>)>;

    fn open_push(
        &self,
        protocol: StreamProtocol,
        url: &str,
        options: PublisherOptions,
    ) -> Result<Box<dyn dg_stream_cheetah::PublisherSink>>;
}

#[cfg(feature = "cheetah")]
static CHEETAH_CONNECTOR: OnceLock<Box<dyn CheetahRuntimeConnector>> = OnceLock::new();

/// Installs the process-wide cheetah runtime connector.
///
/// Returns an error if a different connector is already installed. Calling this
/// repeatedly with the same connector source is treated as a configuration
/// conflict; callers should use [`install_embedded_cheetah_connector`] for the
/// built-in embedded runtime.
#[cfg(feature = "cheetah")]
pub fn install_cheetah_connector(connector: Box<dyn CheetahRuntimeConnector>) -> Result<()> {
    CHEETAH_CONNECTOR
        .set(connector)
        .map_err(|_| Error::AlreadyInstalled)
}

#[cfg(feature = "cheetah")]
fn ensure_cheetah_connector_installed() -> Result<()> {
    if CHEETAH_CONNECTOR.get().is_some() {
        return Ok(());
    }
    crate::embedded::install_embedded_cheetah_connector()
}

#[cfg(feature = "cheetah")]
fn open_cheetah_pull(
    protocol: StreamProtocol,
    url: &str,
    options: SubscriberOptions,
    frame_policy: dg_core::ResourcePolicy,
) -> Result<PullEndpoint> {
    ensure_cheetah_connector_installed()?;
    let connector = CHEETAH_CONNECTOR.get().ok_or_else(|| {
        Error::Runtime(
            "no cheetah runtime connector installed; call install_cheetah_connector \
             before opening protocol URLs"
                .to_string(),
        )
    })?;
    let (tracks, source) = connector.open_pull(protocol, url, options)?;
    Ok(PullEndpoint {
        tracks,
        source: Box::new(CheetahSubscriberSourceAdapter::with_policy(
            source,
            protocol.label(),
            frame_policy,
        )),
    })
}

#[cfg(feature = "cheetah")]
fn open_cheetah_push(
    protocol: StreamProtocol,
    url: &str,
    options: PublisherOptions,
    frame_policy: dg_core::ResourcePolicy,
) -> Result<Box<dyn PublisherSink>> {
    ensure_cheetah_connector_installed()?;
    let connector = CHEETAH_CONNECTOR.get().ok_or_else(|| {
        Error::Runtime(
            "no cheetah runtime connector installed; call install_cheetah_connector \
             before opening protocol URLs"
                .to_string(),
        )
    })?;
    let sink = connector.open_push(protocol, url, options)?;
    Ok(Box::new(CheetahPublisherSinkAdapter::with_policy(
        sink,
        protocol.label(),
        frame_policy,
    )))
}

#[cfg(not(feature = "cheetah"))]
fn open_cheetah_pull(
    protocol: StreamProtocol,
    url: &str,
    _options: SubscriberOptions,
    _frame_policy: dg_core::ResourcePolicy,
) -> Result<PullEndpoint> {
    Err(cheetah_feature_disabled(protocol, url))
}

#[cfg(not(feature = "cheetah"))]
fn open_cheetah_push(
    protocol: StreamProtocol,
    url: &str,
    _options: PublisherOptions,
    _frame_policy: dg_core::ResourcePolicy,
) -> Result<Box<dyn PublisherSink>> {
    Err(cheetah_feature_disabled(protocol, url))
}

#[cfg(not(feature = "cheetah"))]
fn cheetah_feature_disabled(protocol: StreamProtocol, url: &str) -> Error {
    Error::Sdk(format!(
        "{} endpoint `{}` requires building dg-stream with the `cheetah` feature",
        protocol.label(),
        redact_url(url)
    ))
}
