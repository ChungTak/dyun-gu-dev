//! CORE6-01 基线失败测试（dg-graph）。
//!
//! 这些测试验证 `GraphSpec` 在字符串入口和 include 解析中未正确执行
//! `ResourceLimits` 的已知缺陷。对应风险关闭后应取消 ignore。

use dg_graph::{GraphFormat, GraphSpec};
use std::fs;
use std::path::{Path, PathBuf};

fn write_graph(dir: &Path, name: &str, max_include_depth: usize, include: Option<&str>) -> PathBuf {
    let mut content = format!(
        "apiVersion: dg/v1\nkind: Graph\nlimits:\n  max_include_depth: {}\n",
        max_include_depth
    );
    if let Some(inc) = include {
        content.push_str(&format!("includes:\n  - {}\n", inc));
    } else {
        content.push_str("includes: []\n");
    }
    content.push_str("nodes: []\nconnections: []\n");
    let path = dir.join(name);
    fs::write(&path, content).expect("write temp graph file");
    path
}

#[test]
#[ignore = "R6-001: CORE6-02 will enforce configured max_config_bytes in from_str_with_format"]
fn from_str_with_format_ignores_max_config_bytes() {
    // The content is clearly larger than the configured 1 byte limit, but
    // GraphSpec::from_str_with_format does not perform the size check.
    let yaml = "apiVersion: dg/v1\nkind: Graph\nlimits:\n  max_config_bytes: 1\nnodes: []\nconnections: []\n";
    let result = GraphSpec::from_str_with_format(yaml, GraphFormat::Yaml);
    assert!(
        result.is_err(),
        "from_str_with_format must reject content exceeding configured max_config_bytes"
    );
}

#[test]
#[ignore = "R6-001: CORE6-02 will honor configured max_include_depth"]
fn load_from_path_ignores_configured_include_depth() {
    let dir = std::env::temp_dir().join(format!("dg-core6-include-depth-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");

    // Build a chain root -> a -> b -> c with configured max_include_depth=2.
    // The implementation currently uses DEFAULT_MAX_INCLUDE_DEPTH (16).
    write_graph(&dir, "root.yaml", 2, Some("a.yaml"));
    write_graph(&dir, "a.yaml", 2, Some("b.yaml"));
    write_graph(&dir, "b.yaml", 2, Some("c.yaml"));
    write_graph(&dir, "c.yaml", 2, None);

    let root = dir.join("root.yaml");
    let result = GraphSpec::load_from_path(&root);
    let _ = fs::remove_dir_all(&dir);

    assert!(
        result.is_err(),
        "load_from_path must fail when include depth exceeds configured max_include_depth"
    );
}
