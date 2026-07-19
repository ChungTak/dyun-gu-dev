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
| 状态 | CORE6-01～11 代码路径 Done；软件 residual 已补齐（MemoryPool、frame limit、hot-update fault injection、hub registry reap、allocate_with_policy、frame-local drop）；R6-002/R6-003 仍 Mitigated（device/网络证据）；acceptance Pending |

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
| CORE6-11 | Done | PR #27 | `CORE_PRODUCT_ACCEPTANCE.md` 已更新候选身份、门禁结果、阻塞项与 Pending 决定；软件 profile 验收通过，硬件/sanitizer/soak 证据待 runner | P0/P1 未全部 Closed，acceptance 不得 Accepted |

## 风险摘要

| 等级 | Open | Reproduced | In Progress | Closed | Exception |
|---|---:|---:|---:|---:|---:|
| P0 | 0 | 0 | 0 | 4 | 0 |
| P1 | 0 | 0 | 0 | 13 | 0 |
| P2 | 0 | 0 | 0 | 3 | 0 |

> 注：R6-002 / R6-003 为 **Mitigated**（软件合同已落地，硬件/网络 soak 证据仍阻塞 Accepted）；R6-018 已以 phase fault injection 关闭。

统计必须与 `CORE_RISK_REGISTER.md` 同步更新。

## 状态更新规则

- `In Progress` 必须有 branch/PR 和 owner。
- `Done` 必须满足对应章节全部完成条件并引用自动测试/运行证据。
- P0/P1 未关闭时 CORE6-11 不得 Done，acceptance 不得 Accepted。
- compile-only、mock 或人工日志不能关闭真实协议/硬件证据。
- 每次更新记录源码 SHA、Cargo.lock hash、制品 digest 和 artifact URL。
- Accepted Exception 仅限 P2，且必须写到 acceptance 的例外表。
