# 04. Graph 与 Stream 的媒体元数据运输

## 1. 问题定义

当前 `MediaFrame → Tensor → dg_graph::Packet` 只保留 sequence、stream_id 和字符串 tags，codec、bitstream、timebase、extradata 与 image planes 全部丢失。该问题必须在 graph envelope 解决，不能在下游通过猜测 codec 或固定 1/25 修补。

## 2. PacketMeta 契约

向 `dg_graph::PacketMeta` 增加：

```rust
pub media_info: Option<dg_core::MediaInfo>
```

保持现有 `sequence`、`stream_id`、`tags` 兼容。规则如下：

1. sequence 是 graph 运输序号，不等于 PTS，不得互相覆盖。
2. PTS/DTS/timebase 只来自 `media_info.timing`。
3. stream_id 是业务流标识；track_id/stream_index 是媒体标识。
4. tags 只存放可观察标签，不承载 codec config、plane layout 或时间基。
5. EOS Packet 不携带普通 payload；允许携带 stream_id，但不得伪造最后一帧 metadata。

## 3. dg-media 转换

- `graph_packet_to_media_frame` 必须把 PacketMeta.media_info 移入 MediaFrameMeta。
- `media_frame_to_graph_packet` 必须在消费 frame 前提取完整 metadata，再构造 Tensor payload。
- shared `Arc<PacketPayload>` 分支和 unique 分支结果必须一致。
- 非 Tensor payload 进入 media element 时返回类型错误；只有 EOS 可转换为 `MediaFrameKind::EndOfStream`。
- 任何 conversion error 都必须保留 node/stream 上下文。

## 4. dg-stream 输入规则

Pull element 已持有 TrackInfo，必须按 track_id 建立只读缓存：

- H264/H265/H266 的 canonical H26x payload 是 Annex-B start-code 格式。
- H264 SPS/PPS、H265 VPS/SPS/PPS 转为 Annex-B codec config；若同时有 AVCC/HVCC，也作为独立 config 保存。
- AV1 sequence header/config、VP8/VP9 config、AAC ASC 按格式保存。
- frame 的 codec、track_id、PTS/DTS、timebase、keyframe 必须来自收到的 AVFrame。
- frame track 不在已宣布 TrackInfo 中时返回错误，不用默认 track 0。

## 5. dg-stream 输出规则

- Push element 优先使用 `media_info`；旧 metadata 只作为一期兼容 fallback。
- 编码输出发生 codec/尺寸/bitrate 变化时，必须生成或更新对应 TrackInfo，再推送 frame。
- `CanonicalH26x` 只接受 H264/H265/H266 Annex-B；AVCC/HVCC payload 必须先显式转换，不能只改 enum。
- 缺失必需 extradata 时沿用 track readiness 规则返回错误。
- keyframe 取自 packet/image flags，不再只依赖 `KEYFRAME_TAG`。

## 6. 执行体任务

- [ ] 扩展 PacketMeta 并修复所有 struct literal 编译点。
- [ ] 更新 graph packet clone/queue/sink 测试，证明 media_info 不丢失。
- [ ] 重写 dg-media 两向 conversion，删除 PTS→sequence 的隐式替代。
- [ ] 在 StreamPullElement 建立 track_id→TrackInfo 映射并附加 codec config。
- [ ] 在 StreamPushElement 实现 metadata 优先级和冲突检测。
- [ ] 增加 Annex-B 参数集拼接 helper，使用 checked length 且限制总大小。
- [ ] 更新 cheetah connector contract tests，覆盖 H264/H265/AV1 和多 track。
- [ ] 搜索并删除用 tags 解析 PTS/DTS/keyframe 的正常路径；仅保留兼容读取并告警。

## 7. 测试场景

1. H264 track 7、stream index 2、timebase 1/90000、负 DTS 经过 source→decode 前不变。
2. H265 VPS/SPS/PPS 与 HVCC 同时存在时均被运输。
3. 两个 track 交错 frame 不串 metadata。
4. packet clone 后两条 consumer 均读取相同 metadata。
5. media_info 与旧 stream_metadata 冲突时确定性失败。
6. EOS 不复制前一帧 codec config。

## 8. 完成条件

任何 graph media element 都不需要通过 YAML 默认值猜测上游 codec/timebase；stream pull→media decode 和 media encode→stream push 的 metadata contract tests 全部通过。

