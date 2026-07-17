# dyun-gu-dev

Rust 多芯片推理框架（OpenVINO / TensorRT / RKNN2 / Sophon）。

## 快速开始

```bash
cargo run -p dg-cli -- validate --config examples/mock-multi-algorithm.yaml
cargo run -p dg-cli -- run --config examples/mock-multi-algorithm.yaml
cargo run -p dg-cli -- demo --config examples/mock-multi-stream-demo.yaml
cargo run -p dg-cli -- list-elements
```

Intel 产品构建（OpenVINO + software H.264 + Cheetah 真流）：

```bash
cargo build -p dg-cli --locked --no-default-features --features product-intel
```

默认构建不依赖厂商 SDK，并使用 mock 后端、录制式内存帧和 `mock://` stream
验证图执行、算法后处理与多分支编排。`demo` 命令运行两路 SDK-free mock 流，
并输出由 `ZeroCopyPlanner` 计算的 planned copy count；默认的 `media_decode`
处理录制的原始帧 payload，不是通用压缩码流解析器。真实后端和可选 codec
通过各 crate 的 feature 及对应 SDK/运行时环境启用。

真实压缩编解码请启用同名 **`avcodec-profile-*`** feature，并在图节点里写
`profile`（示例与 feature 对照见 [examples/media/README.md](examples/media/README.md)）。

- [用户指南](docs/user-guide.md)
- [设计方案与里程碑](docs/design.md)
- [媒体 Profile 示例](examples/media/README.md)
- [C API 示例](crates/dg-capi/examples/basic.c)
- [C API 长运行生命周期示例](crates/dg-capi/examples/lifecycle.c)

## 质量门禁

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
