use std::collections::HashMap;

use dg_core::{CpuDevice, DataFormat, DataType, DeviceKind, Shape, Tensor, TensorDesc};
use dg_graph::{Graph, GraphSpecBuilder, NodeSpec};
use serde_json::json;

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

#[test]
fn source_mock_sink_pipeline_runs_end_to_end() {
    let spec = GraphSpecBuilder::new()
        .add_node(NodeSpec {
            name: "source".to_string(),
            kind: "source".to_string(),
            template: None,
            params: json!({
                "count": 2,
                "shape": [1, 4],
                "start": 3.0
            }),
        })
        .add_node(NodeSpec {
            name: "infer".to_string(),
            kind: "mock_inference".to_string(),
            template: None,
            params: json!({
                "shape": [1, 4],
                "echo_inputs": true
            }),
        })
        .add_node(NodeSpec {
            name: "sink".to_string(),
            kind: "sink".to_string(),
            template: None,
            params: json!({}),
        })
        .connect("source.out -> infer.in")
        .connect("infer.out -> sink.in")
        .build()
        .expect("build pipeline spec");

    let report = Graph::new(spec)
        .expect("build graph")
        .run()
        .expect("run graph");
    let tensors = report.sinks.get("sink").expect("sink outputs");
    assert_eq!(tensors.len(), 2);
    let first_bytes = tensors[0].buffer().read_bytes();
    let second_bytes = tensors[1].buffer().read_bytes();
    assert_eq!(first_bytes.len(), 16);
    assert_eq!(first_bytes, f32_bytes(&[3.0, 3.0, 3.0, 3.0]));
    assert_eq!(second_bytes, f32_bytes(&[4.0, 4.0, 4.0, 4.0]));
}

#[test]
fn injected_input_mock_sink_pipeline_runs_end_to_end() {
    let spec = GraphSpecBuilder::new()
        .add_node(NodeSpec {
            name: "input".to_string(),
            kind: "input".to_string(),
            template: None,
            params: json!({}),
        })
        .add_node(NodeSpec {
            name: "infer".to_string(),
            kind: "mock_inference".to_string(),
            template: None,
            params: json!({
                "shape": [1, 4],
                "echo_inputs": true
            }),
        })
        .add_node(NodeSpec {
            name: "sink".to_string(),
            kind: "sink".to_string(),
            template: None,
            params: json!({}),
        })
        .connect("input.out -> infer.in")
        .connect("infer.out -> sink.in")
        .build()
        .expect("build injected input spec");

    let device = CpuDevice::new();
    let desc = TensorDesc::new(
        Shape::new([1, 4]),
        DataType::F32,
        DataFormat::NC,
        DeviceKind::Cpu,
    );
    let tensor = Tensor::allocate(&device, desc).expect("allocate injected tensor");
    tensor
        .buffer()
        .write_from_slice(&f32_bytes(&[1.0, 2.0, 3.0, 4.0]))
        .expect("write injected tensor");

    let report = Graph::new(spec)
        .expect("build graph")
        .run_with_inputs(HashMap::from([("input".to_string(), vec![tensor])]))
        .expect("run graph with input");
    let tensors = report.sinks.get("sink").expect("sink outputs");
    assert_eq!(tensors.len(), 1);
    assert_eq!(
        tensors[0].buffer().read_bytes(),
        f32_bytes(&[1.0, 2.0, 3.0, 4.0])
    );
}
