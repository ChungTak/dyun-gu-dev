//! CORE6-02 资源策略回归测试（dg-graph）。
//!
//! 这些测试验证 `GraphSpec` 的字符串入口与 include 解析在 CORE6-02 之后
//! 正确执行 `ResourcePolicy` 与 `ResourceLimits` 的下限。

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
fn from_str_with_format_enforces_max_config_bytes() {
    let yaml = "apiVersion: dg/v1\nkind: Graph\nlimits:\n  max_config_bytes: 1\nnodes: []\nconnections: []\n";
    let result = GraphSpec::from_str_with_format(yaml, GraphFormat::Yaml);
    assert!(
        result.is_err(),
        "from_str_with_format must reject content exceeding configured max_config_bytes"
    );
}

#[test]
fn from_str_with_format_rejects_oversized_input_before_parse() {
    // The default max_config_bytes is 8 MiB; a 9 MiB string must be rejected
    // before serde is asked to parse it.
    let huge = "x".repeat(9 * 1024 * 1024);
    let result = GraphSpec::from_str_with_format(&huge, GraphFormat::Yaml);
    let err = result.expect_err("oversized input should be rejected before parse");
    assert!(
        err.to_string().contains("config string size"),
        "error should mention config string size: {err}"
    );
}

#[test]
fn load_from_path_enforces_configured_include_depth() {
    let dir = std::env::temp_dir().join(format!("dg-core6-include-depth-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");

    // Build a chain root -> a -> b -> c with configured max_include_depth=2.
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
