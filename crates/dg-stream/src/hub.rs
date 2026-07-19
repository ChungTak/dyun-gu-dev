use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tracing::debug;

use crate::error::{Error, Result};
use crate::ids::SubscriberId;
use crate::stream::{
    BackpressurePolicy, DispatchResult, MediaFilter, PublisherOptions, PublisherSink,
    ReceiveOutcome, SubscriberOptions, SubscriberSource,
};
use crate::track::{TrackInfo, TrackReadiness};
use dg_media::MediaFrame;

const RECV_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Tag key marking a frame as a random access point.
pub const KEYFRAME_TAG: &str = "keyframe";
/// Tag key carrying the media kind (`video` / `audio`) of a frame.
pub const MEDIA_TAG: &str = "media";

fn is_keyframe(frame: &MediaFrame) -> bool {
    frame
        .meta
        .tags
        .get(KEYFRAME_TAG)
        .is_some_and(|value| value == "true")
}

fn passes_filter(frame: &MediaFrame, filter: MediaFilter) -> bool {
    match frame.meta.tags.get(MEDIA_TAG).map(String::as_str) {
        Some("video") => filter.enable_video,
        Some("audio") => filter.enable_audio,
        _ => true,
    }
}

#[derive(Debug)]
struct SubscriberQueue {
    queue: VecDeque<Arc<MediaFrame>>,
    capacity: usize,
    policy: BackpressurePolicy,
    filter: MediaFilter,
    overflowed: bool,
    dropping_until_keyframe: bool,
}

#[derive(Debug, Default)]
struct StreamState {
    tracks: Vec<TrackInfo>,
    subscribers: HashMap<SubscriberId, SubscriberQueue>,
    /// Live [`HubPublisher`] handles for this stream.
    publishers: usize,
    publisher_closed: bool,
    keyframe_requests: u64,
}

#[derive(Debug, Default)]
struct StreamCore {
    state: Mutex<StreamState>,
    frame_ready: Condvar,
}

impl StreamCore {
    fn lock(&self) -> MutexGuard<'_, StreamState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

struct HubInner {
    streams: Mutex<HashMap<String, Arc<StreamCore>>>,
    next_subscriber: AtomicU64,
    max_streams: usize,
    max_subscribers_per_stream: usize,
}

impl HubInner {
    fn stream_count(&self) -> usize {
        match self.streams.lock() {
            Ok(guard) => guard.len(),
            Err(poisoned) => poisoned.into_inner().len(),
        }
    }

    fn try_get_stream(&self, url: &str) -> Option<Arc<StreamCore>> {
        let guard = self
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.get(url).map(Arc::clone)
    }

    /// Atomically creates a stream entry if missing, enforcing `max_streams`.
    fn get_or_create_stream(&self, url: &str) -> Result<Arc<StreamCore>> {
        let mut guard = self
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(existing) = guard.get(url) {
            return Ok(Arc::clone(existing));
        }
        if guard.len() >= self.max_streams {
            return Err(Error::Overflow(format!(
                "stream registry limit {} reached",
                self.max_streams
            )));
        }
        let core = Arc::new(StreamCore::default());
        guard.insert(url.to_string(), Arc::clone(&core));
        Ok(core)
    }

    /// Drops a stream registry entry when no publishers or subscribers remain.
    fn try_reap(&self, url: &str, core: &Arc<StreamCore>) {
        {
            let state = core.lock();
            if state.publishers > 0 || !state.subscribers.is_empty() {
                return;
            }
        }
        let mut streams = self
            .streams
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(existing) = streams.get(url) {
            if Arc::ptr_eq(existing, core) {
                let state = existing.lock();
                if state.publishers == 0 && state.subscribers.is_empty() {
                    drop(state);
                    streams.remove(url);
                }
            }
        }
    }
}

/// In-process stream hub backing the `mock://` scheme of the stream elements.
///
/// The hub is a Sans-I/O test double for a real media server: publishers and
/// subscribers exchange frames through bounded per-subscriber queues with the
/// configured [`BackpressurePolicy`], and publisher close is propagated to all
/// subscribers as a clean end of stream.
///
/// Stream registry entries are reaped when the last publisher and subscriber
/// leave, so unique URL churn cannot grow without bound.
#[derive(Clone)]
pub struct MemoryStreamHub {
    inner: Arc<HubInner>,
}

impl core::fmt::Debug for MemoryStreamHub {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MemoryStreamHub")
            .field("stream_count", &self.stream_count())
            .field("max_streams", &self.inner.max_streams)
            .field(
                "max_subscribers_per_stream",
                &self.inner.max_subscribers_per_stream,
            )
            .finish()
    }
}

