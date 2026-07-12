#![cfg(feature = "cheetah")]

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use dg_stream::{CheetahRuntimeConnector, EmbeddedCheetahRuntimeConnector, Error, StreamProtocol};
use dg_stream_cheetah::cheetah_connector::{
    ConnectorBuilder, LoopbackLayer, LoopbackOptions, LoopbackTopology, Protocol,
};
use dg_stream_cheetah::cheetah_runtime_tokio::TokioRuntime;
use dg_stream_cheetah::{
    AVFrame, CodecId, FrameFlags, FrameFormat, MediaKind, Timebase, TrackId, TrackInfo,
    TrackReadiness,
};

#[test]
fn embedded_connector_routes_all_stream_protocols() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    runtime.block_on(async {
        let connector = EmbeddedCheetahRuntimeConnector::new().expect("embedded connector");
        let pull_cases = [
            (StreamProtocol::RtspPull, "rtsp://"),
            (StreamProtocol::HttpFlvPull, "http://"),
        ];
        for (protocol, url) in pull_cases {
            let error = match connector.open_pull(protocol, url, Default::default()) {
                Ok(_) => panic!("invalid endpoint unexpectedly opened"),
                Err(error) => error,
            };
            assert!(matches!(error, Error::Sdk(_)), "{error:?}");
        }

        let push_cases = [
            (StreamProtocol::RtmpPush, "rtmp://"),
            (StreamProtocol::WebRtcPush, "webrtc://"),
        ];
        for (protocol, url) in push_cases {
            let error = match connector.open_push(protocol, url, Default::default()) {
                Ok(_) => panic!("invalid endpoint unexpectedly opened"),
                Err(error) => error,
            };
            assert!(matches!(error, Error::Sdk(_)), "{error:?}");
        }
    });
}

#[test]
fn engine_only_loopback_roundtrips_h264_without_sockets() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .expect("test runtime");
    runtime
        .block_on(async {
            let runtime_api =
                Arc::new(TokioRuntime::new()) as Arc<dyn dg_stream_cheetah::RuntimeApi>;
            let connector = ConnectorBuilder::new(runtime_api)
                .without_default_modules()
                .build()?;
            connector.start().await?;

            let mut track = TrackInfo::new(TrackId(0), MediaKind::Video, CodecId::H264, 90_000);
            track.readiness = TrackReadiness::Ready;
            track.extradata = dg_stream_cheetah::CodecExtradata::H264 {
                sps: vec![Bytes::from_static(&[0x67, 0x42, 0x00, 0x1f])],
                pps: vec![Bytes::from_static(&[0x68, 0xce, 0x3c, 0x80])],
                avcc: None,
            };

            let options = LoopbackOptions {
                stream_name: "dg_stream_engine_only".to_string(),
                topology: LoopbackTopology::SameProtocol {
                    protocol: Protocol::Rtmp,
                },
                preferred_layer: LoopbackLayer::EngineOnlyBypassWire,
                tracks: vec![track],
                ..Default::default()
            };

            let mut pair = connector.open_in_memory_loopback(options).await?;
            assert_eq!(pair.layer, LoopbackLayer::EngineOnlyBypassWire);
            pair.publisher.wait_ready().await?;

            let mut frame = AVFrame::new(
                TrackId(0),
                MediaKind::Video,
                CodecId::H264,
                FrameFormat::CanonicalH26x,
                0,
                0,
                Timebase::new(1, 1_000),
                Bytes::from_static(&[
                    0x00, 0x00, 0x00, 0x01, 0x65, 0x88, 0x84, 0x00, 0x2f, 0xff, 0xff, 0x00, 0x04,
                    0x00, 0x00, 0x04, 0x01,
                ]),
            );
            frame.flags = FrameFlags::KEY;
            pair.publisher.push_frame(Arc::new(frame))?;

            let received = tokio::time::timeout(Duration::from_secs(5), pair.subscriber.recv())
                .await??
                .ok_or("loopback subscriber ended")?;
            assert_eq!(received.codec, CodecId::H264);
            assert_eq!(received.media_kind, MediaKind::Video);
            assert!(!received.payload.is_empty());

            pair.publisher.close()?;
            pair.subscriber.close().await?;
            connector.stop().await;
            Ok::<(), Box<dyn std::error::Error>>(())
        })
        .expect("engine-only loopback");
}
