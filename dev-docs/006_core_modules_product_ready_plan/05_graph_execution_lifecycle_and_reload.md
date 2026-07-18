# 05. Graph 执行、生命周期与热更新

> 需求 ID：CORE6-05

## 1. Queue 与 worker 预算

- pipeline/task 使用有界 DataPipe；capacity 同时受 GraphSpec 和 process policy。
- sequential 使用带总 packet/bytes 预算的 collector，达到预算立即返回 `ResourceLimit`，不能 Full 自旋。
- `try_recv`、`recv_timeout` 和 send 的 depth 加减必须一一对应；disconnect/error 不留下虚假 depth。
- node `threads` 展开后的总 worker 数不超过 effective `max_nodes`，创建线程失败返回带 node/instance 的错误。
- source、sink、input handle 和 report collector 都必须有 packet/bytes 上限；无限图禁止使用无界内存 sink。

Packet bytes 至少包含 tensor/frame buffer 和结构化结果的估算 owned bytes；共享 Arc 在同一 route fanout 中只计一次
实际内存、但每个 queue slot 仍计入 packet 数。

## 2. Element 处理合同

`ElementIo` 的 recv/send/backpressure poll interval 统一由运行上下文提供，默认 1 ms；外部 I/O 使用 CORE6-06
的最长 100 ms stop 检查切片。

每次非 EOS recv 必须最终调用一次 send、finish 或 drop；`packet_starts` 增加 debug invariant 和最大深度，
防止自定义 element 忘记完成导致内存增长。多输出 fanout 只完成一次输入处理时延。

worker panic 在边界转为 typed runtime error，记录 node/instance/root cause，并请求其他 worker 停止。

## 3. Stop 与 Shutdown

`request_stop` 保持幂等、非阻塞。`shutdown(timeout)` 使用单一绝对 deadline：

1. 状态进入 Draining，readiness false；
2. 通知 element、stream source、backend 和 queue；
3. poll 已完成工作并回收 worker；
4. timeout 时保留未 join worker 和可重试状态；
5. 全部回收后才进入 Stopped。

禁止把 Rust thread detach 当作成功 shutdown。正常 stop、worker error、backend cancel failure、timeout 和 panic
有稳定优先级；首个非取消根因不能被后续 `NotRunning` 覆盖。

## 4. Hot Reload

reload 使用 effective policy 验证完整候选：

`load → normalize → validate → prepare → quiesce affected routes → switch → resume`

- prepare 失败不触碰 live graph；
- drain 使用独立配置 deadline，不能无限等待 upstream；
- route depth、metrics 和未受影响 state 保持；
- switch 失败恢复旧 route/worker；若已跨越不可回滚边界，明确 Failed，不声称旧图仍健康；
- limits 提高超过 hard policy 直接拒绝；limits 降低只约束新资源；
- reload 与 shutdown 竞争时 shutdown 优先，禁止死锁。

修复 route drain 使用 `try_recv` 时的 depth 计数，并验证迁移 packet 不重复、不丢失。

## 5. Graph 状态与 Drop

公开状态保持 `Starting/Running/Reloading/Draining/Stopped/Failed`。status/metrics snapshot 不持有 worker 重锁，
可与 supervisor 并发读取。

Drop 只做兜底 request_stop 和回收已经结束的 worker；产品入口必须显式 shutdown。若 Drop 时仍有 live worker，
记录 fatal diagnostic，不把它作为正常退出路径。

## 6. 测试

- tiny bounded queue、慢 sink、多分支和大 packet 下 backpressure 有界。
- sequential/task 超预算明确失败，不 OOM、不死锁。
- `try_recv`、drain、disconnect 后 queue depth 精确回零。
- 无限 source、网络 pending、backend pending、满输出队列均在 deadline 内停止。
- 连续 100 次 start/stop、reload success/failure 后 worker、queue、sink 和 metrics 回到基线。
- prepare/create/drain/switch/respawn 各故障点注入，验证原子性或 fail-closed。
- SIGTERM 与 reload、metrics scrape、stream reconnect 同时发生时无死锁。

## 7. 完成条件

- [ ] 所有 execution mode、collector、worker 和 route 都有硬预算。
- [ ] shutdown 对 queue、stream 和 backend pending 均有确定 deadline。
- [ ] hot reload 的回滚边界、失败状态和 packet 语义有故障测试。
- [ ] queue/latency/worker 指标在 drain/reload 后准确。
- [ ] 产品入口不依赖 Drop 完成长流回收。
