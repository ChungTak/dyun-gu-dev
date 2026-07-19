//! CORE6-02 Graph-level ResourcePolicy boundary tests.
#![allow(clippy::field_reassign_with_default)]

use std::fs;
use std::path::{Path, PathBuf};

use dg_core::ResourcePolicy;
use dg_graph::{Graph, GraphSpec, GraphSpecBuilder, NodeSpec, ParallelType};
use serde_json::json;

fn source(name: &str) -> NodeSpec {
    NodeSpec {
        name: name.into(),
        kind: "source".into(),
        params: json!({"count": 1, "shape": [1, 4]}),
        ..NodeSpec::default()
    }
}

fn sink(name: &str) -> NodeSpec {
    NodeSpec {
        name: name.into(),
        kind: "sink".into(),
        params: json!({}),
        ..NodeSpec::default()
    }
}

#[test]
fn graph_new_with_policy_rejects_requested_above_hard() {
    let mut spec = GraphSpec::default();
    spec.limits.max_nodes = 1024; // requested limit
    spec.nodes.push(source("a"));

    let hard = ResourcePolicy {
        max_nodes: 2,
        ..ResourcePolicy::default()
    };
    let err = Graph::new_with_policy(spec, hard)
        .err()
        .expect("should fail");
    assert!(err.to_string().contains("max_nodes"), "{err}");
}

#[test]
fn graph_new_enforces_node_count() {
    let mut spec = GraphSpec::default();
    spec.limits.max_nodes = 2;
    spec.nodes.push(source("a"));
    spec.nodes.push(source("b"));
    spec.nodes.push(source("c"));

    let err = Graph::new(spec)
        .err()
        .expect("node count should exceed limit");
    assert!(err.to_string().contains("node count"), "{err}");
}

#[test]
fn graph_new_enforces_queue_capacity_against_connections() {
    let mut spec = GraphSpec::default();
    spec.limits.max_connections = 1;
    spec.execution.parallel = ParallelType::Pipeline;
    spec.execution.queue_capacity = 4;
    spec.nodes.push(source("src"));
    spec.nodes.push(sink("snk"));
    spec.connections.push("src.out -> snk.in".into());

    let err = Graph::new(spec)
        .err()
        .expect("queue capacity should exceed connection limit");
    assert!(err.to_string().contains("queue_capacity"), "{err}");
}

#[test]
fn graph_reload_lower_limit_passes() {
    let mut base = GraphSpec::default();
    base.limits.max_nodes = 5;
    base.nodes.push(source("a"));
    base.nodes.push(source("b"));
    base.nodes.push(source("c"));

    let mut graph = Graph::new(base).expect("valid base");

    let mut lowered = GraphSpec::default();
    lowered.limits.max_nodes = 3;
    lowered.nodes = graph.spec().nodes.clone();

    graph
        .reload(lowered)
        .expect("lowering limit should succeed");
}

#[test]
fn graph_reload_raise_limit_fails() {
    let mut base = GraphSpec::default();
    base.limits.max_nodes = 5;
    base.nodes.push(source("a"));
    base.nodes.push(source("b"));
    base.nodes.push(source("c"));

    let mut graph = Graph::new_with_policy(base, ResourcePolicy::default()).expect("valid base");

    let mut raised = GraphSpec::default();
    raised.limits.max_nodes = 2048; // above default hard max_nodes
    raised.nodes = graph.spec().nodes.clone();

    let err = graph
        .reload(raised)
        .expect_err("raising above hard limit should fail");
    assert!(err.to_string().contains("max_nodes"), "{err}");
}

fn temp_dir(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{}-{}", prefix, std::process::id()))
}

fn write_graph_file(dir: &Path, name: &str, include: Option<&str>) -> PathBuf {
    let _ = fs::create_dir_all(dir);
    let mut content = "apiVersion: dg/v1\nkind: Graph\nnodes: []\nconnections: []\n".to_string();
    if let Some(inc) = include {
        content.push_str(&format!("includes:\n  - {}\n", inc));
    }
    let path = dir.join(name);
    fs::write(&path, content).expect("write temp graph");
    path
}

fn write_graph_with_config_limit(
    dir: &Path,
    name: &str,
    include: Option<&str>,
    max_config_bytes: usize,
) -> PathBuf {
    let _ = fs::create_dir_all(dir);
    let mut content = format!(
        "apiVersion: dg/v1\nkind: Graph\nlimits:\n  max_config_bytes: {}\nnodes: []\nconnections: []\n",
        max_config_bytes
    );
    if let Some(inc) = include {
        content.push_str(&format!("includes:\n  - {}\n", inc));
    }
    let path = dir.join(name);
    fs::write(&path, content).expect("write graph");
    path
}

