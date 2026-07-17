# 03. RunningGraph 生命周期与 CLI Supervisor

## 1. 生命周期状态

新增公开状态 `Starting/Running/Draining/Stopped/Failed`，并为 `RunningGraph` 提供：

- `request_stop(&self)`：幂等、非阻塞地发出协作取消；
- `shutdown(&mut self, timeout)`：等待 worker/driver 回收，超时保留可重试状态；
- `status(&self)`：返回状态与首个根因；
- `metrics_snapshot(&self)`：运行中读取指标；
- `finish(self)`：有限图兼容入口。

Drop 只做安全兜底：发停止信号、回收已经结束的 worker；生产代码必须显式 shutdown。不得尝试强杀 Rust 线程。

## 2. 可取消 element 合同

所有 recv/send/poll/connect/drain 都必须在有限超时后检查 stop。禁止无限阻塞的 socket、condvar、SDK poll。
媒体 drain timeout、stream recv timeout 和 inference poll interval 统一由运行时 limits 约束。

## 3. CLI Supervisor

`dg run` 改为 supervisor 持有 RunningGraph、watch handle、ops server 和信号接收器：

1. 加载并 preflight；
2. 初始化内置 connector/backend；
3. start graph；
4. readiness 变为 true；
5. 处理 reload、worker failure、SIGINT/SIGTERM；
6. readiness false，drain，shutdown，输出最终报告与退出码。

根因错误优先于后续取消错误；正常 SIGTERM 返回 0，配置错误返回 2，运行错误返回 3，强制超时返回 4。

## 4. 测试

- 无限 mock source 可被 stop 并在 deadline 内 join；
- 两次 request_stop 幂等；
- worker panic 转换为 Failed 且其他 worker 停止；
- sink/backpressure 阻塞时仍可退出；
- SIGTERM 集成测试验证进程、线程、socket 和临时资源回收。

## 5. 完成条件

- [ ] Rust API 和 CLI 均能优雅停止长流图。
- [ ] 无 detached worker、死锁或无限 join。
- [ ] 状态、根因和退出码稳定并有测试。
- [ ] C API stop 在 INT5-09 复用同一生命周期。

