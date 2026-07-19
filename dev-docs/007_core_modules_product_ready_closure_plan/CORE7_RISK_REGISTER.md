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
| R7-001 | P0 | CLI/C bootstrap 未安装可信 process policy，产品路径仍使用 default | Rust/CLI/C 同策略 + init/reload/boundary tests | In Progress | devin |
| R7-002 | P0 | vendor backend 直接读模型，device output policy 未统一 | bounded reader + SDK shim/allocator pre-call tests | In Progress | devin |
| R7-003 | P0 | bridge 按 default 检查并复制，较小 effective frame limit 检查过晚 | copy-before-reject 失败基线转绿，copy count=0 | In Progress | devin |
| R7-004 | P0 | Cheetah timeout 创建 timer thread，上游无原生 deadline | pinned upstream native timeout/close + thread/fd tests | In Progress | devin |
| R7-005 | P1 | vendor sync/cancel capability 与真实可中断性未统一验收 | common contract + capability/support matrix | In Progress | devin |
| R7-006 | P1 | 多个 algorithm 数据错误仍可终止整个 graph | frame-local continuation + fatal classification tests | In Progress | devin |
| R7-007 | P1 | pool/affinity/registry/resource/shutdown metrics 与 readiness 不完整 | ops golden、slow scrape、state matrix | In Progress | devin |
| R7-008 | P0 | C ABI view 未接线、runtime options 空、制品 ABI 未验证 | C11/C++17 + symbol/SONAME/view/fuzz/sanitizer | In Progress | devin |
| R7-009 | P0 | 最近 nightly `reload-transitions` fuzz 失败且当前 SHA 未重跑 | minimized corpus + fix + candidate nightly green | In Progress | devin |
| R7-010 | P0 | 无 Miri、ASan/LSan/TSan 和并发模型 release gate | workflow artifacts 无 report | Open | devin |
| R7-011 | P0 | soak 仅重复 workspace tests，无24h资源/性能证据 | real workload 2h/24h + threshold summary | In Progress | devin |
| R7-012 | P1 | release package 未验证 `.so.2`、symlink、symbols、C examples | unpacked artifact smoke + manifest/rollback | In Progress | devin |
| R7-013 | P2 | Plan 6 status/acceptance SHA 与当前 main 不一致 | Plan 7 handoff、候选身份与状态规则一致 | In Progress | devin |

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

