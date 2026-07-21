# Plan 7 核心风险台账

## 1. 状态与关闭规则

状态：`Open / Reproduced / In Progress / Mitigated / Closed / Accepted Exception`。

- Closed：修复已合入 main，自动回归通过，且需要的 sanitizer/soak artifact 可访问。
- Mitigated：临时降低影响但产品合同未满足；仍阻塞核心 acceptance。
- Accepted Exception：只允许 P2，必须有 owner、到期日、监控和关闭条件。
- capability qualification 单独登记，不以 Open 核心风险统计，也不能获得 product support。

## 2. 初始风险

| ID | 等级 | 当前事实 | 目标关闭证据 | 状态 | Owner |
|---|---|---|---|---|---|
| R7-001 | P0 | CLI/C bootstrap 已可安装 process policy；C options 仍缺 deadline/pool 全字段 | Rust/CLI/C 同策略 + init/reload/boundary tests | Mitigated | local |
| R7-002 | P0 | vendor 已用 `load_bounded`；device output 实机未验 | bounded reader + SDK shim/allocator pre-call tests | Mitigated | local |
| R7-003 | P0 | bridge/open_* 预检 + core7_policy_bridge 测试；cheetah 路径仍缺实网 copy_count 证据 | copy-before-reject 失败基线转绿，copy count=0 | Mitigated | local |
| R7-004 | P0 | 产品路径用 `tokio::time::timeout`；上游原生 deadline 与实网仍 CAP | pinned upstream native timeout/close + thread/fd tests | Mitigated | local |
| R7-005 | P1 | vendor sync/cancel capability 与真实可中断性未统一验收 | common contract + capability/support matrix | In Progress | local |
| R7-006 | P1 | 主流算法 element 已 FrameLocal；ResourceLimit 仍 NodeFatal | frame-local continuation + fatal classification tests | Mitigated | local |
| R7-007 | P1 | readiness 加强 + schema v2 聚合 drop/cancel/ready gauges；pool/affinity 细项仍缺 | ops golden、slow scrape、state matrix | Mitigated | local |
| R7-008 | P0 | views 主入口 + package_smoke 脚本；待 CI 首次 green artifact | C11/C++17 + symbol/SONAME/view/fuzz/sanitizer | Mitigated | local |
| R7-009 | P0 | reload-transitions fuzz cleanup 已合入；当前候选 nightly 需重跑 | minimized corpus + fix + candidate nightly green | Mitigated | local |
| R7-010 | P0 | 已加 sanitizers.yml；需 CI runner 实际通过并归档 report | workflow artifacts 无 report | In Progress | local |
| R7-011 | P0 | soak 支持 candidate；nightly 增加 smoke + 2h workspace；24h 仍缺 | real workload 2h/24h + threshold summary | In Progress | local |
| R7-012 | P1 | package_smoke 验证 .so.2/SONAME/symbols/C11；已接 ci/release | unpacked artifact smoke + manifest/rollback | Mitigated | local |
| R7-013 | P2 | Plan 6/7 status 与候选身份文档已同步更新 | Plan 7 handoff、候选身份与状态规则一致 | In Progress | local |

初始统计：P0 8、P1 4、P2 1。

## 3. Capability Qualification

| ID | 范围 | 当前状态 | 必需证据 | Owner |
|---|---|---|---|---|
| CAP7-001 | Cheetah real network | Blocked | RTSP/HTTP-FLV/RTMP/WebRTC fault + soak | Unassigned |
| CAP7-002 | GPU/NPU device allocation/cancel | Blocked | per backend hardware contract + resource curves | Unassigned |
| CAP7-003 | hardware avcodec pre-allocation | Blocked | oversized stream rejection before SDK allocation | Unassigned |

## 4. 执行记录模板

```text
Risk ID:
Owner:
Branch/PR:
Failure baseline:
Root cause:
Chosen fix:
Public compatibility impact:
Tests:
Runtime/sanitizer/soak evidence:
Residual capability qualification:
Reviewer:
Closed commit/date:
```

## 5. 风险变更规则

- 一个 PR 可关闭多个同根因 risk，但每个 risk 保留独立测试和证据引用。
- 软件 risk 不能因“缺设备”转 Accepted Exception；应先关闭软件合同，再把真实设备移入 capability。
- nightly/sanitizer/24h 的失败产生新 risk 或重开相关 risk。
- 当前候选改变后，Closed 代码风险保持 Closed，但 release evidence 必须在新候选重跑。

