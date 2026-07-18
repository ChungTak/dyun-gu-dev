# 006 执行状态 — **In Progress**

## 审计基线

| Field | Value |
|---|---|
| 计划创建基线 | `main@f0230e946dc05561d830581df277b79aadb1b807` |
| 正式执行基线 | `main@015eb5642972c9e474bcb74b4b513c610865236f` |
| 范围 | core/runtime/scheduler/graph/media/stream/elements/capi/cli |
| GraphSpec | 保持 `dg/v1`，资源语义安全收紧 |
| C ABI 目标 | v2；v1 立即停止发布 |
| 默认门禁 | fmt/clippy/workspace tests/deny 全绿（见 `ADMISSION_BASELINE.md`） |
| 状态 | CORE6-05 PR #18 merged, CORE6-06 PR #19 冲突已解决 |

## CORE6 状态

| ID | 状态 | PR/Commit | Evidence | Blocker |
|---|---|---|---|---|
| CORE6-01 | Done | PR #14 | `ADMISSION_BASELINE.md`, `CORE_RISK_REGISTER.md`, `crates/dg-core/tests/core6_baseline.rs`, `crates/dg-graph/tests/core6_baseline.rs`; CI 15/15 green | - |
| CORE6-02 | Done | PR #15 | `crates/dg-core/src/resource.rs`, `crates/dg-graph/src/spec.rs`, `crates/dg-graph/src/engine.rs`, `crates/dg-runtime/src/runtime.rs`; boundary tests in `core6_resource_policy.rs`; CI 15/15 green | - |
| CORE6-03 | Done | PR #16 | `crates/dg-core/src/buffer.rs`, `crates/dg-core/src/tensor.rs`, `crates/dg-core/src/shape.rs`, `crates/dg-core/src/memory.rs`, `crates/dg-core/src/device.rs`; 更新的跨 crate `read_bytes`/`allocate_host` 调用；`core6_baseline.rs` 回归通过 | - |
| CORE6-04 | Done | PR #17 | `dg-runtime/src/metrics.rs`, `dg-runtime/src/runtime.rs`, `dg-runtime/src/backend.rs`, `dg-runtime/src/mock.rs`, `dg-scheduler/src/lib.rs`, `dg-graph/src/inference.rs`; 新增 `core6_runtime_scheduler.rs`; CI 15/15 green | - |
| CORE6-05 | Done | PR #18 | `dg-graph` lifecycle/budget fixes, `core6_graph_execution.rs`; local gates green | - |
| CORE6-06 | Done | PR #19 | `ReceiveOutcome`/`recv_timeout` in `dg-stream`, `StreamPullElement` 100ms poll, bridge `MediaInfo`/track-id propagation, `core6_stream_io.rs`, `core6_media_bridge.rs`; local gates green | - |
| CORE6-07 | Not Started | - | - | 依赖 CORE6-02/03 |
| CORE6-08 | Not Started | - | - | 破坏性 ABI v2，需原子切换 |
| CORE6-09 | Not Started | - | - | 依赖 CORE6-04/05/06 |
| CORE6-10 | Not Started | - | - | 依赖前述实现 |
| CORE6-11 | Not Started | - | - | 所有 gate 完成后执行 |

## 风险摘要

| 等级 | Open | Reproduced | In Progress | Closed | Exception |
|---|---:|---:|---:|---:|---:|
| P0 | 4 | 0 | 1 | 1 | 0 |
| P1 | 7 | 0 | 0 | 7 | 0 |
| P2 | 2 | 0 | 0 | 0 | 0 |

统计必须与 `CORE_RISK_REGISTER.md` 同步更新。

## 状态更新规则

- `In Progress` 必须有 branch/PR 和 owner。
- `Done` 必须满足对应章节全部完成条件并引用自动测试/运行证据。
- P0/P1 未关闭时 CORE6-11 不得 Done，acceptance 不得 Accepted。
- compile-only、mock 或人工日志不能关闭真实协议/硬件证据。
- 每次更新记录源码 SHA、Cargo.lock hash、制品 digest 和 artifact URL。
- Accepted Exception 仅限 P2，且必须写到 acceptance 的例外表。
