# 09. Nightly、24h Soak、性能与证据

> 需求 ID：CORE7-09

## 1. Soak 驱动

替换“循环执行 workspace tests”的伪长稳。`tools/soak.sh` 只做编排，实际驱动必须运行候选二进制/动态库和
确定性的长流 workload：

- 4 路以上持续输入；
- decode/preprocess/infer/postprocess/sink 的 SDK-free 或 CPU 路径；
- 周期性 stream disconnect/reconnect；
- 合法/非法 reload；
- backend delay/pending/cancel；
- 并发 metrics scrape；
- 结束时 SIGTERM 和正式 shutdown deadline。

提供 `--duration`、`--artifact-dir`、`--profile`、`--candidate`、`--baseline`。release evidence 不得只保存
测试 stdout。

## 2. 资源采样

固定间隔保存：

- RSS/VmSize；
- thread/fd/task/worker；
- queue packets/bytes；
- backend submitted/in-flight/pending/cancel；
- pool cache、affinity、registry、subscriber、sink/collector；
- external callback acquired/released；
- readiness、root cause、reload/reconnect；
- throughput 和 latency histogram。

每份样本带 monotonic timestamp；运行结束生成 JSON summary 和机器可读 pass/fail，不靠人工看图决定。

## 3. Nightly 2h

在固定 CPU runner 执行：

- 全 fuzz corpus regression 和每 target 15 分钟；
- 1000 次 start/stop、reload accept/reject、disconnect/reconnect；
- pool 1/2/4/8 实例 metrics 对账；
- stream/cache/affinity 高基数 churn；
- 2h core workload 和慢 metrics client；
- Miri/sanitizer 可并行独立 job。

当前 SHA 的首次 green 只能关闭基础设施风险；release 仍需候选制品 24h。

## 4. Release 24h

- 使用不可变候选 artifact，不在工作树临时编译后直接运行；
- 固定 GraphSpec/model/stream/runtime-policy hash；
- warmup 后 RSS 净增长 ≤128 MiB；
- shutdown 后 thread/fd/worker/request/callback 回到基线；
- queue/cache/affinity/registry/sink 无单调无界增长；
- 无 panic、deadlock、未恢复 error、counter wrap 或 readiness 假阳性；
- 任何采样中断、runner reboot 或 artifact mismatch 均判失败并重跑完整 24h。

## 5. 性能

同一 runner、配置、模型和采样方法比较 Accepted baseline：

| 指标 | 门禁 |
|---|---:|
| throughput | 下降 ≤10% |
| p95 latency | 上升 ≤15% |
| metrics scrape overhead | throughput 损失 ≤5% |
| stop/reload/reconnect | 100次全部在 deadline |
| copy count/bytes | 不得无解释增加 |

没有 Accepted baseline 时先冻结首个 reference candidate，不能在看到候选结果后选择更差的 baseline。

## 6. Capability Soak

真实 Cheetah/device runner 复用相同 evidence schema，但结论只更新对应 support matrix 行。CPU/mock 结果不能
关闭真实网络、device allocator、zero-copy、精度或厂商 cancel 资格。

## 7. 完成条件

- [ ] soak 驱动真实长流而非重复测试套件。
- [ ] 资源与性能曲线机器可判定并绑定候选 artifact。
- [ ] 2h nightly 与 24h release 均在候选 SHA 通过。
- [ ] 性能阈值、shutdown 和资源回零全部满足。
- [ ] capability evidence 与 core evidence 分层保存。

