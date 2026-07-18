use dg_core::ResourcePolicy;
use dg_graph::{ExecutionSpec, GraphFormat, GraphSpec, NodeSpec};
use proptest::prelude::*;
use serde_json::json;

fn spec_with_n_sources(n: usize) -> GraphSpec {
    let mut spec = GraphSpec::default();
    spec.nodes.push(NodeSpec {
        name: "sink".to_string(),
        kind: "sink".to_string(),
        params: json!({}),
        ..NodeSpec::default()
    });
    for i in 0..n {
        spec.nodes.push(NodeSpec {
            name: format!("source_{i}"),
            kind: "source".to_string(),
            params: json!({}),
            ..NodeSpec::default()
        });
    }
    if n > 0 {
        spec.connections.push("source_0.out -> sink.in".to_string());
    }
    spec
}

proptest! {
    #[test]
    fn graph_spec_round_trips_through_all_formats(n in 0usize..8) {
        let spec = spec_with_n_sources(n);

        for format in [GraphFormat::Yaml, GraphFormat::Json, GraphFormat::Toml] {
            let serialized = spec.to_string_with_format(format).expect("serialize");
            let parsed = GraphSpec::from_str_with_format(&serialized, format).expect("parse");
            prop_assert_eq!(parsed.nodes.len(), spec.nodes.len());
            prop_assert_eq!(parsed.connections.len(), spec.connections.len());
        }
    }

    #[test]
    fn node_count_limit_is_enforced(n in 1usize..16) {
        let spec = spec_with_n_sources(n);
        let total_nodes = spec.nodes.len();

        let passes = ResourcePolicy {
            max_nodes: total_nodes,
            ..Default::default()
        };
        prop_assert!(spec.validate_with_policy(&passes).is_ok());

        let fails = ResourcePolicy {
            max_nodes: total_nodes - 1,
            ..Default::default()
        };
        prop_assert!(spec.validate_with_policy(&fails).is_err());
    }

    #[test]
    fn connection_count_limit_is_enforced(n in 1usize..8) {
        let mut spec = spec_with_n_sources(1);
        for i in 1..=n {
            spec.nodes.push(NodeSpec {
                name: format!("extra_{i}"),
                kind: "source".to_string(),
                params: json!({}),
                ..NodeSpec::default()
            });
            spec.nodes.push(NodeSpec {
                name: format!("sink_{i}"),
                kind: "sink".to_string(),
                params: json!({}),
                ..NodeSpec::default()
            });
            spec.connections
                .push(format!("extra_{i}.out -> sink_{i}.in"));
        }

        let spec = GraphSpec {
            execution: ExecutionSpec {
                queue_capacity: 1,
                ..Default::default()
            },
            ..spec
        };

        let passes = ResourcePolicy {
            max_connections: spec.connections.len(),
            ..Default::default()
        };
        prop_assert!(spec.validate_with_policy(&passes).is_ok());

        let fails = ResourcePolicy {
            max_connections: spec.connections.len() - 1,
            ..Default::default()
        };
        prop_assert!(spec.validate_with_policy(&fails).is_err());
    }
}
