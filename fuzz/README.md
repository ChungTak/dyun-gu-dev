# Fuzz targets

This directory is an independent Cargo workspace and is excluded from the
repository workspace. It therefore does not participate in stable workspace
`fmt`, `clippy`, `test`, or cross-target checks.

Install `cargo-fuzz` and use a nightly toolchain:

```text
cargo install cargo-fuzz
```

From the repository root:

```text
cargo +nightly fuzz check graph-spec
cargo +nightly fuzz check capi-load-string
cargo +nightly fuzz check runtime-backend-options
cargo +nightly fuzz run graph-spec
cargo +nightly fuzz run capi-load-string
cargo +nightly fuzz run runtime-backend-options
```

`graph-spec` exercises YAML, JSON, and TOML configuration parsing. The
`capi-load-string` target exercises the C ABI's arbitrary NUL-terminated input
boundary and verifies that malformed input does not panic or cross the ABI.
`runtime-backend-options` exercises the SDK-free mock backend's JSON model and
tensor metadata option parser. The repository currently has no SDK-free media
container/bitstream parser or model-header parser; those paths are delegated to
optional codec/vendor libraries and are not fuzzed by this target.
