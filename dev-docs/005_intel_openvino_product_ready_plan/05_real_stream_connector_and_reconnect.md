# 05. Cheetah 真流入口、重连与错误分类

## 1. 产品 feature 与初始化

为 `dg-cli`/`dg-capi` 转发 `cheetah`，并纳入 `product-intel`。CLI 在 graph build 前幂等安装
`EmbeddedCheetahRuntimeConnector`；Rust SDK 保留显式安装；C API 由 `dg_runtime_init` 初始化内置 adapter。
重复初始化返回成功，配置冲突返回明确错误。

## 2. 配置合同

pull/push element 增加严格字段：

```yaml
retry:
  initial_backoff_ms: 250
  max_backoff_ms: 30000
  multiplier: 2
  jitter_percent: 20
  max_attempts: 0 # 0 = 无限
connect_timeout_ms: 10000
io_timeout_ms: 30000
```

默认只对 connector 标记 `retryable=true` 的 connect/timeout/remote-close 重试；鉴权、URL、codec、track 和配置错误终止。
重连成功前不发送伪造 frame/EOS；恢复后首帧等待随机访问点并标记 discontinuity。

## 3. Typed error 与脱敏

`dg-stream::Error` 保留 protocol、operation、retryable、endpoint class、status/code 和 source chain；
不得在错误和 metrics label 中包含 URL userinfo、query token 或完整 stream key。日志使用脱敏 endpoint。

## 4. 运行指标

增加 connect attempts、reconnects、reconnect delay、remote closes、auth failures、protocol errors、
frames dropped while disconnected、keyframe requests。连接丢失使 readiness false，但 process liveness 保持 true。

## 5. 验收

- RTSP/HTTP-FLV pull、RTMP/WebRTC push 走真实 connector 分支；
- socket-free loopback 只作为普通 CI；协议 framing/local socket 用于集成 CI；
- 服务器停止后恢复，长流图自动重连且 metadata 保真；
- 错误分类和 URL 脱敏 golden test；
- runtime drop/shutdown 不泄漏 Tokio runtime 或 bridge thread。

## 6. 完成条件

- [ ] 正式 CLI/OCI 可运行真实协议 URL。
- [ ] 重连/超时/取消均为配置化确定行为。
- [ ] typed error 可供 supervisor、metrics 和 C API 映射。
- [ ] 不记录认证信息或完整敏感 endpoint。

