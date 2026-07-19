use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use dg_core::{CpuDevice, DataFormat, DataType, DeviceKind, Shape, Tensor, TensorDesc};
use dg_graph::{
    CreatedElement, Element, ElementDescriptor, ElementHandle, ElementIo, Graph, GraphFormat,
    GraphSpec, GraphStatus, ParamField, PortSchema, Result,
};

const IN_PORT: PortSchema = PortSchema {
    name: "in",
    dtype: Some(DataType::F32),
    required: true,
};

const OUT_PORT: PortSchema = PortSchema {
    name: "out",
    dtype: Some(DataType::F32),
    required: false,
};

const NO_PARAMS: &[ParamField] = &[];

fn make_tensor() -> Tensor {
    let device = CpuDevice::new();
    let desc = TensorDesc::new(
        Shape::new([1, 4]),
        DataType::F32,
        DataFormat::NC,
        DeviceKind::Cpu,
    );
    Tensor::allocate(&device, desc).expect("allocate test tensor")
}

struct PanicElement;

impl Element for PanicElement {
    fn run(self: Box<Self>, io: ElementIo) -> Result<()> {
        let _ = io;
        panic!("test panic");
    }
}

fn create_panic(_node: &dg_graph::NodeSpec) -> Result<CreatedElement> {
    Ok(CreatedElement {
        element: Box::new(PanicElement),
        handle: ElementHandle::None,
    })
}

inventory::submit! {
    ElementDescriptor {
        kind: "core6_test_panic",
        input_ports: &[IN_PORT],
        output_ports: &[OUT_PORT],
        params: NO_PARAMS,
        validate: None,
        create: create_panic,
    }
}

struct HoardElement;

impl Element for HoardElement {
    fn run(self: Box<Self>, io: ElementIo) -> Result<()> {
        loop {
            match io.recv("in") {
                Ok(Some(_)) => continue,
                Ok(None) => {
                    if io.should_stop() {
                        return Err(dg_graph::Error::NotRunning);
                    }
                    continue;
                }
                Err(err) => return Err(err),
            }
        }
    }
}

fn create_hoard(_node: &dg_graph::NodeSpec) -> Result<CreatedElement> {
    Ok(CreatedElement {
        element: Box::new(HoardElement),
        handle: ElementHandle::None,
    })
}

inventory::submit! {
    ElementDescriptor {
        kind: "core6_test_hoard",
        input_ports: &[IN_PORT],
        output_ports: &[OUT_PORT],
        params: NO_PARAMS,
        validate: None,
        create: create_hoard,
    }
}

struct SlowElement;

impl Element for SlowElement {
    fn run(self: Box<Self>, io: ElementIo) -> Result<()> {
        loop {
            // Sleep first so the worker ignores the cooperative stop signal
            // long enough for a short shutdown deadline to expire.
            thread::sleep(Duration::from_millis(100));
            if io.should_stop() {
                return Err(dg_graph::Error::NotRunning);
            }
        }
    }
}

fn create_slow(_node: &dg_graph::NodeSpec) -> Result<CreatedElement> {
    Ok(CreatedElement {
        element: Box::new(SlowElement),
        handle: ElementHandle::None,
    })
}

inventory::submit! {
    ElementDescriptor {
        kind: "core6_test_slow",
        input_ports: &[IN_PORT],
        output_ports: &[OUT_PORT],
        params: NO_PARAMS,
        validate: None,
        create: create_slow,
    }
}

#[test]
fn worker_panic_becomes_typed_error_with_node_name() {
    let yaml = r#"
apiVersion: dg/v1
kind: Graph
nodes:
  - name: source
    kind: source
    params:
      count: 1
      shape: [1, 4]
      start: 1.0
  - name: panic
    kind: core6_test_panic
  - name: sink
    kind: sink
connections:
  - source.out -> panic.in
  - panic.out -> sink.in
"#;
    let spec = GraphSpec::from_str_with_format(yaml, GraphFormat::Yaml).expect("parse spec");
    let err = Graph::new(spec)
        .expect("build graph")
        .run()
        .expect_err("panic must fail");
    assert!(
        matches!(err, dg_graph::Error::Element { ref element, .. } if element == "panic"),
        "expected Element error for node panic, got {err}"
    );
}

#[test]
fn packet_starts_max_depth_is_enforced_without_oom() {
    let yaml = r#"
apiVersion: dg/v1
kind: Graph
execution:
  parallel: pipeline
  queue_capacity: 2
nodes:
  - name: source
    kind: source
    params:
      count: 5
      shape: [1, 4]
      start: 1.0
  - name: hoard
    kind: core6_test_hoard
  - name: sink
    kind: sink
connections:
  - source.out -> hoard.in
  - hoard.out -> sink.in
"#;
    let spec = GraphSpec::from_str_with_format(yaml, GraphFormat::Yaml).expect("parse spec");
    let err = Graph::new(spec)
        .expect("build graph")
        .run()
        .expect_err("hoard must exceed packet_starts");
    assert!(
        err.to_string().contains("packet_starts"),
        "expected packet_starts limit, got {err}"
    );
}

