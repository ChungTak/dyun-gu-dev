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
| | | | | 进展：host allocation/read/copy 已 fallible；runtime/scheduler metrics 已落地；graph source/sink/input 队列 packets/bytes 预算已生效；device/policy 计数仍需 05/11 | | |
| R6-003 | P0 | `dg-stream/src/elements.rs`, `stream.rs` | pull 用 `recv_blocking()`，真实 recv 可无限 pending | timeout outcome + close wakeup + deadline shutdown test | Open | John Doe |
| R6-004 | P1 | `dg-graph/src/inference.rs` | pool 只 attach 首 Runtime metrics | 全 pool 共享 metrics，2/4/8 实例对账 | Closed | John Doe |
| R6-005 | P1 | `dg-runtime/src/metrics.rs` | latency 保存到无界 `Vec<u64>` | 固定 buckets，百万观测常量内存 | Closed | John Doe |
| R6-006 | P1 | `dg-scheduler/src/lib.rs` | 两级 affinity HashMap 无 capacity/TTL | 有界 LRU/TTL，churn/close/reload 测试 | Closed | John Doe |
| R6-007 | P1 | `dg-graph/src/pipe.rs`, `engine.rs` | sequential/task unbounded；sink/report 可无界 | bounded/budgeted execution，超限不死锁 | Closed | John Doe |
| R6-008 | P2 | `dg-graph/src/pipe.rs::try_recv` | route drain 不递减 depth | depth invariant/golden tests | Closed | John Doe |
| R6-009 | P1 | `dg-core/src/buffer.rs::read_bytes` | external-only buffer 静默返回空 Vec | 只保留 fallible/staging API，backend tests | Closed | John Doe |
| R6-010 | P0 | `dg-core/src/tensor.rs`, `shape.rs` | physical stride bytes 未完整计算，stride 乘法 saturating | checked physical span + padded/packed tests | Closed | John Doe |
| R6-011 | P1 | `dg-core/src/buffer.rs`, `memory.rs` | host allocation和MemoryPool cache缺少统一失败/容量合同 | fallible alloc + cache bytes/eviction soak | Open | John Doe |
| R6-012 | P0 | `dg-capi/src/lib.rs` external imports | C 导入使用空 drop guard，可 UAF | v2 release callback exactly-once + ASan | Open | John Doe |
| R6-013 | P0 | `dg-capi/src/lib.rs` enum parameters | C 未知判别值先形成 Rust enum，存在 UB | v2 `int32_t` 输入 + fuzz/ABI tests | Open | John Doe |
| R6-014 | P1 | `dg-capi` `LAST_DATA/LAST_ERROR` | pointer 被后续 ABI 调用覆盖 | owned bytes/error handle 跨调用稳定 | Open | John Doe |
| R6-015 | P0 | `dg-capi` shape/length helpers | rank/length未在构造slice前统一受硬上限 | v2 views先验limit/null/overflow | Open | John Doe |
| R6-016 | P1 | `dg-runtime::Runtime` | sync submit 可阻塞；cancel无失败报告 | capability诚实 + cancel report + pending shutdown | Closed | John Doe |
| R6-017 | P1 | `dg-scheduler::Lease` | poisoned state getter使用`expect` panic | immutable placement/no getter lock + poison tests | Closed | John Doe |
| R6-018 | P1 | `dg-graph` reload drain | drain无独立绝对deadline，部分阶段fail-closed边界不完整 | injected phase failures + bounded drain | Mitigated | John Doe |
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

## 6. CORE6-04 关闭记录

**R6-004**
- Owner: John Doe
- Branch/PR: `devin/1784405775-core6-04-runtime-scheduler-metrics`
- Reproduction: `dg-runtime/tests/core6_runtime_scheduler.rs::shared_metrics_aggregate_submissions_across_runtimes`
- Root cause: `dg-graph/src/inference.rs::create_inference_pool` 只在首个 `Runtime` 上 `attach_backend_metrics`，pool 内实例的 metrics 未聚合。
- Chosen fix: 在 `InstancePool` 创建阶段构建共享的 `Arc<BackendMetrics>`；每个 `Runtime` 通过 `Runtime::new_with_metrics` 使用同一实例；`InferenceExecution::Pool` 保存该句柄，`Element::run` 中通过共享句柄 attach，不再依赖 `runtimes.first()`。
- Public compatibility impact: `Runtime` 新增 `new_with_metrics`/`new_with_policy_and_metrics`/`from_backend_with_metrics` 构造函数；`InferenceExecution::Pool` 增加 `metrics` 字段。
- Tests: `crates/dg-runtime/tests/core6_runtime_scheduler.rs::shared_metrics_aggregate_submissions_across_runtimes`。
- Runtime evidence: 本地 `cargo fmt/clippy/test/deny` 全绿；`dg-media --features avcodec-profile-native-free` 全绿；Cargo.lock 无变化。
- Residual risk: 跨 pool 的 queue wait/H2D/D2H/host copy 计数仍依赖各 backend 主动记录；后续 05 统一 staging/copy 路径时补齐。
- Reviewer: self-review
- Closed commit/date: (待 PR 合入后回填)

