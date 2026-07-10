#![no_main]

use dg_graph::{GraphFormat, GraphSpec};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = std::str::from_utf8(data) {
        for format in [GraphFormat::Yaml, GraphFormat::Json, GraphFormat::Toml] {
            let _ = GraphSpec::from_str_with_format(input, format);
        }
    }
});
