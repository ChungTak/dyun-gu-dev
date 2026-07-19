use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use dg_core::{DataFormat, DataType, DeviceKind};
use dg_media::{MediaFrame, MediaFrameKind};
use dg_stream::{
    BackpressurePolicy, MemoryStreamHub, PublisherOptions, PublisherSink, ReceiveOutcome,
    SubscriberOptions, SubscriberSourceSyncExt,
};

fn subscriber_options(capacity: usize, backpressure: BackpressurePolicy) -> SubscriberOptions {
    SubscriberOptions {
        queue_capacity: capacity,
        backpressure,
        ..SubscriberOptions::default()
    }
}

fn video_frame(pts: i64, key: bool, bytes: &[u8]) -> Arc<MediaFrame> {
    let mut frame = MediaFrame::from_host_bytes(
        MediaFrameKind::Tensor,
        DataType::U8,
        DataFormat::Auto,
        vec![bytes.len()],
        DeviceKind::Cpu,
        bytes.to_vec(),
    )
    .expect("host bytes");
    frame.meta.pts = Some(pts);
    frame.meta.tags.insert(
        dg_stream::KEYFRAME_TAG.to_string(),
        if key { "true" } else { "false" }.to_string(),
    );
    frame
        .meta
        .tags
        .insert(dg_stream::MEDIA_TAG.to_string(), "video".to_string());
    Arc::new(frame)
}

#[test]
fn subscriber_recv_timeout_returns_timed_out_then_resumes() {
    let hub = MemoryStreamHub::new();
    let publisher = hub
        .publish("mock://timeout", PublisherOptions::default())
        .expect("publish");
    let mut subscriber = hub
        .subscribe(
            "mock://timeout",
            subscriber_options(10, BackpressurePolicy::DropDroppableFirst),
        )
        .expect("subscribe");

    let start = Instant::now();
    let outcome = subscriber
        .recv_blocking_timeout(Duration::from_millis(50))
        .expect("recv timeout");
    assert!(matches!(outcome, ReceiveOutcome::TimedOut));
    assert!(start.elapsed() >= Duration::from_millis(50));

    publisher
        .push_frame(video_frame(0, true, b"frame"))
        .unwrap();
    let outcome = subscriber
        .recv_blocking_timeout(Duration::from_millis(100))
        .expect("recv frame");
    assert!(matches!(outcome, ReceiveOutcome::Frame(_)));
}

#[test]
fn subscriber_close_wakes_pending_recv() {
    let hub = MemoryStreamHub::new();
    let mut subscriber = hub
        .subscribe(
            "mock://close-wake",
            subscriber_options(10, BackpressurePolicy::DropDroppableFirst),
        )
        .expect("subscribe");

    let mut sub_clone = subscriber.clone();
    let handle = thread::spawn(move || sub_clone.recv_blocking_timeout(Duration::from_millis(500)));
    thread::sleep(Duration::from_millis(50));
    subscriber.close_blocking().expect("close");
    let start = Instant::now();
    let outcome = handle.join().expect("thread").expect("recv");
    assert!(
        matches!(outcome, ReceiveOutcome::EndOfStream),
        "got {outcome:?}"
    );
    assert!(start.elapsed() < Duration::from_millis(200));
}

#[test]
fn hub_stream_and_subscriber_limits_are_enforced() {
    let hub = MemoryStreamHub::with_limits(1, 1);
    let _publisher = hub
        .publish("mock://a", PublisherOptions::default())
        .expect("first publish");
    let _subscriber = hub
        .subscribe(
            "mock://a",
            subscriber_options(1, BackpressurePolicy::DropDroppableFirst),
        )
        .expect("first subscribe");

    assert!(hub
        .publish("mock://b", PublisherOptions::default())
        .is_err());
    assert!(hub
        .subscribe(
            "mock://a",
            subscriber_options(1, BackpressurePolicy::DropDroppableFirst)
        )
        .is_err());
}

#[test]
fn hub_registry_reaps_and_does_not_grow_with_url_churn() {
    let hub = MemoryStreamHub::with_limits(4, 4);
    for i in 0..20 {
        let url = format!("mock://churn-{i}");
        let publisher = hub
            .publish(&url, PublisherOptions::default())
            .expect("publish");
        let mut subscriber = hub
            .subscribe(
                &url,
                subscriber_options(2, BackpressurePolicy::DropDroppableFirst),
            )
            .expect("subscribe");
        publisher
            .push_frame(video_frame(0, true, b"x"))
            .expect("push");
        let _ = subscriber
            .recv_blocking_timeout(Duration::from_millis(50))
            .expect("recv");
        subscriber.close_blocking().expect("close sub");
        drop(publisher);
        assert!(
            hub.stream_count() <= 4,
            "registry must stay within capacity after reap, got {}",
            hub.stream_count()
        );
    }
    assert_eq!(hub.stream_count(), 0);
}
