//! CORE6-02 Runtime model-size ResourcePolicy boundary tests.

use dg_core::ResourcePolicy;
use dg_runtime::{BackendKind, BackendOptions, MockOptions, ModelSource, Runtime, RuntimeOption};

fn option_with_model(size: usize) -> RuntimeOption {
    RuntimeOption::new(
        BackendKind::Mock,
        ModelSource::Bytes(vec![0; size]),
        BackendOptions::Mock(MockOptions::default()),
    )
}

#[test]
fn runtime_new_with_policy_accepts_limit_minus_one_and_limit() {
    let policy = ResourcePolicy {
        max_model_bytes: 4,
        ..ResourcePolicy::default()
    };
    Runtime::new_with_policy(option_with_model(3), policy.clone()).expect("limit-1");
    Runtime::new_with_policy(option_with_model(4), policy).expect("limit");
}

#[test]
fn runtime_new_with_policy_rejects_limit_plus_one() {
    let policy = ResourcePolicy {
        max_model_bytes: 4,
        ..ResourcePolicy::default()
    };
    let err = Runtime::new_with_policy(option_with_model(5), policy)
        .err()
        .expect("limit+1");
    assert!(err.to_string().contains("model bytes"), "{err}");
}

#[test]
fn runtime_new_with_default_policy_rejects_over_limit() {
    let policy = ResourcePolicy {
        max_model_bytes: 2,
        ..ResourcePolicy::default()
    };
    let err = Runtime::new_with_policy(option_with_model(3), policy)
        .err()
        .expect("should reject above limit");
    assert!(err.to_string().contains("model bytes"), "{err}");
}
