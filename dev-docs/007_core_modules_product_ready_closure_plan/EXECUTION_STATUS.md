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
| CORE7-03 | Not Started | - | - | R7-002/R7-003 |
| CORE7-04 | Not Started | - | - | R7-005 |
| CORE7-05 | Not Started | - | - | R7-006/R7-007 |
| CORE7-06 | Not Started | - | - | R7-004、UP7-001 |
| CORE7-07 | Not Started | - | - | R7-008 |
| CORE7-08 | Not Started | - | nightly failure run #29674706044 | R7-009/R7-010 |
| CORE7-09 | Not Started | - | - | R7-011、fixed runner |
| CORE7-10 | Not Started | - | - | R7-012 |
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

