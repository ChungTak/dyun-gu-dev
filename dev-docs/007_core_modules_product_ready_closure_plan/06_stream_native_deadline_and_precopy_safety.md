# 06. Stream 原生 Deadline 与 Pre-Copy 安全

> 需求 ID：CORE7-06

## 1. 上游准入

固定的 Cheetah revision 必须提供可取消接收合同：

```rust
pub enum ReceiveOutcome {
    Frame(Arc<AVFrame>),
    EndOfStream,
    TimedOut,
}

async fn recv_timeout(&mut self, timeout: Duration) -> Result<ReceiveOutcome, SdkError>;
async fn close(&mut self) -> Result<(), SdkError>;
```

timeout 由 runtime 原生 timer/cancellation 驱动；close 唤醒 pending recv。不得每个 poll 创建 OS thread，
不得用 detached thread 包装永久 pending future，也不得从 Tokio runtime 内调用 `Handle::block_on`。

上游未满足前，Cheetah capability 保持 Blocked。升级 revision 时记录 compare、Cargo.lock hash、feature
集合和上游 contract test。

## 2. Runtime Bridge

- Embedded connector 显式拥有 runtime、connector task 和 cancellation token；
- open/connect/recv/close/stop 均接受 deadline；
- Drop 只触发非阻塞 cancel；产品入口显式 close/join；
- bridge thread 如确实需要，必须有 handle、数量上限和 deadline join；
- recv poll slice 默认不超过 100ms，完整 I/O timeout 与 poll slice 分离；
- shutdown 后 runtime task/thread/fd 回到基线。

## 3. Effective Frame Policy

`open_pull`/adapter/bridge 接收 `Arc<ProcessRuntimePolicy>`。在上游 payload 产生第一份 host owned copy 前校验：

- payload bytes；
- track/config/tag count 和累计 bytes；
- coded dimensions、planes、stride 与 physical bytes；
- timebase、track ID、codec/format/readiness；
- queue packets/bytes。

检查必须使用 graph effective limit，而非 process default。若上游已经先分配完整 payload，记录
UP7-002 并要求上游 allocation hook；dyun 的后置拒绝不能宣称 pre-allocation protection。

## 4. Reconnect 与关闭

- 只有 typed retryable connect/timeout/remote-close 可重试；
- auth/config/codec/resource-limit 直接停止对应 stream；
- reconnect 前 close 旧 endpoint 并释放 generation/bootstrap/queue；
- `max_attempts=0` 可表示次数无限，但总 elapsed、backoff、memory 和 shutdown 仍有上限；
- shutdown/reload 优先于 reconnect，sleep 可被 cancellation 唤醒；
- 恢复后等待随机访问点，只给首帧 discontinuity；
- reconnecting 使 readiness false，liveness 保持 true。

## 5. Registry

Memory hub 与 Cheetah manager 的 stream/subscriber/bootstrap cache 使用 process policy 的 capacity/TTL。
查询未知 key 不创建 entry；最后 handle 关闭后回收；高基数 churn 不产生 metrics label。

## 6. 测试

- fake upstream 永久 pending，recv 每 100ms 返回 TimedOut 且无新 OS thread；
- close 与 recv 并发，在 100ms 内唤醒；
- timeout、close、runtime stop、graph shutdown 和 reload race；
- effective limit 小于 default 的 `limit+1` frame 在 copy count=0 时拒绝；
- 真实 RTSP/HTTP-FLV 断网、半开、DNS/connect stall、remote close；
- RTMP/WebRTC push backpressure、close 和 reconnect；
- 10万 stream key churn 后 registry/cache 有界；
- 2h/24h 记录 thread/fd/task/subscriber/queue 曲线。

## 7. 完成条件

- [ ] Cheetah SubscriberSource 提供原生 timeout/cancel。
- [ ] dyun adapter 不创建 detached timer/recv thread。
- [ ] effective frame policy 在第一份 host copy 前执行。
- [ ] shutdown/reload 可中断 connect/recv/backoff/close。
- [ ] 真实协议未验收时 support matrix 保持 Blocked。

