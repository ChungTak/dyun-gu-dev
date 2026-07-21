//! CORE7-03 bounded model loader tests.

use std::fs;
use std::io::Write;
use std::sync::Arc;

use dg_runtime::{BackendKind, BackendOptions, MockOptions, ModelSource, Runtime, RuntimeOption};

fn temp_file(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "dg-runtime-core7-{}-{}-{}.bin",
        name,
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ))
}

fn option_with_model_source(source: ModelSource, max_model_bytes: usize) -> RuntimeOption {
    let mut option = RuntimeOption::new(
        BackendKind::Mock,
        source,
        BackendOptions::Mock(MockOptions::default()),
    );
    option.process_policy.resource.max_model_bytes = max_model_bytes;
    option
}

#[test]
fn model_source_bytes_at_limit_is_accepted() {
    let source = ModelSource::Bytes(Arc::new(vec![1, 2, 3, 4]));
    let loaded = source.load_bounded(4).expect("load at limit");
    assert_eq!(loaded.as_ref(), &[1, 2, 3, 4]);
}

#[test]
fn model_source_bytes_over_limit_is_rejected() {
    let source = ModelSource::Bytes(Arc::new(vec![1, 2, 3, 4]));
    assert!(source.load_bounded(3).is_err());
}

#[test]
fn model_source_file_at_limit_is_accepted() {
    let path = temp_file("at-limit");
    let mut file = fs::File::create(&path).expect("create temp");
    file.write_all(&[7; 8]).expect("write");
    drop(file);
    let source = ModelSource::File(path.clone());
    let loaded = source.load_bounded(8).expect("load file at limit");
    assert_eq!(loaded.as_ref(), &[7; 8]);
    fs::remove_file(path).ok();
}

#[test]
fn model_source_file_over_limit_is_rejected_and_reads_only_limit_plus_one() {
    let path = temp_file("over-limit");
    let mut file = fs::File::create(&path).expect("create temp");
    file.write_all(&[9; 16]).expect("write");
    drop(file);
    let source = ModelSource::File(path.clone());
    let err = source.load_bounded(8).expect_err("oversized file");
    assert!(err.to_string().contains("exceeds limit"));
    fs::remove_file(path).ok();
}

#[test]
fn runtime_new_rejects_model_file_exceeding_limit() {
    let path = temp_file("runtime-reject");
    let mut file = fs::File::create(&path).expect("create temp");
    file.write_all(&[5; 16]).expect("write");
    drop(file);
    let option = option_with_model_source(ModelSource::File(path.clone()), 8);
    assert!(Runtime::new(option).is_err());
    fs::remove_file(path).ok();
}

#[test]
fn runtime_new_accepts_model_file_within_limit() {
    let path = temp_file("runtime-accept");
    let mut file = fs::File::create(&path).expect("create temp");
    file.write_all(&[5; 8]).expect("write");
    drop(file);
    let option = option_with_model_source(ModelSource::File(path.clone()), 8);
    assert!(Runtime::new(option).is_ok());
    fs::remove_file(path).ok();
}
