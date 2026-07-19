# 007 执行状态 — **Not Started**

## 审计基线

| 字段 | 值 |
|---|---|
| 计划创建日期 | 2026-07-19 |
| 计划创建基线 | `main@feddd3add23ec8647f91b61fd3c15837342b790a` |
| 当前审计 HEAD | `f6d6cb06e07b8dde332ed585a8250207501898dc` |
| 工作树 | clean |
| 范围 | core product-ready closure；不新增 vendor capability |
| GraphSpec | 保持 `dg/v1`，process policy 为可信外层 |
| C ABI | 首次 Accepted v2 前完成 view/runtime/artifact 合同 |
| 当前决定 | Plan 6 acceptance Pending；Plan 7 未开始 |

## CORE7 状态

| ID | 状态 | PR/Commit | Evidence | Blocker |
|---|---|---|---|---|
| CORE7-01 | In Progress | devin/1784469684-core7-01-audit | `PLAN6_GAP_MATRIX.md` 已复核；基线脚本 `tools/core7_baseline.sh` | devin |
| CORE7-02 | In Progress | devin/1784470201-core7-02-policy | `ProcessRuntimePolicy` + serde, Graph/Runtime/CLI bootstrap tests pass | R7-001 |
| CORE7-03 | In Progress | devin/1784473631-core7-03-bounded-model | `ModelSource::load_bounded`, Runtime pre-load, backend bounded reads | R7-002/R7-003 |
| CORE7-04 | In Progress | devin/1784473631-core7-04-cancel | CancelReport diagnostics, ExecutionMode, capability contract tests | R7-005 |
| CORE7-05 | In Progress | devin/1784473631-core7-05-isolation | ErrorScope + fatal selection; readiness reflects graph status/root cause | R7-006/R7-007 |
| CORE7-06 | In Progress | devin/1784473631-core7-06-stream-deadline | Cheetah adapter uses tokio::time::timeout; no Handle::block_on or detached thread | R7-004、UP7-001 |
| CORE7-07 | In Progress | devin/1784473631-core7-07-cabi-v2-abi-version | DgAbiVersion struct replaces C-string ABI version; header/examples regenerated | R7-008 |
| CORE7-08 | In Progress | devin/1784473631-core7-08-fuzz-reload-cleanup | `reload-transitions` fuzz cleanup with stop + finite destroy retry | R7-009/R7-010 |
| CORE7-09 | In Progress | devin/1784473631-core7-09-soak-driver | tools/soak.sh supports candidate/spec/baseline/profile + machine summary | R7-011、fixed runner |
| CORE7-10 | In Progress | devin/1784473631-core7-10-release-package | release.yml package produces .so.2 + symlink + C examples + pkg-config + manifest; docs/support-matrix.md added | R7-012 |
| CORE7-11 | Not Started | - | `CORE7_PRODUCT_ACCEPTANCE.md` Pending | 前置 CORE7 全部 |

## 风险摘要

| 等级 | Open | Reproduced | In Progress | Mitigated | Closed | Exception |
|---|---:|---:|---:|---:|---:|---:|
| P0 | 8 | 0 | 0 | 0 | 0 | 0 |
| P1 | 4 | 0 | 0 | 0 | 0 | 0 |
| P2 | 1 | 0 | 0 | 0 | 0 | 0 |

Capability：3 项 Blocked，不计入核心 risk 数量。

## 状态更新规则

- Not Started → Reproduced：失败测试/最小 corpus 已合入。
- In Progress：必须有真实 owner、branch/PR 和目标 risk。
- Done：全部章节完成条件满足，修复已在 main，证据引用可访问。
- Blocked 只用于外部 capability/runner；不能隐藏仍可实现的软件工作。
- CORE7-11 只有 acceptance Accepted 后才能 Done。
- 每次候选更新记录 source SHA、Cargo.lock、header/library、artifact digest 和 workflow URL。