#[test]
fn shutdown_timeout_is_retryable_and_keeps_draining_status() {
    let yaml = r#"
apiVersion: dg/v1
kind: Graph
execution:
  parallel: pipeline
  queue_capacity: 20
nodes:
  - name: source
    kind: source
    params:
      count: 100
      shape: [1, 4]
      start: 1.0
  - name: slow
    kind: core6_test_slow
  - name: sink
    kind: sink
connections:
  - source.out -> slow.in
  - slow.out -> sink.in
"#;
    let spec = GraphSpec::from_str_with_format(yaml, GraphFormat::Yaml).expect("parse spec");
    let graph = Graph::new(spec).expect("build graph");
    let mut running = graph.start(HashMap::new()).expect("start graph");

    let err = running
        .shutdown(Duration::from_millis(10))
        .expect_err("shutdown must time out");
    assert!(err.to_string().contains("timed out"), "got {err}");

    let (status, _) = running.status();
    assert_eq!(status, GraphStatus::Draining);

    running
        .shutdown(Duration::from_millis(300))
        .expect("retry shutdown should succeed");
    let (status, _) = running.status();
    assert_eq!(status, GraphStatus::Stopped);
}

#[test]
fn sink_packet_budget_fails_without_oom() {
    let yaml = r#"
apiVersion: dg/v1
kind: Graph
limits:
  max_buffer_packets: 2
execution:
  parallel: pipeline
  queue_capacity: 2
nodes:
  - name: source
    kind: source
    params:
      count: 3
      shape: [1, 4]
      start: 1.0
  - name: infer
    kind: mock_inference
    params:
      shape: [1, 4]
      echo_inputs: true
  - name: sink
    kind: sink
connections:
  - source.out -> infer.in
  - infer.out -> sink.in
"#;
    let spec = GraphSpec::from_str_with_format(yaml, GraphFormat::Yaml).expect("parse spec");
    let err = Graph::new(spec)
        .expect("build graph")
        .run()
        .expect_err("sink budget must fail");
    assert!(
        matches!(err, dg_graph::Error::ResourceLimit { ref resource, .. } if resource.contains("sink")),
        "expected sink ResourceLimit, got {err}"
    );
}

#[test]
fn input_packet_budget_fails_at_start() {
    let yaml = r#"
apiVersion: dg/v1
kind: Graph
limits:
  max_buffer_packets: 1
nodes:
  - name: input
    kind: input
  - name: sink
    kind: sink
connections:
  - input.out -> sink.in
"#;
    let spec = GraphSpec::from_str_with_format(yaml, GraphFormat::Yaml).expect("parse spec");
    let inputs = HashMap::from([("input".to_string(), vec![make_tensor(), make_tensor()])]);
    let err = Graph::new(spec)
        .expect("build graph")
        .run_with_inputs(inputs)
        .expect_err("input budget must fail");
    assert!(
        err.to_string().contains("buffer packet count"),
        "expected buffer packet count error, got {err}"
    );
}

#[test]
fn input_tensor_bytes_checked_per_tensor() {
    let yaml = r#"
apiVersion: dg/v1
kind: Graph
limits:
  max_tensor_bytes: 1
nodes:
  - name: input
    kind: input
  - name: infer
    kind: mock_inference
    params:
      shape: [1, 4]
      echo_inputs: true
  - name: sink
    kind: sink
connections:
  - input.out -> infer.in
  - infer.out -> sink.in
"#;
    let spec = GraphSpec::from_str_with_format(yaml, GraphFormat::Yaml).expect("parse spec");
    let inputs = HashMap::from([("input".to_string(), vec![make_tensor()])]);
    let err = Graph::new(spec)
        .expect("build graph")
        .run_with_inputs(inputs)
        .expect_err("per-tensor byte limit must fail");
    assert!(
        err.to_string().contains("tensor bytes"),
        "expected per-tensor byte limit error, got {err}"
    );
}

#[test]
fn large_packet_backpressure_is_bounded_by_sink_bytes() {
    let yaml = r#"
apiVersion: dg/v1
kind: Graph
limits:
  max_buffer_bytes: 1
execution:
  parallel: pipeline
  queue_capacity: 2
nodes:
  - name: source
    kind: source
    params:
      count: 1
      shape: [1024, 1024, 3]
      dtype: f32
      start: 1.0
  - name: infer
    kind: mock_inference
    params:
      shape: [1024, 1024, 3]
      echo_inputs: true
  - name: sink
    kind: sink
connections:
  - source.out -> infer.in
  - infer.out -> sink.in
"#;
    let spec = GraphSpec::from_str_with_format(yaml, GraphFormat::Yaml).expect("parse spec");
    let err = Graph::new(spec)
        .expect("build graph")
        .run()
        .expect_err("sink bytes must fail");
    assert!(
        matches!(err, dg_graph::Error::ResourceLimit { ref resource, .. } if resource.contains("sink")),
        "expected sink bytes ResourceLimit, got {err}"
    );
}