impl Default for MemoryStreamHub {
    fn default() -> Self {
        Self::with_limits(10_000, 1_000)
    }
}

static GLOBAL_HUB: OnceLock<MemoryStreamHub> = OnceLock::new();

impl MemoryStreamHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limits(max_streams: usize, max_subscribers_per_stream: usize) -> Self {
        Self {
            inner: Arc::new(HubInner {
                streams: Mutex::new(HashMap::new()),
                next_subscriber: AtomicU64::new(0),
                max_streams,
                max_subscribers_per_stream,
            }),
        }
    }

    /// Process-wide hub used by graph elements with `mock://` URLs.
    pub fn global() -> &'static Self {
        GLOBAL_HUB.get_or_init(Self::new)
    }

    /// Number of stream registry entries currently retained.
    pub fn stream_count(&self) -> usize {
        self.inner.stream_count()
    }

    /// Opens a publisher for the stream at `url`.
    pub fn publish(&self, url: &str, _options: PublisherOptions) -> Result<HubPublisher> {
        let core = self.inner.get_or_create_stream(url)?;
        {
            let mut state = core.lock();
            state.publishers = state.publishers.saturating_add(1);
            state.publisher_closed = false;
        }
        Ok(HubPublisher {
            hub: Arc::clone(&self.inner),
            url: url.to_string(),
            core,
            closed: AtomicBool::new(false),
        })
    }

    /// Opens a subscriber for the stream at `url`.
    pub fn subscribe(&self, url: &str, options: SubscriberOptions) -> Result<HubSubscriber> {
        if options.queue_capacity == 0 {
            return Err(Error::InvalidArgument(
                "subscriber queue_capacity must be greater than zero".to_string(),
            ));
        }
        if self.inner.max_subscribers_per_stream == 0 {
            return Err(Error::Overflow(
                "subscriber limit for stream is zero".to_string(),
            ));
        }
        let core = self.inner.get_or_create_stream(url)?;
        let id = SubscriberId(self.inner.next_subscriber.fetch_add(1, Ordering::Relaxed));
        {
            let mut state = core.lock();
            if state.subscribers.len() >= self.inner.max_subscribers_per_stream {
                // Undo a just-created empty stream so limit failures do not leak
                // registry slots.
                let empty = state.publishers == 0 && state.subscribers.is_empty();
                drop(state);
                if empty {
                    self.inner.try_reap(url, &core);
                }
                return Err(Error::Overflow(format!(
                    "subscriber limit {} reached for stream {url}",
                    self.inner.max_subscribers_per_stream
                )));
            }
            state.subscribers.insert(
                id,
                SubscriberQueue {
                    queue: VecDeque::new(),
                    capacity: options.queue_capacity,
                    policy: options.backpressure,
                    filter: options.media_filter,
                    overflowed: false,
                    dropping_until_keyframe: false,
                },
            );
        }
        Ok(HubSubscriber {
            hub: Arc::clone(&self.inner),
            url: url.to_string(),
            core,
            id,
            closed: false,
        })
    }

    /// Current track metadata announced on the stream at `url`.
    ///
    /// Does not create a registry entry for unknown URLs.
    pub fn tracks(&self, url: &str) -> Vec<TrackInfo> {
        self.inner
            .try_get_stream(url)
            .map(|core| core.lock().tracks.clone())
            .unwrap_or_default()
    }

    /// Number of active subscribers on the stream at `url`.
    ///
    /// Does not create a registry entry for unknown URLs.
    pub fn subscriber_count(&self, url: &str) -> usize {
        self.inner
            .try_get_stream(url)
            .map(|core| core.lock().subscribers.len())
            .unwrap_or(0)
    }

    /// Requests a keyframe from the publisher of the stream at `url`.
    ///
    /// If the stream does not exist yet, creates it (subject to registry limits)
    /// so a late publisher can observe the request via [`PublisherSink::take_keyframe_requests`].
    pub fn request_keyframe(&self, url: &str) -> Result<()> {
        let core = self.inner.get_or_create_stream(url)?;
        let mut state = core.lock();
        state.keyframe_requests = state.keyframe_requests.saturating_add(1);
        Ok(())
    }
}

