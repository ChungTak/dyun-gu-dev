use std::sync::{Arc, Mutex};

use dg_elements::{install_http_push_driver, HttpPushDriver, HttpPushRequest};
use dg_graph::{find_element, Graph, GraphSpecBuilder, NodeSpec, Result};
use serde_json::json;

#[derive(Default)]
struct DriverState {
    requests: Vec<(String, String)>,
    fail: bool,
}

struct RecordingDriver {
    state: Arc<Mutex<DriverState>>,
}

impl HttpPushDriver for RecordingDriver {
    fn post(&self, request: HttpPushRequest) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| dg_graph::Error::Runtime("driver state poisoned".to_string()))?;
        state.requests.push((request.url, request.method));
        if state.fail {
            return Err(dg_graph::Error::Runtime(
                "simulated HTTP failure".to_string(),
            ));
        }
        Ok(())
    }
}

fn node(name: &str, kind: &str, params: serde_json::Value) -> NodeSpec {
    NodeSpec {
        name: name.to_string(),
        kind: kind.to_string(),
        template: None,
        params,
        ..NodeSpec::default()
    }
}

fn graph(url: &str, method: &str) -> Graph {
    let spec = GraphSpecBuilder::new()
        .add_node(node(
            "source",
            "source",
            json!({"count": 2, "shape": [1, 4]}),
        ))
        .add_node(node(
            "push",
            "http_push",
            json!({"url": url, "method": method}),
        ))
        .connect("source.out -> push.in")
        .build()
        .expect("build http_push graph");
    Graph::new(spec).expect("create http_push graph")
}

#[test]
fn http_push_is_registered_and_validates_url_and_fields() {
    assert!(find_element("http_push").is_some());

    for (params, expected) in [
        (json!({"url": "ftp://example.test"}), "http:// or https://"),
        (
            json!({"url": "https://example.test", "unknown": true}),
            "unknown field",
        ),
        (json!({"url": 1}), "field url must be"),
    ] {
        let error = GraphSpecBuilder::new()
            .add_node(node("push", "http_push", params))
            .build()
            .expect_err("invalid http_push config must fail at load time");
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn http_push_driver_receives_packets_and_reports_failures() {
    let state = Arc::new(Mutex::new(DriverState::default()));
    install_http_push_driver(Box::new(RecordingDriver {
        state: state.clone(),
    }))
    .expect("install recording driver");

    graph("https://example.test/events", "post")
        .run()
        .expect("successful HTTP push");
    {
        let state = state.lock().expect("driver state");
        assert_eq!(state.requests.len(), 2);
        assert_eq!(
            state.requests[0],
            (
                "https://example.test/events".to_string(),
                "POST".to_string()
            )
        );
    }

    state.lock().expect("driver state").fail = true;
    let error = graph("http://example.test/fail", "PUT")
        .run()
        .expect_err("driver failure must fail graph execution");
    let message = error.to_string();
    assert!(message.contains("push"), "{message}");
    assert!(message.contains("http://example.test/fail"), "{message}");
    assert!(message.contains("simulated HTTP failure"), "{message}");
}
