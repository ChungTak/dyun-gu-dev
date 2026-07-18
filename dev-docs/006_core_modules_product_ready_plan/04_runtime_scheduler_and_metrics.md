# 04. Runtime、Scheduler 与聚合指标

> 需求 ID：CORE6-04

## 1. Runtime 执行合同

`Runtime::submit` 的语义按 backend capability 明确：

- native async backend：submit 不等待完成，poll 返回 `Pending/Ready/EndOfStream`；
- sync backend：不得宣称 async；产品 graph 通过受限 blocking adapter 执行，使 worker 仍可观察 deadline；
- backend 若既不能异步也不能在配置 deadline 内结束，capability 标记为 unverified，不进入 product support matrix。

sequence 使用 checked 单调分配；发生空间耗尽且仍有 in-flight 时返回明确错误，不允许 wrapping 后与旧请求冲突。
submit 失败不增加 metrics/in-flight；poll/cancel/error 对每个成功提交恰好完成一次 in-flight。

`InferBackend::cancel` 改为返回 `Result<CancelReport>`，报告 requested/completed/abandoned。Runtime 只有在 backend
确认释放后减少 in-flight；取消失败进入 root cause 和 backend error metrics。

## 2. Pool 聚合指标

创建 inference pool 时先构造一个共享 `Arc<BackendMetrics>`，每个 Runtime 通过
`Runtime::new_with_metrics`/`from_backend_with_metrics` 使用同一实例。Element 只挂这一个聚合 handle，
不再取 `runtimes.first()`。

聚合项包括：

- submissions、in_flight、poll_pending、errors、cancelled；
- queue wait、inference latency；
- H2D、D2H、host copy count/bytes/time；
- pool checkout wait、instance busy、schedule policy。

若保留实例诊断，只允许 `instance_index` 低基数 label，且实例数量受 policy 限制。

## 3. 有界延迟直方图

删除 `Mutex<Vec<u64>>` 原始样本，改为固定 bucket 原子直方图：

`100µs, 250µs, 500µs, 1ms, 2.5ms, 5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s, +Inf`

快照提供 bucket cumulative count、count、sum、max，以及由 bucket 计算的近似 p50/p95/p99。所有 counter
使用 checked/saturating CAS，达到 `u64::MAX` 后保持最大并增加 overflow diagnostic，不允许回绕。

## 4. Scheduler 状态

`Lease` 在 acquire 时保存不可变 `Placement`；`device()`/`core_id()` 不再重新锁 state，也不会因 poison panic。
release 遇 poison 时记录 invariant failure，而不是静默泄漏 load。

Scheduler 和 InstancePool 的 affinity 统一为有限表：

- capacity 不超过 effective worker/stream budget；
- entry 带 last-used，默认 10 分钟 TTL；
- capacity 满时淘汰最久未使用 entry；
- stream 结束、node reload 和 pool drop 主动删除；
- expose entries/evictions/expired 指标，不输出 stream id label。

load 不能用 saturating 运算掩盖 acquire/release 不平衡；overflow/underflow 触发 invariant error 和测试失败。

## 5. Capability 与 backend 边界

- static capability 只做语法 preflight；运行期 readiness 使用 backend live probe。
- `RuntimeOption` 携带 ResourcePolicy、deadline 和 model identity，不允许 backend 绕过。
- backend input/output `TensorInfo` 在 init/reshape 后都执行 rank、stride、physical bytes 和 precision 校验。
- unsupported device/precision/memory 明确失败，不回退其他 backend/device。
- vendor backend 的 sync/cancel/metrics/resource contract 必须通过共享 contract test；真实精度与性能仍走硬件计划。

## 6. 测试

- 2/4/8 实例 pool 分散提交，聚合 snapshot 等于各实例实际总和。
- out-of-order、submit failure、poll error、cancel success/failure、sequence 边界不泄漏 in-flight。
- 100 万 latency 观测后内存保持常量，bucket/percentile 可重复。
- affinity 超 capacity、TTL、stream close、reload 后条目有界且 load 回零。
- poison/fault injection 不 panic；错误进入 root cause 和 metrics。
- 所有 backend feature 至少运行公共 contract test；无 SDK 时明确 blocked，不 success skip。

## 7. 完成条件

- [ ] Runtime 的 sync/async/cancel/sequence 合同可验证。
- [ ] pool 指标覆盖全部实例且 snapshot 内存有界。
- [ ] scheduler affinity、load 和 lease 不因长流或 poison 失控。
- [ ] backend 无法绕过 model/tensor/resource policy。
- [ ] 公共 backend contract test 接入 CI/hardware runner。
