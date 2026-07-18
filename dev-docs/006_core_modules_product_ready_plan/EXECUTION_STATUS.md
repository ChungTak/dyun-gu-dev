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
| 状态 | CORE6-05 PR #18 merged, CORE6-06 PR #19 merged, CORE6-07 PR #20 merged, CORE6-08 PR #22-24 merged, CORE6-09 PR #25 merged, CORE6-10 PR #26 已创建 |

## CORE6 状态

| ID | 状态 | PR/Commit | Evidence | Blocker |
|---|---|---|---|---|
| CORE6-01 | Done | PR #14 | `ADMISSION_BASELINE.md`, `CORE_RISK_REGISTER.md`, `crates/dg-core/tests/core6_baseline.rs`, `crates/dg-graph/tests/core6_baseline.rs`; CI 15/15 green | - |
| CORE6-02 | Done | PR #15 | `crates/dg-core/src/resource.rs`, `crates/dg-graph/src/spec.rs`, `crates/dg-graph/src/engine.rs`, `crates/dg-runtime/src/runtime.rs`; boundary tests in `core6_resource_policy.rs`; CI 15/15 green | - |
| CORE6-03 | Done | PR #16 | `crates/dg-core/src/buffer.rs`, `crates/dg-core/src/tensor.rs`, `crates/dg-core/src/shape.rs`, `crates/dg-core/src/memory.rs`, `crates/dg-core/src/device.rs`; 更新的跨 crate `read_bytes`/`allocate_host` 调用；`core6_baseline.rs` 回归通过 | - |
| CORE6-04 | Done | PR #17 | `dg-runtime/src/metrics.rs`, `dg-runtime/src/runtime.rs`, `dg-runtime/src/backend.rs`, `dg-runtime/src/mock.rs`, `dg-scheduler/src/lib.rs`, `dg-graph/src/inference.rs`; 新增 `core6_runtime_scheduler.rs`; CI 15/15 green | - |
| CORE6-05 | Done | PR #18 | `dg-graph` lifecycle/budget fixes, `core6_graph_execution.rs`; local gates green | - |
| CORE6-06 | Done | PR #19 | `ReceiveOutcome`/`recv_timeout` in `dg-stream`, `StreamPullElement` 100ms poll, bridge `MediaInfo`/track-id propagation, `core6_stream_io.rs`, `core6_media_bridge.rs`; local gates green | - |
| CORE6-07 | Done | PR #20 | `dg-elements` 算法边界/NMS/top-k/PPOCR/ByteTrack/OSD/distributor-converger 预算，`dg-media` OSD 硬上限与外部 buffer 检查，`core6_elements.rs` 外部 tensor/非有限/100 reload 测试；local gates green |
| CORE6-08 | Done | PR #22 (split 1/3), PR #23 (split 2/3), PR #24 (split 3/3) | v2 `DgExternalMemoryV2` 与 `DgReleaseCallback`；FD 导入自动 dup 并由库 close；raw 导入要求非空 release callback 且只调用一次；`dg_engine_destroy(timeout_ms, out_error)` 替代 `dg_engine_free`，超时返回 `Busy` 并可重试；cbindgen header、ABI snapshot、C examples、`docs/user-guide.md` 已同步；本地 fmt/clippy/test/deny 全绿 | - |
| CORE6-09 | Done | PR #25 | typed error taxonomy in `dg-core`/`dg-graph`/`dg-capi`; `dg-cli` ops health/metrics boundedness; local fmt/clippy/test/deny 全绿 | - |
| CORE6-10 | Done | PR #26 | fuzz 目标、property/concurrency 测试、`Strides::physical_element_count` 物理跨度修正、`.github/workflows/nightly.yml` + `tools/soak.sh` 长稳基础设施；本地 fmt/clippy/test/deny/Cargo.lock 全绿；fuzz workspace 编译通过 | - |
| CORE6-11 | Not Started | - | - | 所有 gate 完成后执行 |

## 风险摘要

| 等级 | Open | Reproduced | In Progress | Closed | Exception |
|---|---:|---:|---:|---:|---:|
| P0 | 0 | 0 | 2 | 4 | 0 |
| P1 | 1 | 0 | 2 | 10 | 0 |
| P2 | 0 | 0 | 0 | 3 | 0 |

统计必须与 `CORE_RISK_REGISTER.md` 同步更新。

## 状态更新规则

- `In Progress` 必须有 branch/PR 和 owner。
- `Done` 必须满足对应章节全部完成条件并引用自动测试/运行证据。
- P0/P1 未关闭时 CORE6-11 不得 Done，acceptance 不得 Accepted。
- compile-only、mock 或人工日志不能关闭真实协议/硬件证据。
- 每次更新记录源码 SHA、Cargo.lock hash、制品 digest 和 artifact URL。
- Accepted Exception 仅限 P2，且必须写到 acceptance 的例外表。