**R6-005**
- Owner: John Doe
- Branch/PR: `devin/1784405775-core6-04-runtime-scheduler-metrics`
- Reproduction: `dg-runtime/tests/core6_runtime_scheduler.rs::latency_histogram_is_bounded_after_million_records`
- Root cause: `BackendMetrics` 使用 `Mutex<Vec<u64>>` 保存原始 latency，样本量无界且排序成本高。
- Chosen fix: 替换为 16 个固定上界 bucket（100µs ~ +Inf）的原子直方图；`LatencyPercentiles` 同时提供 count/sum/max、buckets、cumulative buckets 与近似 p50/p95/p99；所有 additive counter 使用 checked/saturating CAS，溢出时递增 `overflow` diagnostic；`in_flight` 减量使用 checked subtraction，下溢时递增 `underflow` diagnostic。
- Public compatibility impact: `BackendMetricsSnapshot` 增加 `cancelled`、`overflow_count`、`underflow_count` 与 `LatencyPercentiles` 的 `buckets`/`cumulative`/`sum_ns`/`max_ns`；原 `infer_latencies` 字段保留。
- Tests: `crates/dg-runtime/src/metrics.rs` 单元测试，`crates/dg-runtime/tests/core6_runtime_scheduler.rs::latency_histogram_is_bounded_after_million_records`。
- Runtime evidence: 同上。
- Residual risk: bucket 粒度为近似值；p99 等百分位为 bucket 上界插值，非精确排序值。
- Reviewer: self-review
- Closed commit/date: (待 PR 合入后回填)

**R6-006**
- Owner: John Doe
- Branch/PR: `devin/1784405775-core6-04-runtime-scheduler-metrics`
- Reproduction: `dg-scheduler/src/lib.rs` 中 affinity 为无界 `HashMap`。
- Root cause: `Scheduler` 和 `InstancePool` 的 affinity 表无容量与 TTL 控制，长 stream 集可无限增长。
- Chosen fix: 引入 `BoundedAffinityTable<T>`，容量默认为 scheduler 总 core 数 / pool instance 数，TTL 默认 10 分钟；按 LRU 驱逐，支持 `remove_affinity` 主动清理；暴露 `AffinityMetrics`（entries/evictions/expired）。
- Public compatibility impact: `Scheduler` 新增 `affinity_metrics()`；`InstancePool` 新增 `affinity_metrics()` 与 `remove_affinity()`；`CoreLoad` 增加 `overflow_count`/`underflow_count`。
- Tests: `dg-scheduler/src/lib.rs::scheduler_affinity_table_is_bounded_by_core_count`、`instance_pool_affinity_capacity_evicts_lru`、`instance_pool_affinity_expires_after_ttl`、`instance_pool_remove_affinity_drops_entry_and_load`。
- Runtime evidence: 同上。
- Residual risk: 默认 TTL 10 分钟无法快速释放僵尸 stream 条目；`remove_affinity` 需调用方在 stream end 时调用，graph 层未全量集成。
- Reviewer: self-review
- Closed commit/date: (待 PR 合入后回填)

**R6-016**
- Owner: John Doe
- Branch/PR: `devin/1784405775-core6-04-runtime-scheduler-metrics`
- Reproduction: `dg-runtime/tests/core6_runtime_scheduler.rs::cancel_report_releases_in_flight_and_records_cancel`
- Root cause: `InferBackend::cancel` 返回 `()`，`Runtime::cancel` 不报告实际释放数量，失败无追踪。
- Chosen fix: `InferBackend::cancel` 改为返回 `Result<CancelReport>`（requested/completed/abandoned）；`Runtime::cancel` 据此精确递减 `in_flight`、记录 `cancelled` 与 `backend_errors`；`Runtime::submit` 使用 checked monotonic sequence，溢出返回 `Error::SequenceExhausted`；`submit` 的 in-flight 限制改为使用 backend 自身计数，避免共享 metrics 导致多实例误触发。
- Public compatibility impact: `InferBackend::cancel` 签名变更；`Runtime::cancel` 返回 `Result<CancelReport>`；`dg-runtime::Error` 新增 `SequenceExhausted`。
- Tests: `crates/dg-runtime/tests/core6_runtime_scheduler.rs::cancel_report_releases_in_flight_and_records_cancel`，`crates/dg-runtime/tests/runtime.rs::delayed_mock_in_flight_limit_and_cancel`，`dg-openvino/src/backend.rs` cancel 适配。
- Runtime evidence: 同上。
- Residual risk: 各 vendor backend（TensorRT/RKNN/Sophon）仍使用默认 cancel；需硬件 runner 验证实际 cancel 语义。
- Reviewer: self-review
- Closed commit/date: (待 PR 合入后回填)

