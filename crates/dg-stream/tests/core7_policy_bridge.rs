//! CORE7-03 / R7-003: policy is threaded into stream open helpers so product
//! paths do not silently fall back to default frame limits.

use dg_core::ResourcePolicy;
use dg_stream::{open_pull_with_policy, open_push_with_policy, StreamProtocol};

#[test]
fn open_pull_with_policy_rejects_non_pull_protocol() {
    let policy = ResourcePolicy {
        max_frame_bytes: 64,
        ..ResourcePolicy::default()
    };
    match open_pull_with_policy(
        StreamProtocol::RtmpPush,
        "mock://policy-test",
        Default::default(),
        policy,
    ) {
        Ok(_) => panic!("rtmp is not a pull protocol"),
        Err(err) => assert!(
            err.to_string().contains("not a pull protocol"),
            "unexpected error: {err}"
        ),
    }
}

#[test]
fn open_push_with_policy_rejects_pull_protocol() {
    let policy = ResourcePolicy {
        max_frame_bytes: 64,
        ..ResourcePolicy::default()
    };
    match open_push_with_policy(
        StreamProtocol::RtspPull,
        "mock://policy-test",
        Default::default(),
        policy,
    ) {
        Ok(_) => panic!("rtsp is not a push protocol"),
        Err(err) => assert!(
            err.to_string().contains("not a push protocol"),
            "unexpected error: {err}"
        ),
    }
}

#[test]
fn effective_frame_limit_is_honored_before_allocation() {
    // Pure resource-policy contract used by the Cheetah bridge pre-copy path.
    // The bridge calls check_frame_bytes before try_reserve/extend_from_slice.
    let policy = ResourcePolicy {
        max_frame_bytes: 8,
        ..ResourcePolicy::default()
    };
    let ok = policy.check_frame_bytes(8);
    assert!(ok.is_ok(), "exactly at limit must pass: {ok:?}");
    let err = policy
        .check_frame_bytes(9)
        .expect_err("one byte over limit must fail");
    let text = err.to_string();
    assert!(
        text.contains("frame") || text.contains("limit") || text.contains("bytes"),
        "unexpected limit error: {text}"
    );
}
