# 08. 运维、可观测性与输入安全

## 1. 本地运维 HTTP

新增可关闭的 ops server，默认 `127.0.0.1:9090`：

- `/livez`：supervisor/event loop 存活；
- `/readyz`：graph Running、目标 backend/device 可用、必需 source/sink 已就绪且不在重连；
- `/metrics`：OpenMetrics 文本。

不提供 graph mutation、模型上传或推理 HTTP API。非 loopback bind 必须显式配置并打印安全警告。

## 2. Readiness 状态机

Starting/Reloading/Reconnecting/Draining/Failed 均返回非 2xx readiness和稳定 reason code；liveness 只在 supervisor
不可服务时失败。handler 不持有 graph 重锁，不得因指标抓取阻塞媒体线程。

## 3. 指标与日志

指标覆盖节点吞吐、延迟直方图、队列、drop/backpressure、reload、重连、copy、backend request和资源状态。
日志支持 human/json，字段包含 graph/node/backend/device/operation/error_kind；敏感 URL、token、userinfo和模型路径脱敏。

## 4. ResourceLimits

统一 limits 默认值：配置 8 MiB、include depth 16/数量64、节点1024、边8192、tensor/frame 512 MiB、模型2 GiB。
所有 size/rank/stride/queue 计算 checked；可信部署可在进程初始化时提高，但不得由网络媒体动态改变。

媒体/协议输入按不可信处理：限制 codec config、frame、track、队列和解析深度；错误 frame 只终止对应流或按策略重连，
不得 panic 整个进程。

## 5. 验收

- health/metrics 并发抓取不影响图吞吐；
- 断流、reload、shutdown 的 readiness 变化正确；
- secret redaction golden tests；
- 超大 frame/config/model/shape 在分配前拒绝；
- panic、OOM 可分类场景和 poisoned lock 返回明确错误。

## 6. 完成条件

- [ ] 三个 ops endpoint 与 supervisor 状态一致。
- [ ] 指标无敏感/高基数 label。
- [ ] 默认 limits 在 Rust、CLI、C API 一致。
- [ ] 不可信媒体错误不导致进程崩溃。

