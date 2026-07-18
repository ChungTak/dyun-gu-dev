//! CORE6-02 ResourcePolicy boundary tests.

use dg_core::ResourcePolicy;

fn policy_with(max_config_bytes: usize) -> ResourcePolicy {
    ResourcePolicy {
        max_config_bytes,
        ..ResourcePolicy::default()
    }
}

#[test]
fn effective_for_tightens_all_limits() {
    let hard = ResourcePolicy::default();
    let requested = ResourcePolicy {
        max_config_bytes: 1024,
        max_nodes: 64,
        max_model_bytes: 1024 * 1024,
        ..ResourcePolicy::default()
    };

    let effective = hard.effective_for(&requested).expect("effective");
    assert_eq!(effective.max_config_bytes, 1024);
    assert_eq!(effective.max_nodes, 64);
    assert_eq!(effective.max_model_bytes, 1024 * 1024);
}

#[test]
fn effective_for_rejects_exceeded_limit_with_field_value_and_hard() {
    let hard = policy_with(8);
    let requested = policy_with(16);
    let err = hard.effective_for(&requested).expect_err("should fail");
    let msg = err.to_string();
    assert!(msg.contains("max_config_bytes"), "{msg}");
    assert!(msg.contains("16"), "{msg}");
    assert!(msg.contains("8"), "{msg}");
}

#[test]
fn new_rejects_zero_limits() {
    let p = ResourcePolicy {
        max_nodes: 0,
        ..ResourcePolicy::default()
    };
    assert!(ResourcePolicy::new(p).is_err());
}

#[test]
fn new_rejects_32bit_unrepresentable_limits() {
    let p = ResourcePolicy {
        max_model_bytes: (u32::MAX as usize) + 1,
        ..ResourcePolicy::default()
    };
    let err = ResourcePolicy::new(p).expect_err("should fail");
    assert!(err.to_string().contains("32-bit"));
}

#[test]
fn check_config_bytes_accepts_at_limit() {
    let p = policy_with(4);
    p.check_config_bytes(3).expect("limit-1");
    p.check_config_bytes(4).expect("limit");
    assert!(p.check_config_bytes(5).is_err(), "limit+1");
}

#[test]
fn check_model_bytes_accepts_at_limit() {
    let p = ResourcePolicy {
        max_model_bytes: 2,
        ..ResourcePolicy::default()
    };
    p.check_model_bytes(1).expect("limit-1");
    p.check_model_bytes(2).expect("limit");
    assert!(p.check_model_bytes(3).is_err(), "limit+1");
}
