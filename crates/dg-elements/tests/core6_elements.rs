use std::collections::HashMap;

use dg_core::{
    Buffer, BufferDesc, DataFormat, DataType, DeviceKind, ExternalDropGuard, ExternalHandle,
    MemoryDomain, Shape, Tensor, TensorDesc,
};
use dg_graph::{Graph, GraphSpecBuilder, NodeSpec};
use serde_json::json;

use dg_elements as _;

fn external_only_u8_tensor() -> Tensor {
    let shape = Shape::new([1, 3, 2, 4]);
    let desc = TensorDesc::new(shape, DataType::U8, DataFormat::NCHW, DeviceKind::Cpu);
    let size = desc.storage_bytes().expect("storage bytes");
    let buffer = Buffer::from_external(
        DeviceKind::Cpu,
        MemoryDomain::Host,
        BufferDesc::new(size, 1),
        ExternalHandle::none(),
        ExternalDropGuard::new(|| {}),
    )
    .expect("external buffer");
    assert!(!buffer.is_host_readable());
    Tensor::from_buffer(desc, buffer).expect("tensor from external buffer")
}

#[test]
fn preprocess_rejects_external_only_tensor() {
    let spec = GraphSpecBuilder::new()
        .add_node(NodeSpec {
            name: "input".to_string(),
            kind: "input".to_string(),
            template: None,
            params: json!({}),
            ..NodeSpec::default()
        })
        .add_node(NodeSpec {
            name: "pre".to_string(),
            kind: "yolo_preprocess".to_string(),
            template: None,
            params: json!({"input_width": 4, "input_height": 4}),
            ..NodeSpec::default()
        })
        .add_node(NodeSpec {
            name: "sink".to_string(),
            kind: "sink".to_string(),
            template: None,
            params: json!({}),
            ..NodeSpec::default()
        })
        .connect("input.out -> pre.in")
        .connect("pre.out -> sink.in")
        .build()
        .expect("build graph");

    let graph = Graph::new(spec).expect("build graph");
    let err = graph
        .run_with_inputs(HashMap::from([(
            "input".to_string(),
            vec![external_only_u8_tensor()],
        )]))
        .expect_err("external-only tensor must be rejected");
    assert!(err.to_string().contains("host-readable"), "{err}");
}

#[test]
fn postprocess_drops_non_finite_frame_and_continues() {
    // Bad frame: NaN in model output → FrameLocal drop; graph finishes cleanly.
    let mut bad_values = [0.0_f32; 6];
    bad_values[4] = f32::NAN;
    let bad_bytes: Vec<u8> = bad_values.iter().flat_map(|v| v.to_ne_bytes()).collect();
    let desc = TensorDesc::new(
        Shape::new([1, 6, 1]),
        DataType::F32,
        DataFormat::NC,
        DeviceKind::Cpu,
    );
    let bad = Tensor::allocate(&dg_core::CpuDevice::new(), desc).expect("allocate");
    bad.buffer().write_from_slice(&bad_bytes).expect("write");

    let spec = GraphSpecBuilder::new()
        .add_node(NodeSpec {
            name: "input".to_string(),
            kind: "input".to_string(),
            template: None,
            params: json!({}),
            ..NodeSpec::default()
        })
        .add_node(NodeSpec {
            name: "infer".to_string(),
            kind: "mock_inference".to_string(),
            template: None,
            params: json!({
                "shape": [1, 6, 1],
                "output_shape": [1, 6, 1],
                "echo_inputs": true
            }),
            ..NodeSpec::default()
        })
        .add_node(NodeSpec {
            name: "post".to_string(),
            kind: "yolo_postprocess".to_string(),
            template: None,
            params: json!({
                "input_width": 4,
                "input_height": 4,
                "class_count": 1,
                "confidence_threshold": 0.2,
                "nms_threshold": 0.4
            }),
            ..NodeSpec::default()
        })
        .add_node(NodeSpec {
            name: "sink".to_string(),
            kind: "sink".to_string(),
            template: None,
            params: json!({}),
            ..NodeSpec::default()
        })
        .connect("input.out -> infer.in")
        .connect("infer.out -> post.in")
        .connect("post.out -> sink.in")
        .build()
        .expect("build graph");

    let graph = Graph::new(spec).expect("build graph");
    let report = graph
        .run_with_inputs(HashMap::from([("input".to_string(), vec![bad])]))
        .expect("NaN output must be frame-local and must not fail the graph");
    let post_metrics = report.element_metrics.get("post").expect("post metrics");
    assert!(
        post_metrics.drop_count >= 1,
        "NaN frame should be counted as a frame-local drop: {post_metrics:?}"
    );
    let sink_packets = report.sinks.get("sink").cloned().unwrap_or_default();
    assert!(
        sink_packets.is_empty(),
        "dropped NaN frame must not produce sink detections"
    );
}

fn resnet_input_tensor() -> Tensor {
    let shape = Shape::new([1, 3]);
    let desc = TensorDesc::new(shape, DataType::F32, DataFormat::NC, DeviceKind::Cpu);
    let tensor = Tensor::allocate(&dg_core::CpuDevice::new(), desc).expect("allocate");
    let bytes = [1.0_f32, 3.0, 2.0]
        .iter()
        .flat_map(|v| v.to_ne_bytes())
        .collect::<Vec<_>>();
    tensor.buffer().write_from_slice(&bytes).expect("write");
    tensor
}

#[test]
fn reload_preserves_stateless_element_results() {
    let spec = GraphSpecBuilder::new()
        .add_node(NodeSpec {
            name: "input".to_string(),
            kind: "input".to_string(),
            template: None,
            params: json!({}),
            ..NodeSpec::default()
        })
        .add_node(NodeSpec {
            name: "post".to_string(),
            kind: "resnet_postprocess".to_string(),
            template: None,
            params: json!({"top_k": 2, "labels": ["a", "b", "c"]}),
            ..NodeSpec::default()
        })
        .add_node(NodeSpec {
            name: "sink".to_string(),
            kind: "sink".to_string(),
            template: None,
            params: json!({}),
            ..NodeSpec::default()
        })
        .connect("input.out -> post.in")
        .connect("post.out -> sink.in")
        .build()
        .expect("build graph");

    let mut graph = Graph::new(spec.clone()).expect("build graph");
    for iteration in 0..100 {
        let diff = graph.reload(spec.clone()).expect("reload");
        assert!(
            diff.is_empty(),
            "reload of identical spec must be empty at {iteration}"
        );
        let report = graph
            .run_with_inputs(HashMap::from([(
                "input".to_string(),
                vec![resnet_input_tensor()],
            )]))
            .expect("run graph");
        let results = report
            .classifications
            .get("sink")
            .expect("classification results");
        assert_eq!(results.len(), 2, "iteration {iteration}");
        assert_eq!(results[0].class_id, 1, "iteration {iteration}");
        assert_eq!(results[1].class_id, 2, "iteration {iteration}");
    }
}
