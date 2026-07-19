# 05. Graph/Elements 失败隔离与可观测性

> 需求 ID：CORE7-05

## 1. 错误作用域

统一错误作用域：

| 作用域 | 示例 | 行为 |
|---|---|---|
| FrameLocal | 单帧 metadata、shape、NaN、解码损坏 | drop 当前帧、计数、继续同一 stream |
| StreamLocal | 协议断开、retryable I/O | 关闭旧 endpoint，按预算 reconnect |
| NodeFatal | 参数/模型/backend capability 错误 | 停止 node 并使 graph fail-closed |
| GraphFatal | invariant、worker panic、route corruption | 保存首个根因并停止全部 worker |

分类由 typed variant 决定，禁止 `to_string().contains`。frame-local 处理必须完成当前 packet accounting，
不能留下 `packet_starts`、queue bytes 或 state 引用。

## 2. Element 合同

- 所有产品 element 声明允许的错误作用域、state reset 和 EOS 行为；
- 算法 element 在 run loop 内捕获 FrameLocal 并调用统一 `ElementIo` drop/report helper；
- ResourceLimit 默认 NodeFatal；只有明确可丢单帧的输入预算才可 FrameLocal；
- reload 重建有状态 element 时增加 `state_reset_total`；
- distributor/converger 的缺分支、deadline 和 pending state 有界；
- config/model/invariant 不得伪装为 frame drop。

## 3. 必需指标

扩展 snapshot 和 ops exporter：

- queue packets/bytes current/max、backpressure/drop；
- frame/stream/node/graph error totals，使用有限 error kind；
- resource rejects，按固定 limit name；
- MemoryPool cached/evicted/rejected bytes/entries；
- scheduler affinity entries/evicted/expired/invariant；
- stream registry/subscriber/bootstrap/reconnect/timeout；
- backend cancel report、copy count/bytes/time；
- shutdown/reload duration/result；
- counter overflow diagnostic。

不得使用 URL、stream key、model path、错误文本或 raw handle 作为 label。

## 4. Readiness

`/readyz` 由以下条件共同决定：

- GraphStatus 为 Running；
- 不在 reload/drain/reconnect；
- 所有 required source/sink ready；
- 所有 required backend live probe ready；
- 无未恢复 ResourceLimit 或 root cause。

`/livez` 表示 supervisor 和 ops loop 可服务。reason code 使用稳定枚举；详细错误只进入 body/log。
metrics 先 clone snapshot 再渲染，慢客户端不持 graph/worker 重锁。

## 5. Typed Error

Core/Runtime/Graph/Stream/C API 至少稳定映射：

`InvalidArgument, Parse, Validation, ResourceLimit, ResourceExhausted, Unsupported,
BackendUnavailable, Timeout, Cancelled, Closed, Protocol, Auth, RemoteClosed, Busy,
InvalidState, Invariant, Panic, Internal`。

跨 crate 转换保留 operation、node/backend/device、limit 和 source chain。C `DgError` category 与 CLI exit
code 使用同一矩阵。

## 6. 测试

- 每个算法 element 注入坏帧，验证后续好帧仍输出；
- config/model/invariant 使 graph Failed 并保留根因；
- 多 stream 中一条 frame-local/stream-local 故障不影响其他流；
- queue bytes、pool、affinity、registry 和 resource reject 指标 golden；
- Starting/Running/Reloading/Reconnecting/Draining/Failed/Stopped readiness；
- 慢 metrics client 与 shutdown/reload 并发；
- secret/URL/path/handle redaction property test；
- 100 次 reload 后 state reset、queue、worker 和 metrics 稳定。

## 7. 完成条件

- [ ] typed error 唯一决定 drop/retry/fail。
- [ ] frame-local 错误不终止 graph，fatal 错误不被吞掉。
- [ ] 必需资源与生命周期指标完整且有界。
- [ ] readiness 汇总真实 backend/source/sink 状态。
- [ ] CLI、C API、logs 和 metrics 的分类/脱敏一致。

