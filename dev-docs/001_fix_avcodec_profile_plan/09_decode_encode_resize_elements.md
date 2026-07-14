# 09. Decode、Encode、Resize Element 集成

## 1. Session 组装

建立 `AvcodecSessionBuilder`，依赖注入 Registry 和 `AvcodecProfile`。生产代码使用上游 Profile Factory；测试传入 fake registry。禁止模块级隐藏 singleton 影响并行测试。

构建结果保存：

- Profile name；
- role；
- selected backend；
- input/output domain；
- allow_staging；
- SelectionTrace。

## 2. DecodeCore

1. 首包到达后合并 element 约束和 MediaInfo，构造 DecoderConfig/CodecParameters。
2. codec/bitstream/timebase 缺失且不是 legacy JPEG 时返回 InvalidArgument。
3. 创建 decoder 前执行 Profile preflight。
4. 后续 packet 的 codec、format、timebase 和 stream_index 必须与 session 匹配。
5. 输出保留 decoder 原生 pixel format、coded/visible/crop、planes、flags 和 timestamps。
6. 配置 output_format 时创建同一 Profile processor，并由状态机异步 CSC。
7. 旧 width/height/channels 仅检查输出；channels=3 可兼容映射 RGB24并告警。

## 3. EncodeCore

1. 首帧 ImageMediaInfo 决定 width/height/input format/domain。
2. H264/H265/VP8/VP9/AV1 必须有非零 bitrate；不使用 1 作为占位。
3. timebase 优先来自 frame；显式配置只可匹配或补缺，不能静默改写。
4. 根据 Profile descriptor 选择 encoder input format。若需 CSC，创建同 Profile processor。
5. CUDA device-frame 必须已经是 CudaDevice NV12，缺少 processor 时拒绝 RGB Host 输入。
6. 输出 Packet 写入准确 codec、bitstream format、flags、timebase、stream_index。
7. encoder 产生新的 codec config 时更新 metadata，供 stream sink 更新 TrackInfo。

## 4. ResizeCore

- ProcessorConfig 必须设置 Resize target operation。
- input/output domain 由 Profile transition 决定。
- width/height 转换为 u32 前检查溢出。
- 保留 PTS/DTS/timebase、颜色和 frame flags。
- 更新 coded/visible/plane layout；crop 若超出新尺寸必须重置或显式换算。
- processor 不支持该 format/domain 时返回 selection report，不换到 RegistryOrder。

## 5. Format 默认值

| Profile/codec | Encoder 默认输入格式 |
| --- | --- |
| native-free/software H26x | YUV420P |
| RKMPP Host/zero-copy | NV12，兼容 YUV420P 由 capability 决定 |
| NV Host/device-frame | NV12 |
| OneVPL/AMF Host | NV12 |
| JPEG/MJPEG | 保留受 backend 支持的 packed format |

默认值只在 metadata/配置未指定时使用，并必须写入 build diagnostics。

## 6. Element schema 兼容

- avcodec 模式下 decode width/height 不再必需。
- 无 avcodec feature 的 raw adapter schema 和行为保持不变。
- encode 空 params 继续兼容 JPEG，但用户文档标为测试/legacy 行为。
- unknown profile、format、codec、memory_domain 在 graph load 时失败。
- 依赖首帧才能判断的冲突在 element runtime 返回带 node name 的错误。

## 7. 执行体任务

- [ ] 引入 SessionBuilder 并移除正常路径的 `create_decoder/create_encoder/create_csc_processor`候选循环。
- [ ] 实现首输入惰性初始化和 session invariants。
- [ ] 将 CSC/resize adaptation 纳入 08 状态机。
- [ ] 更新 element ParamField 和 unknown-field 校验。
- [ ] 删除固定 timebase、bitrate、stream_index 和 JPEG fallback。
- [ ] 更新输出 metadata 和 TrackInfo 更新事件。
- [ ] 为无 avcodec/raw adapter 保留原回归测试。

## 8. 完成条件

三个 element 只通过 Profile Factory 创建后端；真实视频参数来自配置或 metadata；格式适配、异步语义和输出 metadata 均可由 contract test 证明。

