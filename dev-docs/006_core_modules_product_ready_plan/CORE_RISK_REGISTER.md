# Plan 6 核心风险台账

## 1. 状态与关闭规则

状态：`Open / Reproduced / In Progress / Mitigated / Closed / Accepted Exception`。

Closed 必须引用修复 commit、自动回归和必要的 soak/sanitizer artifact。Accepted Exception 只允许 P2，
必须包含 owner、到期日、监控和关闭条件。

## 2. 初始风险

| ID | 等级 | 模块/证据 | 当前事实 | 目标与关闭证据 | 状态 |
|---|---|---|---|---|---|
| R6-001 | P1 | `dg-graph/src/spec.rs` | string 入口无 config bytes 检查；configured include depth 未执行 | process policy + 累计限长读取 + boundary tests | Open |
| R6-002 | P0 | `dg-graph::ResourceLimits` 横向路径 | tensor/frame/model limits 未进入真实消费边界 | allocate/copy/read/import 前统一拒绝，计数 allocator 证明 | Open |
| R6-003 | P0 | `dg-stream/src/elements.rs`, `stream.rs` | pull 用 `recv_blocking()`，真实 recv 可无限 pending | timeout outcome + close wakeup + deadline shutdown test | Open |
| R6-004 | P1 | `dg-graph/src/inference.rs` | pool 只 attach 首 Runtime metrics | 全 pool 共享 metrics，2/4/8 实例对账 | Open |
| R6-005 | P1 | `dg-runtime/src/metrics.rs` | latency 保存到无界 `Vec<u64>` | 固定 buckets，百万观测常量内存 | Open |
| R6-006 | P1 | `dg-scheduler/src/lib.rs` | 两级 affinity HashMap 无 capacity/TTL | 有界 LRU/TTL，churn/close/reload 测试 | Open |
| R6-007 | P1 | `dg-graph/src/pipe.rs`, `engine.rs` | sequential/task unbounded；sink/report 可无界 | bounded/budgeted execution，超限不死锁 | Open |
| R6-008 | P2 | `dg-graph/src/pipe.rs::try_recv` | route drain 不递减 depth | depth invariant/golden tests | Open |
| R6-009 | P1 | `dg-core/src/buffer.rs::read_bytes` | external-only buffer 静默返回空 Vec | 只保留 fallible/staging API，backend tests | Open |
| R6-010 | P0 | `dg-core/src/tensor.rs`, `shape.rs` | physical stride bytes 未完整计算，stride 乘法 saturating | checked physical span + padded/packed tests | Open |
| R6-011 | P1 | `dg-core/src/buffer.rs`, `memory.rs` | host allocation和MemoryPool cache缺少统一失败/容量合同 | fallible alloc + cache bytes/eviction soak | Open |
| R6-012 | P0 | `dg-capi/src/lib.rs` external imports | C 导入使用空 drop guard，可 UAF | v2 release callback exactly-once + ASan | Open |
| R6-013 | P0 | `dg-capi/src/lib.rs` enum parameters | C 未知判别值先形成 Rust enum，存在 UB | v2 `int32_t` 输入 + fuzz/ABI tests | Open |
| R6-014 | P1 | `dg-capi` `LAST_DATA/LAST_ERROR` | pointer 被后续 ABI 调用覆盖 | owned bytes/error handle 跨调用稳定 | Open |
| R6-015 | P0 | `dg-capi` shape/length helpers | rank/length未在构造slice前统一受硬上限 | v2 views先验limit/null/overflow | Open |
| R6-016 | P1 | `dg-runtime::Runtime` | sync submit 可阻塞；cancel无失败报告 | capability诚实 + cancel report + pending shutdown | Open |
| R6-017 | P1 | `dg-scheduler::Lease` | poisoned state getter使用`expect` panic | immutable placement/no getter lock + poison tests | Open |
| R6-018 | P1 | `dg-graph` reload drain | drain无独立绝对deadline，部分阶段fail-closed边界不完整 | injected phase failures + bounded drain | Open |
| R6-019 | P1 | `dg-stream/src/bridge.rs` | 复制前无统一frame limit；饱和ID和metadata吞错 | pre-copy limit + typed conversion golden | Open |
| R6-020 | P1 | `dg-elements` | NMS/anchors/OCR/track state/output缺统一预算 | worst-case complexity/state limit tests | Open |
| R6-021 | P2 | `dg-cli/src/ops.rs` | metrics渲染持snapshot锁；livez语义弱 | clone snapshot、slow-client和ops failure tests | Open |
| R6-022 | P2 | 横向 error paths | timeout/retry等部分路径依赖字符串判断 | typed taxonomy matrix | Open |

## 3. 执行记录模板

```text
Risk ID:
Owner:
Branch/PR:
Reproduction:
Root cause:
Chosen fix:
Public compatibility impact:
Tests:
Runtime evidence:
Residual risk:
Reviewer:
Closed commit/date:
```

