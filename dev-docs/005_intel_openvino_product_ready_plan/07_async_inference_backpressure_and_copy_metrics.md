# 07. 异步推理、背压与 Copy 诊断

## 1. Runtime 接口

把当前“submit 内同步 run 后缓存结果”的实现改为真实提交/轮询合同。`InferBackend` 增加 backend submission，
`Runtime::submit` 不等待推理完成，`poll` 返回 `Pending/Ready`；`run` 作为阻塞兼容封装。

同步厂商 backend 可用受限默认 adapter，但必须报告 `async=false`；OpenVINO 使用 infer request pool 实现 `async=true`。

## 2. OpenVINO request pool

每个 Runtime 默认 `max_in_flight=2`，配置范围 1..64。每个请求拥有输入 tensor、OpenVINO tensor、输出和提交时间，
直到 Ready/Error 才释放。禁止复用仍在飞的 request 或提前释放外部 buffer。

poll 不 busy-loop；graph worker在无结果时让出执行并继续处理取消/指标。完成顺序可不同，但 PacketMeta/stream_id
必须与对应 submission 一致。

## 3. Graph 背压

Inference element 的 in-flight 满时停止消费上游，让有界 DataPipe 产生背压。输出队列满时保留完成结果，
不重复 infer。多实例调度的 lease/load 覆盖整个 in-flight 生命周期，而不是只覆盖 submit 调用。

## 4. 指标

增加 submissions、in_flight、queue wait、infer latency histogram、poll pending、backend errors、H2D/D2H/Host copy
count/bytes/latency。label 只用 node/backend/device/precision，禁止 model path、URL 和 stream id 高基数字段。

## 5. 测试与基准

- mock 延迟 backend 验证 submit 立即返回、poll Pending→Ready；
- 乱序完成仍保持 metadata 对应；
- queue full、取消、backend error 和 hot reload 不泄漏 request；
- OpenVINO CPU/iGPU 分别测试 1/2/4 in-flight；
- 固定 runner比较吞吐、p50/p95/p99 和 copy bytes，形成 release baseline。

## 6. 完成条件

- [ ] OpenVINO submit/poll 为真实异步。
- [ ] in-flight、背压、取消和资源所有权正确。
- [ ] 性能/拷贝指标可在线抓取。
- [ ] 其他 backend 不被错误标记为 async。