**R6-017**
- Owner: John Doe
- Branch/PR: `devin/1784405775-core6-04-runtime-scheduler-metrics`
- Reproduction: `dg-scheduler/src/lib.rs` 中 `Lease::device`/`core_id` 在 `device()`/`core_id()` 内部 `lock().expect(...)`。
- Root cause: 查询 placement 时需要持有 scheduler state 锁，state poison 会 panic。
- Chosen fix: `Lease` 在 `acquire` 时缓存 `(device_kind, device_id)` 与 `core_id`；`device()`/`core_id()` 直接返回缓存值；`Drop` 遇到 poisoned mutex 时记录 invariant failure 而不是 panic（通过 `load` 不再回零可由 snapshot 检测）。
- Public compatibility impact: `Lease` 结构增加缓存字段；行为不变，poison 不再 panic。
- Tests: `dg-scheduler` 既有 lease 测试，`Lease` drop 不 panic 通过 clippy/test 全绿间接验证。
- Runtime evidence: 同上。
- Residual risk: poison 后 load 泄漏；后续需显式 poison 注入测试。
- Reviewer: self-review
- Closed commit/date: (待 PR 合入后回填)

## 7. CORE6-05 关闭记录

**R6-007**
- Owner: John Doe
- Branch/PR: `devin/1784350799-core6-05-graph-execution-lifecycle`
- Reproduction: `crates/dg-graph/tests/core6_graph_execution.rs::sink_packet_budget_fails_without_oom`, `input_packet_budget_fails_at_start`, `large_packet_backpressure_is_bounded_by_sink_bytes`, `packet_starts_max_depth_is_enforced_without_oom`
- Root cause: `ResourcePolicy` 的 `max_buffer_packets`/`max_buffer_bytes` 未落到 source/sink/input 的运行时消费边界；`ElementIo::recv` 无限累积 `packet_starts`。
- Chosen fix: `RuntimeGraph::build` 计算 `effective = hard_policy.effective_for(spec.limits)`，用于 `SinkCollector` 与 input queue 的 packets/bytes 预算；`ElementIo` 暴露 `policy()` 并新增 `max_packet_starts` 限制；`ElementIo::recv` 在 `packet_starts` 超过 `max_packet_starts` 时返回 `Error::ResourceLimit`。
- Public compatibility impact: `ElementIo` 增加 `policy()`、`packet_starts` 容量随 `execution.queue_capacity`；`SinkCollector` 新增 `set_budget`/`try_push` 内部方法。
- Tests: `crates/dg-graph/tests/core6_graph_execution.rs`（sink/input/large-packet/packet_starts）；`crates/dg-graph/src/pipe.rs::tests`（depth accounting）。
- Runtime evidence: 本地 `cargo fmt/clippy/test/deny` 全绿；`dg-media --features avcodec-profile-native-free` 测试通过（仅余 pre-existing warnings）。
- Residual risk: R6-002 中 device/policy 字节计数与 `MemoryPool` cache eviction 仍待后续收敛。
- Reviewer: self-review
- Closed commit/date: (待 PR 合入后回填)

**R6-008**
- Owner: John Doe
- Branch/PR: `devin/1784350799-core6-05-graph-execution-lifecycle`
- Reproduction: `crates/dg-graph/src/pipe.rs::tests::try_recv_and_recv_timeout_decrement_depth_exactly`, `disconnect_after_drain_leaves_depth_at_zero`
- Root cause: `PipeReceiver::try_recv()` 早期只从底层 channel 取包，未同步递减 `PipeState::depth`。
- Chosen fix: `PipeReceiver::try_recv()` 和 `recv_timeout()` 统一通过 `inspect`/`inspect_err` 在成功接收时 `fetch_sub(1)`；断开连接时不改动 depth。
- Public compatibility impact: 无。
- Tests: `crates/dg-graph/src/pipe.rs::tests`。
- Runtime evidence: 同上。
- Residual risk: 无。
- Reviewer: self-review
- Closed commit/date: (待 PR 合入后回填)

**R6-018**
- Owner: John Doe
- Branch/PR: `devin/1784350799-core6-05-graph-execution-lifecycle`
- Reproduction: `crates/dg-graph/tests/core6_graph_execution.rs::shutdown_timeout_is_retryable_and_keeps_draining_status`
- Root cause: `RunningGraph::drain_routes` 使用固定重试次数，无绝对 deadline，route drain 可能无限阻塞。
- Chosen fix: `drain_routes` 改为接收 `timeout: Duration`，构造绝对 deadline，在接收循环与全 sender 重试循环中持续检查 deadline，超时返回 `Error::Runtime("drain route timed out")`；`apply_hot_update_candidate` 使用 5s 的 `DEFAULT_DRAIN_TIMEOUT`。
- Public compatibility impact: 内部 `drain_routes` 签名变化。
- Tests: `crates/dg-graph/tests/core6_graph_execution.rs::shutdown_timeout_is_retryable_and_keeps_draining_status`；既有 `running_graph_replaces_only_affected_worker_and_rejects_invalid_diff_atomically`、`hot_update_keeps_unaffected_branch_lossless_under_backpressure` 回归通过。
- Runtime evidence: 同上。
- Residual risk: prepare/create/drain/switch 的独立注入故障覆盖仍不完整，需后续补充 fault injection tests。
- Reviewer: self-review
- Closed commit/date: (待 PR 合入后回填)