/// Publisher endpoint of the in-memory hub.
pub struct HubPublisher {
    hub: Arc<HubInner>,
    url: String,
    core: Arc<StreamCore>,
    closed: std::sync::atomic::AtomicBool,
}

impl PublisherSink for HubPublisher {
    fn update_tracks(&self, tracks: Vec<TrackInfo>) -> Result<()> {
        for track in &tracks {
            if track.readiness == TrackReadiness::Ready {
                track
                    .validate_codec_config()
                    .map_err(|err| Error::Media(err.to_string()))?;
            }
        }
        let mut state = self.core.lock();
        if state.publisher_closed || self.closed.load(Ordering::Acquire) {
            return Err(Error::Closed);
        }
        state.tracks = tracks;
        Ok(())
    }

    fn push_frame(&self, frame: Arc<MediaFrame>) -> Result<DispatchResult> {
        let mut state = self.core.lock();
        if state.publisher_closed || self.closed.load(Ordering::Acquire) {
            return Ok(DispatchResult::RejectedClosed);
        }
        let mut enqueued = false;
        let mut dropped = false;
        for subscriber in state.subscribers.values_mut() {
            if subscriber.overflowed || !passes_filter(&frame, subscriber.filter) {
                continue;
            }
            if subscriber.dropping_until_keyframe {
                if is_keyframe(&frame) {
                    subscriber.dropping_until_keyframe = false;
                } else {
                    dropped = true;
                    continue;
                }
            }
            if subscriber.queue.len() >= subscriber.capacity {
                match subscriber.policy {
                    BackpressurePolicy::DropDroppableFirst => {
                        let position = subscriber
                            .queue
                            .iter()
                            .position(|queued| !is_keyframe(queued));
                        match position {
                            Some(index) => {
                                subscriber.queue.remove(index);
                            }
                            None => {
                                subscriber.queue.pop_front();
                            }
                        }
                        dropped = true;
                        subscriber.queue.push_back(Arc::clone(&frame));
                        enqueued = true;
                    }
                    BackpressurePolicy::DropUntilNextKeyframe => {
                        dropped = true;
                        if is_keyframe(&frame) {
                            subscriber.queue.clear();
                            subscriber.queue.push_back(Arc::clone(&frame));
                            enqueued = true;
                        } else {
                            subscriber.dropping_until_keyframe = true;
                        }
                    }
                    BackpressurePolicy::DisconnectOnOverflow => {
                        subscriber.overflowed = true;
                        subscriber.queue.clear();
                        dropped = true;
                    }
                }
            } else {
                subscriber.queue.push_back(Arc::clone(&frame));
                enqueued = true;
            }
        }
        drop(state);
        self.core.frame_ready.notify_all();
        if enqueued || !dropped {
            Ok(DispatchResult::Accepted)
        } else {
            debug!("hub publisher dropped frame on all subscribers by policy");
            Ok(DispatchResult::DroppedByPolicy)
        }
    }

    fn close(&self) -> Result<()> {
        // Idempotent: Drop and explicit close must not double-decrement publishers.
        if self.closed.swap(true, Ordering::AcqRel) {
            return Ok(());
        }
        {
            let mut state = self.core.lock();
            if state.publishers > 0 {
                state.publishers = state.publishers.saturating_sub(1);
            }
            if state.publishers == 0 {
                state.publisher_closed = true;
            }
        }
        self.core.frame_ready.notify_all();
        self.hub.try_reap(&self.url, &self.core);
        Ok(())
    }

    fn take_keyframe_requests(&self) -> u64 {
        let mut state = self.core.lock();
        let value = state.keyframe_requests;
        state.keyframe_requests = 0;
        value
    }
}

