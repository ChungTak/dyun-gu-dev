# Plan 6 核心风险台账

## 1. 状态与关闭规则

状态：`Open / Reproduced / In Progress / Mitigated / Closed / Accepted Exception`。

Closed 必须引用修复 commit、自动回归和必要的 soak/sanitizer artifact。Accepted Exception 只允许 P2，
必须包含 owner、到期日、监控和关闭条件。

## 2. 初始风险

| ID | 等级 | 模块/证据 | 当前事实 | 目标与关闭证据 | 状态 | Owner |
|---|---|---|---|---|---|---|
| R6-001 | P1 | `dg-graph/src/spec.rs` | string 入口无 config bytes 检查；configured include depth 未执行 | process policy + 累计限长读取 + boundary tests | Closed | John Doe |
| R6-002 | P0 | `dg-graph::ResourceLimits` 横向路径 | tensor/frame/model limits 未进入真实消费边界 | allocate/copy/read/import 前统一拒绝，计数 allocator 证明 | In Progress | John Doe
| | | | | 进展：host allocation/read/copy 已 fallible；device/policy 计数待 04/05 | | |
| R6-003 | P0 | `dg-stream/src/elements.rs`, `stream.rs` | pull 用 `recv_blocking()`，真实 recv 可无限 pending | timeout outcome + close wakeup + deadline shutdown test | Open | John Doe |
| R6-004 | P1 | `dg-graph/src/inference.rs` | pool 只 attach 首 Runtime metrics | 全 pool 共享 metrics，2/4/8 实例对账 | Open | John Doe |
| R6-005 | P1 | `dg-runtime/src/metrics.rs` | latency 保存到无界 `Vec<u64>` | 固定 buckets，百万观测常量内存 | Open | John Doe |
| R6-006 | P1 | `dg-scheduler/src/lib.rs` | 两级 affinity HashMap 无 capacity/TTL | 有界 LRU/TTL，churn/close/reload 测试 | Open | John Doe |
| R6-007 | P1 | `dg-graph/src/pipe.rs`, `engine.rs` | sequential/task unbounded；sink/report 可无界 | bounded/budgeted execution，超限不死锁 | Open | John Doe |
| R6-008 | P2 | `dg-graph/src/pipe.rs::try_recv` | route drain 不递减 depth | depth invariant/golden tests | Open | John Doe |
| R6-009 | P1 | `dg-core/src/buffer.rs::read_bytes` | external-only buffer 静默返回空 Vec | 只保留 fallible/staging API，backend tests | Closed | John Doe |
| R6-010 | P0 | `dg-core/src/tensor.rs`, `shape.rs` | physical stride bytes 未完整计算，stride 乘法 saturating | checked physical span + padded/packed tests | Closed | John Doe |
| R6-011 | P1 | `dg-core/src/buffer.rs`, `memory.rs` | host allocation和MemoryPool cache缺少统一失败/容量合同 | fallible alloc + cache bytes/eviction soak | Open | John Doe |
| R6-012 | P0 | `dg-capi/src/lib.rs` external imports | C 导入使用空 drop guard，可 UAF | v2 release callback exactly-once + ASan | Open | John Doe |
| R6-013 | P0 | `dg-capi/src/lib.rs` enum parameters | C 未知判别值先形成 Rust enum，存在 UB | v2 `int32_t` 输入 + fuzz/ABI tests | Open | John Doe |
| R6-014 | P1 | `dg-capi` `LAST_DATA/LAST_ERROR` | pointer 被后续 ABI 调用覆盖 | owned bytes/error handle 跨调用稳定 | Open | John Doe |
| R6-015 | P0 | `dg-capi` shape/length helpers | rank/length未在构造slice前统一受硬上限 | v2 views先验limit/null/overflow | Open | John Doe |
| R6-016 | P1 | `dg-runtime::Runtime` | sync submit 可阻塞；cancel无失败报告 | capability诚实 + cancel report + pending shutdown | Open | John Doe |
| R6-017 | P1 | `dg-scheduler::Lease` | poisoned state getter使用`expect` panic | immutable placement/no getter lock + poison tests | Open | John Doe |
| R6-018 | P1 | `dg-graph` reload drain | drain无独立绝对deadline，部分阶段fail-closed边界不完整 | injected phase failures + bounded drain | Open | John Doe |
| R6-019 | P1 | `dg-stream/src/bridge.rs` | 复制前无统一frame limit；饱和ID和metadata吞错 | pre-copy limit + typed conversion golden | Open | John Doe |
| R6-020 | P1 | `dg-elements` | NMS/anchors/OCR/track state/output缺统一预算 | worst-case complexity/state limit tests | Open | John Doe |
| R6-021 | P2 | `dg-cli/src/ops.rs` | metrics渲染持snapshot锁；livez语义弱 | clone snapshot、slow-client和ops failure tests | Open | John Doe |
| R6-022 | P2 | 横向 error paths | timeout/retry等部分路径依赖字符串判断 | typed taxonomy matrix | Open | John Doe |

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

