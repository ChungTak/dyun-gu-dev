use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");

    if env::var("CARGO_CFG_TARGET_OS").ok().as_deref() == Some("linux") {
        println!("cargo:rustc-link-arg=-Wl,-soname,libdg_capi.so.2");
    }

    let crate_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let config = cbindgen::Config::from_file(crate_dir.join("cbindgen.toml"))
        .expect("load cbindgen configuration");
    let bindings =
        cbindgen::generate_with_config(&crate_dir, config).expect("generate dg-capi header");
    let header = crate_dir.join("include/dg_capi.h");
    bindings.write_to_file(header);
}