impl Drop for HubPublisher {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// Subscriber endpoint of the in-memory hub.
#[derive(Clone)]
pub struct HubSubscriber {
    hub: Arc<HubInner>,
    url: String,
    core: Arc<StreamCore>,
    id: SubscriberId,
    closed: bool,
}

#[async_trait]
impl SubscriberSource for HubSubscriber {
    async fn recv(&mut self) -> Result<Option<Arc<MediaFrame>>> {
        loop {
            match self.recv_timeout(RECV_POLL_INTERVAL).await? {
                ReceiveOutcome::Frame(frame) => return Ok(Some(frame)),
                ReceiveOutcome::EndOfStream => return Ok(None),
                ReceiveOutcome::TimedOut => continue,
            }
        }
    }

    async fn recv_timeout(&mut self, timeout: Duration) -> Result<ReceiveOutcome> {
        if self.closed {
            return Ok(ReceiveOutcome::EndOfStream);
        }
        let mut state = self.core.lock();
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or_else(|| Error::InvalidArgument("recv timeout overflowed".to_string()))?;
        loop {
            let Some(subscriber) = state.subscribers.get_mut(&self.id) else {
                self.closed = true;
                drop(state);
                self.hub.try_reap(&self.url, &self.core);
                return Ok(ReceiveOutcome::EndOfStream);
            };
            if subscriber.overflowed {
                state.subscribers.remove(&self.id);
                self.closed = true;
                drop(state);
                self.hub.try_reap(&self.url, &self.core);
                return Err(Error::Overflow(
                    "subscriber disconnected: queue overflow".to_string(),
                ));
            }
            if let Some(frame) = subscriber.queue.pop_front() {
                return Ok(ReceiveOutcome::Frame(frame));
            }
            if state.publisher_closed {
                return Ok(ReceiveOutcome::EndOfStream);
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(ReceiveOutcome::TimedOut);
            }
            let wait = deadline - now;
            let (s, result) = self
                .core
                .frame_ready
                .wait_timeout(state, wait)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = s;
            if result.timed_out() {
                return Ok(ReceiveOutcome::TimedOut);
            }
        }
    }

    async fn close(&mut self) -> Result<()> {
        if !self.closed {
            self.closed = true;
            {
                let mut state = self.core.lock();
                state.subscribers.remove(&self.id);
            }
            self.core.frame_ready.notify_all();
            self.hub.try_reap(&self.url, &self.core);
        }
        Ok(())
    }

    fn id(&self) -> SubscriberId {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::{BackpressurePolicy, SubscriberOptions, SubscriberSourceSyncExt};

    #[test]
    fn registry_reaps_stream_when_last_handles_close() {
        let hub = MemoryStreamHub::with_limits(8, 8);
        assert_eq!(hub.stream_count(), 0);
        {
            let publisher = hub
                .publish("mock://reap", PublisherOptions::default())
                .expect("publish");
            let mut subscriber = hub
                .subscribe(
                    "mock://reap",
                    SubscriberOptions {
                        queue_capacity: 2,
                        backpressure: BackpressurePolicy::DropDroppableFirst,
                        ..SubscriberOptions::default()
                    },
                )
                .expect("subscribe");
            assert_eq!(hub.stream_count(), 1);
            subscriber.close_blocking().expect("close sub");
            assert_eq!(hub.stream_count(), 1, "publisher still holds stream");
            drop(publisher);
        }
        assert_eq!(hub.stream_count(), 0, "empty stream must be reaped");
    }

    #[test]
    fn tracks_on_unknown_url_does_not_create_stream() {
        let hub = MemoryStreamHub::with_limits(1, 1);
        assert!(hub.tracks("mock://missing").is_empty());
        assert_eq!(hub.stream_count(), 0);
        assert_eq!(hub.subscriber_count("mock://missing"), 0);
    }

    #[test]
    fn stream_limit_is_enforced_atomically_and_slots_free_after_reap() {
        let hub = MemoryStreamHub::with_limits(1, 2);
        let first = hub
            .publish("mock://a", PublisherOptions::default())
            .expect("first");
        assert!(hub
            .publish("mock://b", PublisherOptions::default())
            .is_err());
        drop(first);
        assert_eq!(hub.stream_count(), 0);
        hub.publish("mock://b", PublisherOptions::default())
            .expect("slot freed after reap");
    }
}