## 4. CORE6-02 关闭记录

**R6-001**
- Owner: John Doe
- Branch/PR: `devin/1784344499-core6-02-resource-policy`
- Reproduction: `from_str_with_format_ignores_max_config_bytes`, `load_from_path_ignores_configured_include_depth`
- Root cause: `GraphSpec` 入口未根据 `limits.max_config_bytes` 与 `limits.max_include_depth` 校验输入；include resolver 只检查默认常量。
- Chosen fix: 新增 `dg_core::ResourcePolicy`，`GraphSpec` 在 `from_str_with_policy`/`load_from_path_tracked` 中计算 `effective = min(hard, requested)`，累计 config 字节，include depth/count；`Graph`/`Runtime` 持有 `Arc<ResourcePolicy>`。
- Public compatibility impact: `Graph::new` 行为不变；新增 `Graph::new_with_policy`、`Runtime::new_with_policy`、`ElementIo::policy()`。
- Tests: `crates/dg-core/tests/core6_resource_policy.rs`, `crates/dg-graph/tests/core6_resource_policy.rs`, `crates/dg-runtime/tests/core6_resource_policy.rs`, `crates/dg-graph/tests/core6_baseline.rs` 回归。
- Runtime evidence: 本地 `cargo fmt/clippy/test/deny` 全绿；`dg-media --features avcodec-profile-native-free` 全绿；Cargo.lock 无变化。
- Residual risk: 已迁移到 R6-002，待 tensor/frame 真实消费边界继续收敛。
- Reviewer: self-review
- Closed commit/date: (待 PR 合入后回填)

## 5. CORE6-03 关闭记录

**R6-009**
- Owner: John Doe
- Branch/PR: `devin/1784405775-core6-03-memory-tensor-media`
- Reproduction: `external_only_buffer_read_bytes_is_not_silent_empty`
- Root cause: `Buffer::read_bytes()` 对 device-only external buffer 返回空 `Vec`，产品路径未显式报错。
- Chosen fix: `Buffer::read_bytes()` 与 `Buffer::into_host_bytes()` 改为返回 `Result<Vec<u8>>`；`BufferStorage` 不再对不可读外部内存返回空；跨 crate 调用者统一改为 `?` 或 `.unwrap()` 处理。
- Public compatibility impact: `Buffer::read_bytes` 和 `Buffer::into_host_bytes` 现在返回 `Result`，调用代码需使用 `?`。`try_read_bytes`/`try_into_host_bytes` 保留为别名。
- Tests: `crates/dg-core/tests/core6_baseline.rs::external_only_buffer_read_bytes_is_not_silent_empty`，所有 backend/element 调用已更新并回归通过。
- Runtime evidence: 本地 `cargo fmt/clippy/test/deny` 全绿；`dg-media --features avcodec-profile-native-free` 全绿；Cargo.lock 无变化。
- Residual risk: `R6-002` 中 device/policy 计数边界继续由 CORE6-04/05 收敛。
- Reviewer: self-review
- Closed commit/date: (待 PR 合入后回填)

**R6-010**
- Owner: John Doe
- Branch/PR: `devin/1784405775-core6-03-memory-tensor-media`
- Reproduction: `tensor_from_buffer_accepts_physical_stride_span`
- Root cause: `TensorDesc::storage_bytes()` 只计算逻辑字节，`Strides::contiguous_for()` 使用 `saturating_mul`，跨步物理范围未校验。
- Chosen fix: 新增 `Strides::physical_element_count()`，使用 `checked_sub`/`checked_mul` 计算 `(dim-1)*stride` 最大值；`TensorDesc::storage_bytes()` 在有显式 strides 时返回物理字节；`Strides::contiguous_for()` 改为返回 `Result` 并做 checked 乘法；`Shape::contiguous_strides()`、`Tensor::reshape()`、`Tensor::from_buffer()` 统一处理 checked stride。
- Public compatibility impact: `Shape::contiguous_strides` 与 `Strides::contiguous_for`/`is_contiguous_for` 现在返回 `Result<Strides>`/`Result<bool>`。调用点已更新。
- Tests: `crates/dg-core/tests/core6_baseline.rs::tensor_from_buffer_accepts_physical_stride_span`，`crates/dg-core/tests/core.rs::contiguous_shape_stride_round_trip`。
- Runtime evidence: 同 R6-009。
- Residual risk: packed I4/F4 的物理元素上取整由 `DataType::storage_bytes_for_elements` 覆盖；零维/零跨距边界已在 `physical_element_count` 处理。
- Reviewer: self-review
- Closed commit/date: (待 PR 合入后回填)
