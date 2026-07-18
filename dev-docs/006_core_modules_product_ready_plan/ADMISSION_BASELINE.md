# CORE6-01 接纳基线

> 本文件记录 Plan 6 启动时的实际源码、工具链与门禁结果，供后续风险关闭与 release 证据对账。

## 审计基线

| 字段 | 值 |
|---|---|
| 审计日期 | 2026-07-18 |
| 源码 HEAD | `015eb5642972c9e474bcb74b4b513c610865236f` |
| 分支 | `main` |
| 工作树 | clean |
| Rust 工具链 | `1.94.1 (e408947bf 2026-03-25)` |
| 默认 host | `x86_64-unknown-linux-gnu` |
| Cargo 版本 | `1.94.1 (29ea6fb6a 2026-03-24)` |
| Cargo.lock SHA-256 | `a8e90170594e0ae54295eb6fbf45433fc255e65bed57c5ffa07b29c7b890bb87` |
| GraphSpec | `dg/v1` |
| C ABI 目标 | v2（本轮未切换） |

## 基础门禁结果

| 门禁 | 命令 | 结果 |
|---|---|---|
| fmt | `cargo fmt --all -- --check` | 通过 |
| clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | 通过 |
| tests (workspace) | `cargo test --workspace --locked` | 通过 |
| tests (dg-media native-free) | `cargo test -p dg-media --locked --features avcodec-profile-native-free` | 通过（59 + 3 + 8 + 6 = 76 tests） |
| deny | `cargo deny check` | 通过（仅有允许的 duplicate 警告） |
| lockfile 漂移 | `git diff --exit-code Cargo.lock` | 无漂移 |

## 模块审计摘要

基于 `CORE_RISK_REGISTER.md` 中 22 条初始风险的输入面、资源面、并发面、FFI 面与失败面审计，
已在同一目录风险台账中记录 owner 与复现位置。P0/P1 风险的最小失败测试见：

- `crates/dg-core/tests/core6_baseline.rs`（R6-009、R6-010）
- `crates/dg-graph/tests/core6_baseline.rs`（R6-001）

本轮未修复任何风险，仅保存基线与失败测试，后续 CORE6-02~11 按顺序关闭。
