#![no_main]

use dg_runtime::{configure_backend, BackendConfig};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(value) = serde_json::from_slice(data) else {
        return;
    };
    let config = BackendConfig::new(None, value);
    let _ = configure_backend("mock", config);
});
