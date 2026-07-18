# 06. Media/Stream 可取消 I/O 与桥接

> 需求 ID：CORE6-06

## 1. SubscriberSource 合同

新增明确超时结果：

```rust
pub enum ReceiveOutcome {
    Frame(Arc<MediaFrame>),
    EndOfStream,
    TimedOut,
}

async fn recv_timeout(&mut self, timeout: Duration) -> Result<ReceiveOutcome>;
```

`SubscriberSourceSyncExt` 提供 `recv_blocking_timeout`。旧 `recv()` 仅作为兼容便利封装循环调用 timeout API，
不得被产品 element 使用。所有 adapter 必须原生遵守 timeout，不能用一个可能永久 pending 的 future 包装。

`StreamPullElement` 使用不超过 100 ms 的 poll slice；每次 `TimedOut` 只检查 stop/readiness，不视为断流。
`io_timeout_ms` 控制完整网络读故障，poll slice 控制协作取消，两者语义分离。

## 2. Close 与取消

- `close` 必须唤醒正在等待的 recv；重复 close 幂等。
- Cheetah adapter 用 runtime 原生 timeout/cancel；若上游缺少能力，记录 upstream issue，不能用 detached thread。
- MemoryStreamHub 的 condvar wait 使用相同 deadline，publisher close/overflow/stop 都要 notify。
- connector runtime、bridge thread 和 Tokio runtime 必须有显式 shutdown；Drop 只做非阻塞兜底。
- pull open、recv、close、reconnect backoff 都检查 graph stop。

## 3. Frame 入口安全

网络 payload 按不可信输入处理，在任何 `clone().to_vec()`、host allocation 或 codec parse 前执行：

- frame bytes、track 数、codec config count/bytes；
- track id 可表示性、timebase、codec/format 组合；
- decoded width/height/planes/stride/physical bytes；
- metadata/tag 数量与累计字节；
- queue packet 和 byte budget。

超限错误包含 protocol、operation、limit name、actual、maximum 和脱敏 endpoint。错误只终止对应流或按 typed
retry policy 重连，不 panic 进程。

## 4. Bridge 正确性

修正 Cheetah/media bridge：

- payload 最多做一次必要 host copy，并由 `TransferReport` 诚实计量；
- 删除构造失败后手工分配空/默认 frame 的 fallback；
- `u64 → u32` track id 超范围返回错误，不饱和为 `u32::MAX`；
- `MediaInfo::encoded/image` 构造失败必须传播，不能 `if let Ok` 忽略；
- codec、bitstream、track readiness、extradata 和 keyframe/discontinuity 保真；
- external handle 只有在 domain/layout/ownership 全部兼容时 Shared，否则显式 Staged 或 Unsupported。

push 侧更新 tracks 与发送 frame 使用同一 generation，避免 reload/reconnect 后旧 track metadata 配新 frame。

## 5. Reconnect、背压与 readiness

- connect/remote-close/timeout 仅在 `retryable=true` 时重试；auth/config/codec/limit 错误终止。
- reconnect 前关闭旧 source，释放 queue/bootstrap/track generation。
- 恢复后视频等待随机访问点，并只给第一帧标记 discontinuity。
- disconnected 期间不缓存无界 frame；drop 数和 keyframe request 可观测。
- reconnecting 使 readiness false，但 liveness 保持 true。
- retry/backoff 状态本身受 attempt/time budget；`max_attempts=0` 只表示次数无限，不表示资源和 shutdown 无限。

## 6. Stream registry 与 Hub

MemoryStreamHub/manager 的 stream、publisher、subscriber 和 bootstrap cache 都设置容量与 idle TTL。最后一个
publisher/subscriber 关闭后清理无状态 stream；保留 bootstrap 的 stream 按 policy 到期。不得用私有 stream key
作为 metrics label。

## 7. 测试

- 永久 pending subscriber 在 100 ms slice 后返回 TimedOut，graph shutdown 在 deadline 内完成。
- close 与 recv 并发、重复 close、runtime drop、断流重连和 shutdown/reload 竞争。
- `limit+1` frame 在复制/分配前拒绝；计数 allocator 证明没有大 allocation。
- invalid metadata、oversized track id、zero timebase、未知 codec/format 不被 fallback。
- payload copy count、PTS/DTS/timebase/extradata/keyframe/discontinuity golden test。
- Hub 10 万不同 stream key 后 registry/cache 仍在 capacity/TTL 内。
- 真实 RTSP/HTTP-FLV pull、RTMP/WebRTC push 的 fault injection 由 Cheetah runner 保存证据。

## 8. 完成条件

- [ ] 所有 subscriber adapter 支持可验证 timeout/close。
- [ ] StreamPullElement 不再无限阻塞 shutdown。
- [ ] 网络 frame 在复制前执行资源和 metadata 校验。
- [ ] bridge 不吞错、不饱和 ID、不伪造空 frame。
- [ ] reconnect、Hub registry 和 runtime 生命周期长期有界。
