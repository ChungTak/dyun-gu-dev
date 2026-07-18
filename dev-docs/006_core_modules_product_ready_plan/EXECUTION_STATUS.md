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
| 状态 | CORE6-01 基线审计已合入；CORE6-02 ResourcePolicy 实现中 |

## CORE6 状态

| ID | 状态 | PR/Commit | Evidence | Blocker |
|---|---|---|---|---|
| CORE6-01 | Done | PR #14 | `ADMISSION_BASELINE.md`, `CORE_RISK_REGISTER.md`, `crates/dg-core/tests/core6_baseline.rs`, `crates/dg-graph/tests/core6_baseline.rs`; CI 15/15 green | - |
| CORE6-02 | In Progress | `devin/1784344499-core6-02-resource-policy` | `crates/dg-core/src/resource.rs`, `crates/dg-graph/src/spec.rs`, `crates/dg-graph/src/engine.rs`, `crates/dg-runtime/src/runtime.rs`; boundary tests in `core6_resource_policy.rs` | 待 PR 创建/CI 通过后 Done |
| CORE6-03 | Not Started | - | - | 依赖 CORE6-02 |
| CORE6-04 | Not Started | - | - | 依赖 policy/metrics snapshot 设计 |
| CORE6-05 | Not Started | - | - | 依赖 CORE6-02/04 |
| CORE6-06 | Not Started | - | - | Cheetah timeout 能力需审计 |
| CORE6-07 | Not Started | - | - | 依赖 CORE6-02/03 |
| CORE6-08 | Not Started | - | - | 破坏性 ABI v2，需原子切换 |
| CORE6-09 | Not Started | - | - | 依赖 CORE6-04/05/06 |
| CORE6-10 | Not Started | - | - | 依赖前述实现 |
| CORE6-11 | Not Started | - | - | 所有 gate 完成后执行 |

## 风险摘要

| 等级 | Open | Reproduced | In Progress | Closed | Exception |
|---|---:|---:|---:|---:|---:|
| P0 | 5 | 0 | 0 | 1 | 0 |
| P1 | 10 | 1 | 0 | 1 | 0 |
| P2 | 3 | 0 | 0 | 0 | 0 |

统计必须与 `CORE_RISK_REGISTER.md` 同步更新。

## 状态更新规则

- `In Progress` 必须有 branch/PR 和 owner。
- `Done` 必须满足对应章节全部完成条件并引用自动测试/运行证据。
- P0/P1 未关闭时 CORE6-11 不得 Done，acceptance 不得 Accepted。
- compile-only、mock 或人工日志不能关闭真实协议/硬件证据。
- 每次更新记录源码 SHA、Cargo.lock hash、制品 digest 和 artifact URL。
- Accepted Exception 仅限 P2，且必须写到 acceptance 的例外表。
