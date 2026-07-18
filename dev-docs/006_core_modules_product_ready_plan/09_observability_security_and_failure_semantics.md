# 09. 可观测性、安全与失败语义

> 需求 ID：CORE6-09

## 1. 指标有界性

所有 metrics 结构必须常量或配置上限内存：

- backend latency 使用固定 buckets；
- node/instance/device label 来自有限注册表；
- 禁止 stream id、URL、model path、错误文本和 callback pointer 作为 label；
- counter overflow 有 diagnostic，不回绕；
- metrics scrape 不修改业务 state，不持有 graph/worker 重锁。

metrics JSON/OpenMetrics 增加 schema version。字段变更先更新 snapshot/golden 和 C ABI capability，禁止 exporter
静默丢字段。

## 2. 必需指标

- graph lifecycle、reload attempts/success/rejected/failed；
- worker、queue packet/bytes/current/max、backpressure/drop；
- resource limit rejects，按有限 limit name 分类；
- backend pool submissions/in-flight/pending/error/cancel、latency histogram、copy；
- scheduler affinity entries/evicted/expired、load invariant error；
- memory pool cached/evicted bytes，external callback failure；
- stream connect/reconnect/timeout/remote close/auth/protocol/limit error；
- sink/collector current/max 和 shutdown duration/result。

## 3. Health 与 readiness

- `/livez` 表示 supervisor/event loop 可服务，不只返回固定 200；ops thread fatal 时失败。
- `/readyz` 要求 graph Running、无 reload/drain/reconnect、必需 backend/source/sink ready。
- `/metrics` 从已复制 snapshot 渲染，先释放 RwLock；慢客户端不能阻塞 supervisor 或媒体 worker。
- 非 loopback bind 保持显式安全警告；生产部署由反向代理/网络策略提供认证和 TLS。

reason code 使用稳定枚举，如 `starting/reloading/reconnecting/resource_limited/backend_unavailable/failed/draining`；
详细错误放 body/log，不放高基数 label。

## 4. Error taxonomy

核心错误至少区分：

- InvalidArgument / Parse / Validation；
- ResourceLimit / ResourceExhausted；
- Unsupported / BackendUnavailable；
- Timeout / Cancelled / Closed；
- Protocol / Auth / RemoteClosed；
- Busy / InvalidState；
- Invariant / Panic / Internal。

错误保留 graph/node/backend/device/operation/limit/source chain。retry/drop/fail 由 typed category 决定，
禁止用 `to_string().contains(...)` 判断 shutdown timeout 或 retryability。

首个根因优先，后续取消错误作为 suppressed context。poison lock 不统一吞掉：只读 immutable 数据可恢复，
所有权/调度/路由不变量 poison 必须 fail-closed 并记录。

## 5. 日志与脱敏

URL userinfo、query、token、完整私有 stream key、模型绝对路径和外部 raw handle 默认脱敏。日志只记录模型 hash
短前缀、endpoint class/host、有限 error kind。C error message、metrics 和 release evidence 使用同一 redaction。

callback panic、worker panic 和 resource invariant failure 记录一次结构化 fatal event，不能打印 payload/model bytes。

## 6. 测试

- 100 万 metric observations 后 RSS/结构大小稳定。
- pool、resource、stream、scheduler 指标 golden 与实际事件数一致。
- 并发 scrape 不降低固定 workload 吞吐超过 5%，不阻塞 shutdown/reload。
- health 状态覆盖 Starting/Running/Reloading/Reconnecting/Draining/Failed/Stopped。
- typed error 到 CLI exit code、C status/error 和 retry policy 的矩阵测试。
- secret/URL/path/handle redaction golden；随机 token fuzz 不出现在输出。
- poisoned lock、panic、slow metrics client 和 ops thread failure fault injection。

## 7. 完成条件

- [ ] metrics 内存、label 和 schema 有界稳定。
- [ ] health 与真实 supervisor/readiness 状态一致。
- [ ] 错误分类驱动 retry/drop/fail，不使用字符串匹配。
- [ ] 日志、C error、metrics 和证据无敏感信息。
- [ ] panic/poison/invariant failure 可观测且 fail-closed。
