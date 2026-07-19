//! CORE7-04 runtime/backend cancel and capability contract tests.

use std::time::Duration;

use dg_core::{DataFormat, DataType, DeviceKind, Shape, Tensor, TensorDesc};
use dg_runtime::{
    BackendKind, BackendOptions, CancelReport, ExecutionMode, MockOptions, ModelSource, Runtime,
    RuntimeOption, TensorInfo,
};

fn u8_tensor() -> Tensor {
    let device = dg_core::CpuDevice::new();
    let desc = TensorDesc::new(
        Shape::new([1, 4]),
        DataType::U8,
        DataFormat::NC,
        DeviceKind::Cpu,
    );
    let tensor = Tensor::allocate(&device, desc).expect("allocate");
    tensor.buffer().write_from_slice(&[1; 4]).expect("seed");
    tensor
}

fn mock_option(delay: Option<Duration>) -> RuntimeOption {
    let info = TensorInfo::new(Shape::new([1, 4]), DataType::U8).with_layout(DataFormat::NC);
    RuntimeOption::new(
        BackendKind::Mock,
        ModelSource::Bytes(Vec::new()),
        BackendOptions::Mock(MockOptions {
            input_infos: vec![info.clone()],
            output_infos: vec![info],
            echo_inputs: true,
            fill_value: 0,
            delay,
            max_in_flight: 2,
            ..Default::default()
        }),
    )
}

#[test]
fn async_mock_reports_native_async_capability() {
    let runtime =
        Runtime::new(mock_option(Some(Duration::from_millis(1)))).expect("construct runtime");
    assert_eq!(
        runtime.capabilities().execution_mode,
        ExecutionMode::NativeAsync
    );
    let record = &runtime.capabilities().device_records[0];
    assert_eq!(record.execution_mode, ExecutionMode::NativeAsync);
    assert!(record.async_capable);
}

#[test]
fn sync_mock_reports_bounded_sync_capability() {
    let runtime = Runtime::new(mock_option(None)).expect("construct runtime");
    assert_eq!(
        runtime.capabilities().execution_mode,
        ExecutionMode::BoundedSync
    );
    let record = &runtime.capabilities().device_records[0];
    assert_eq!(record.execution_mode, ExecutionMode::BoundedSync);
    assert!(!record.async_capable);
}

#[test]
fn cancel_report_includes_all_diagnostic_fields() {
    let mut runtime =
        Runtime::new(mock_option(Some(Duration::from_millis(100)))).expect("construct runtime");
    let input = u8_tensor();
    runtime
        .submit(std::slice::from_ref(&input), None)
        .expect("submit 1");
    runtime
        .submit(std::slice::from_ref(&input), None)
        .expect("submit 2");

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
}

#[test]
fn sync_cancel_is_reported_as_unsupported() {
    let mut runtime = Runtime::new(mock_option(None)).expect("construct runtime");
    let input = u8_tensor();
    let sequence = runtime
        .submit(std::slice::from_ref(&input), None)
        .expect("submit");
    let report = runtime.cancel().expect("cancel");
    assert_eq!(
        report,
        CancelReport {
            requested: 1,
            completed: 0,
            abandoned: 0,
            failed: 0,
            unsupported: 1,
        }
    );
    // The sync result was dropped but in-flight accounting must be released.
    assert_eq!(runtime.in_flight(), 0);
    // Runtime is still usable for a fresh submit/poll cycle.
    let sequence2 = runtime
        .submit(std::slice::from_ref(&input), None)
        .expect("submit after cancel");
    assert_ne!(sequence, sequence2);
}
