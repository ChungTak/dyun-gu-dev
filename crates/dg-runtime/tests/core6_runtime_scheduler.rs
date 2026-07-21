use std::sync::Arc;
use std::time::{Duration, Instant};

use dg_core::{DataFormat, DataType, DeviceKind, Shape, Tensor, TensorDesc};
use dg_runtime::{
    BackendKind, BackendMetrics, BackendOptions, CancelReport, InferPoll, MockOptions, ModelSource,
    Runtime, RuntimeOption, TensorInfo,
};

fn u8_tensor(value: u8) -> Tensor {
    let device = dg_core::CpuDevice::new();
    let desc = TensorDesc::new(
        Shape::new([1, 4]),
        DataType::U8,
        DataFormat::NC,
        DeviceKind::Cpu,
    );
    let tensor = Tensor::allocate(&device, desc).expect("allocate");
    tensor.buffer().write_from_slice(&[value; 4]).expect("seed");
    tensor
}

fn mock_option_with_delay(delay: Duration, max_in_flight: usize) -> RuntimeOption {
    let info = TensorInfo::new(Shape::new([1, 4]), DataType::U8).with_layout(DataFormat::NC);
    RuntimeOption::new(
        BackendKind::Mock,
        ModelSource::Bytes(Arc::new(Vec::new())),
        BackendOptions::Mock(MockOptions {
            input_infos: vec![info.clone()],
            output_infos: vec![info],
            echo_inputs: true,
            fill_value: 0,
            delay: Some(delay),
            max_in_flight,
            ..Default::default()
        }),
    )
}

#[test]
fn cancel_report_releases_in_flight_and_records_cancel() {
    let mut runtime = Runtime::new(mock_option_with_delay(Duration::from_millis(200), 2))
        .expect("construct runtime");
    let input = u8_tensor(7);

    runtime
        .submit(std::slice::from_ref(&input), None)
        .expect("submit 1");
    runtime
        .submit(std::slice::from_ref(&input), None)
        .expect("submit 2");
    assert_eq!(runtime.in_flight(), 2);

    let report = runtime.cancel().expect("cancel");
    assert_eq!(
        report,
        CancelReport {
            requested: 2,
            completed: 2,
            abandoned: 0,
            failed: 0,
            unsupported: 0,
        }
    );
    assert_eq!(runtime.in_flight(), 0);
    assert_eq!(runtime.metrics().cancelled(), 1);

    let sequence = runtime
        .submit(std::slice::from_ref(&input), None)
        .expect("submit after cancel");
    std::thread::sleep(Duration::from_millis(250));
    let InferPoll::Ready {
        sequence: ready_seq,
        ..
    } = runtime.poll().expect("poll")
    else {
        panic!("should be ready");
    };
    assert_eq!(ready_seq, sequence);
}

#[test]
fn shared_metrics_aggregate_submissions_across_runtimes() {
    let metrics = Arc::new(BackendMetrics::default());
    let option = mock_option_with_delay(Duration::from_millis(50), 1);

    let mut runtime_a =
        Runtime::new_with_metrics(option.clone(), Arc::clone(&metrics)).expect("runtime a");
    let mut runtime_b = Runtime::new_with_metrics(option, Arc::clone(&metrics)).expect("runtime b");
    let input = u8_tensor(3);

    runtime_a
        .submit(std::slice::from_ref(&input), None)
        .expect("submit a");
    runtime_b
        .submit(std::slice::from_ref(&input), None)
        .expect("submit b");

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.submissions, 2);
    assert_eq!(snapshot.in_flight, 2);

    std::thread::sleep(Duration::from_millis(100));
    while matches!(runtime_a.poll().expect("poll a"), InferPoll::Ready { .. }) {}
    while matches!(runtime_b.poll().expect("poll b"), InferPoll::Ready { .. }) {}

    let snapshot = metrics.snapshot();
    assert_eq!(snapshot.submissions, 2);
    assert_eq!(snapshot.in_flight, 0);
    assert!(snapshot.infer_latencies.count >= 2);
}

#[test]
fn in_flight_underflow_is_recorded_not_panicked() {
    let metrics = Arc::new(BackendMetrics::default());
    metrics.record_submission();
    metrics.finish_in_flight();
    metrics.finish_in_flight();
    assert_eq!(metrics.in_flight(), 0);
    assert_eq!(metrics.underflow_count(), 1);
    assert_eq!(metrics.overflow_count(), 0);
}

#[test]
fn latency_histogram_is_bounded_after_million_records() {
    let metrics = Arc::new(BackendMetrics::default());
    let start = Instant::now();
    for _ in 0..1_000_000 {
        metrics.record_infer_latency_ns(1_000_000);
    }
    let snapshot = metrics.infer_latency_percentiles();
    assert_eq!(snapshot.count, 1_000_000);
    assert_eq!(snapshot.buckets.iter().copied().sum::<u64>(), 1_000_000);
    assert!(snapshot.p50_ns > 0);
    assert!(start.elapsed() < Duration::from_secs(5));
}