#[test]
fn load_from_path_with_policy_rejects_cumulative_config_bytes() {
    let dir = temp_dir("dg-core6-cumulative");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");

    // Write files without an in-file config limit first so we can measure sizes.
    let _ = write_graph_file(&dir, "root.yaml", Some("child.yaml"));
    let _ = write_graph_file(&dir, "child.yaml", None);
    let root_size = fs::metadata(dir.join("root.yaml")).unwrap().len() as usize;
    let child_size = fs::metadata(dir.join("child.yaml")).unwrap().len() as usize;
    let budget = root_size + child_size - 1;
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");

    // Re-write with the budget embedded; each file fits, but the sum does not.
    let _ = write_graph_with_config_limit(&dir, "root.yaml", Some("child.yaml"), budget);
    let _ = write_graph_with_config_limit(&dir, "child.yaml", None, budget);

    let policy = ResourcePolicy {
        max_config_bytes: budget,
        ..ResourcePolicy::default()
    };
    let root = dir.join("root.yaml");
    let err = GraphSpec::load_from_path_with_policy(&root, policy)
        .expect_err("cumulative bytes should exceed limit");
    assert!(err.to_string().contains("cumulative config bytes"), "{err}");

    let _ = fs::remove_dir_all(&dir);
}

fn write_limited_graph(
    dir: &Path,
    name: &str,
    include: Option<&str>,
    max_include_count: usize,
) -> PathBuf {
    let _ = fs::create_dir_all(dir);
    let mut content = format!(
        "apiVersion: dg/v1\nkind: Graph\nlimits:\n  max_include_count: {}\nnodes: []\nconnections: []\n",
        max_include_count
    );
    if let Some(inc) = include {
        content.push_str(&format!("includes:\n  - {}\n", inc));
    }
    let path = dir.join(name);
    fs::write(&path, content).expect("write graph");
    path
}

#[test]
fn load_from_path_with_policy_rejects_include_count() {
    let dir = temp_dir("dg-core6-include-count");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");

    // Root and children all tighten max_include_count to 1; root includes two files.
    let _ = write_limited_graph(&dir, "root.yaml", None, 1);
    let mut root = fs::read_to_string(dir.join("root.yaml")).expect("read root");
    root.push_str("includes:\n  - a.yaml\n  - b.yaml\n");
    fs::write(dir.join("root.yaml"), root).expect("write root");
    let _ = write_limited_graph(&dir, "a.yaml", None, 1);
    let _ = write_limited_graph(&dir, "b.yaml", None, 1);

    let policy = ResourcePolicy {
        max_include_count: 1,
        max_include_depth: 16,
        ..ResourcePolicy::default()
    };
    let root = dir.join("root.yaml");
    let err = GraphSpec::load_from_path_with_policy(&root, policy)
        .expect_err("include count should exceed limit");
    assert!(err.to_string().contains("include count"), "{err}");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn graph_spec_builder_respects_policy_limits() {
    let spec = GraphSpecBuilder::new()
        .api_version("dg/v1")
        .add_node(source("a"))
        .add_node(source("b"))
        .build()
        .expect("build");

    let hard = ResourcePolicy {
        max_nodes: 1,
        ..ResourcePolicy::default()
    };
    let err = Graph::new_with_policy(spec, hard)
        .err()
        .expect("should fail due to node limit");
    assert!(
        err.to_string().contains("node count") || err.to_string().contains("max_nodes"),
        "{err}"
    );
}

#[test]
fn load_from_path_rejects_directory_traversal_includes() {
    let dir = temp_dir("dg-core6-traversal");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");

    // Root graph lives one level below `dir`; the include escapes the graph base.
    let graph_dir = dir.join("graph");
    fs::create_dir_all(&graph_dir).expect("create graph dir");
    let escape_dir = dir.join("escape");
    fs::create_dir_all(&escape_dir).expect("create escape dir");

    let secret = escape_dir.join("secret.yaml");
    fs::write(
        &secret,
        "apiVersion: dg/v1\nkind: Graph\nnodes: []\nconnections: []\n",
    )
    .expect("write secret");

    let mut root = "apiVersion: dg/v1\nkind: Graph\nnodes: []\nconnections: []\n".to_string();
    root.push_str("includes:\n  - ../escape/secret.yaml\n");
    let root_path = graph_dir.join("root.yaml");
    fs::write(&root_path, root).expect("write root");

    let err = GraphSpec::load_from_path_with_policy(&root_path, ResourcePolicy::default())
        .expect_err("directory traversal should be rejected");
    assert!(
        err.to_string().contains("outside the graph base directory"),
        "{err}"
    );

    let _ = fs::remove_dir_all(&dir);
}

fn inference_node(name: &str, model: &std::path::Path) -> NodeSpec {
    NodeSpec {
        name: name.into(),
        kind: "inference".into(),
        params: json!({"backend": "mock", "model": model}),
        ..NodeSpec::default()
    }
}

#[test]
fn graph_new_enforces_model_size_against_max_model_bytes() {
    let dir = temp_dir("dg-core6-model-size");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");

    let model_path = dir.join("model.bin");
    fs::write(&model_path, vec![0u8; 128]).expect("write model file");

    let mut spec = GraphSpec::default();
    spec.limits.max_model_bytes = 1;
    spec.nodes.push(source("src"));
    spec.nodes.push(inference_node("infer", &model_path));
    spec.nodes.push(sink("snk"));
    spec.connections.push("src.out -> infer.in".into());
    spec.connections.push("infer.out -> snk.in".into());

    let err = Graph::new(spec).err().expect("model size should exceed limit");
    assert!(err.to_string().contains("model bytes"), "{err}");

    let _ = fs::remove_dir_all(&dir);
}
