# 本地 avcodec-rs 调试约定

上游问题在本机树 `/dataset/datavol/workspace/media_server/avcodec-rs` 修改与验证。

## Workspace patch（可选本地调试）

上游修复已提交：`avcodec-rs` `f3c1c04`。生产 pin 仍为 RC2 `20684324`，直到打 tag 后原子升级。

临时验证修复时可在 dyun `Cargo.toml` 加：

```toml
[patch."https://github.com/TimothyWalker6922/avcodec-rs-develop.git"]
avcodec = { path = "../avcodec-rs/crates/sdk/avcodec" }
```

- 保留 manifest `rev = 20684324`（`dependency_contract` 仍检查 pin 字符串）。
- **勿将 `[patch]` 合入 production main**；验证后删除并 pin 新 commit。

## 验证命令

```bash
source scripts/env-software-avcodec.sh

# 上游
cd ../avcodec-rs
cargo test -p avcodec-codec-ffmpeg --lib
cargo test -p avcodec --features profile-software --test v3_facade_contract

# dyun（带 patch）
cd ../dyun-gu-dev
cargo test -p dg-media --features avcodec-profile-software --test media_pipeline
cargo test -p dg-media --features avcodec-profile-native-free,avcodec-profile-software
```

## UP4-002 修复摘要

| 项 | 内容 |
|---|---|
| FFmpeg version gate | libavcodec 58/59 启用；≥63 best-effort |
| Software JPEG | allow-list `ffmpeg,jpeg` + feature `jpeg` |
| 仍依赖 | 运行时 `libx264`（H.264 encode） |
