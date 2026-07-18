# 10. CI、Fuzz、并发测试与长稳

> 需求 ID：CORE6-10

## 1. PR 必需门禁

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check
git diff --exit-code Cargo.lock
```

另加：

- core limits/ownership/stride contract tests；
- runtime/scheduler/graph concurrency and fault tests；
- stream timeout/close/bridge tests；
- C11/C++17 ABI v2 smoke、header/symbol/SONAME snapshot；
- product feature check，不用 `--all-features` 混淆厂商 SDK。

测试失败、设备缺失或依赖不可用不能 success skip；硬件 job 可标 Blocked，但不得把 Blocked 当 Passed。

## 2. Property、Miri 与 Fuzz

Property test 覆盖 shape/stride/size、GraphSpec limits/include、scheduler acquire/release、media plane、算法参数。

Miri 至少运行：

- `dg-core` buffer/tensor/external guard；
- C ABI 内部 view/owned result helper 的 Rust 测试；
- 无真实 FFI 的 scheduler/graph ownership tests。

Fuzz targets：

- GraphSpec string/file/include manifest；
- C ABI v2 views/runtime options/external descriptor；
- runtime backend options/model metadata；
- tensor shape/stride/quant metadata；
- media metadata/codec config/track conversion；
- reload event/state transitions。

PR 执行 `cargo fuzz check`；nightly 每 target 至少 15 分钟，保存 corpus/crash/minimized artifact。

## 3. Sanitizer 与并发

- ASan/LSan：C ABI harness、external callback、create/run/destroy 循环。
- TSan 或等价工具：stop/status/metrics/reload、scheduler pool、stream close/recv。
- loom/shuttle 类模型测试：小型 queue、stop flag、callback exactly-once 和 acquire/release 状态机。
- panic/poison/failure injection：allocator、thread spawn、backend submit/poll/cancel、connector recv/close、reload switch。

任何 sanitizer report 都是 release blocker；不以“测试仍返回 0”忽略。

## 4. Nightly 2h

固定 2 小时预算执行：

- fuzz run 和 corpus regression；
- 1000 次 start/stop、disconnect/reconnect、reload accept/reject；
- pool 1/2/4/8 实例并发和 metrics 对账；
- stream registry/affinity/cache 高基数 churn；
- metrics 并发 scrape 和慢客户端；
- optional software/OpenVINO CPU feature regression。

保存 RSS、thread、fd、worker、queue、request、affinity、cache 和 callback 曲线。

## 5. Release 24h soak

使用候选制品、固定 GraphSpec/model/stream hash：

- 至少 4 路持续输入，包含 decode/preprocess/infer/postprocess/stream sink；
- 周期性断流、reload、metrics scrape 和 backend delay；
- 结束时 SIGTERM，使用正式 shutdown deadline；
- RSS warmup 后净增长 ≤128 MiB；
- thread/fd/request/worker 在 shutdown 后回到基线，无 callback、queue、affinity、sink、cache 泄漏；
- 无未恢复 error、panic、deadlock、metric counter 回绕或 readiness 假阳性。

产品硬件发布仍需在目标 runner 重跑同一核心 soak；CPU/mock 结果只关闭软件合同。

## 6. 性能门禁

固定 runner/模型/流/配置比较已接纳基线：

- 吞吐下降 ≤10%；
- p95 端到端延迟上升 ≤15%；
- metrics scrape 开启相对关闭的吞吐损失 ≤5%；
- stop/reload/reconnect deadline 100 次全部通过；
- copy count/bytes 不得无解释增加。

超过阈值必须回滚、优化或由 reviewer 批准带到期日的例外。

## 7. 证据

每个 job 保存源码 SHA、Cargo.lock hash、工具链、features、target、命令、时间、exit code、test/skip 数、
artifact URL 和脱敏环境。release evidence 填入 `RELEASE_EVIDENCE_TEMPLATE.md`。

## 8. 完成条件

- [ ] PR 门禁覆盖核心合同与 C ABI v2。
- [ ] Miri、sanitizer、并发模型和 fuzz 实际运行。
- [ ] Nightly 2h 无资源增长或并发失败。
- [ ] Release 24h soak 与性能阈值通过。
- [ ] 所有证据可追溯到同一候选源码和制品。
