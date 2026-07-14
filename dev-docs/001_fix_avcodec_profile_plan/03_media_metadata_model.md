# 03. 中立媒体元数据模型

## 1. 目标与权威类型

在 `dg-core` 新增不依赖 avcodec、cheetah 或图运行时的媒体值对象。`dg-media` 和 `dg-stream` 只做 mapper，不再各自维护无法无损转换的半套元数据。

建议公开类型（名称可按仓库风格微调，但字段和语义不得删除）：

```rust
pub struct MediaInfo {
    pub timing: MediaTiming,
    pub payload: MediaPayloadInfo,
}

pub struct MediaTiming {
    pub pts: Option<i64>,
    pub dts: Option<i64>,
    pub time_base: Option<MediaTimeBase>,
}

pub enum MediaPayloadInfo {
    Encoded(EncodedMediaInfo),
    Image(ImageMediaInfo),
}
```

## 2. EncodedMediaInfo

必须包含：

- `stream_index: u32`；
- `track_id: Option<u64>`；
- `media_kind`；
- `codec`；
- 精确 `bitstream_format`；
- KEY/LOST/CORRUPT flags；
- 有界 `codec_configs: Vec<MediaCodecConfig>`。

Bitstream format 至少覆盖 H264 Annex-B/AVCC、H265 Annex-B/HVCC、VP8 frame、VP9 frame、AV1 OBU、JPEG interchange、AAC raw/ADTS 和 Unknown。Unknown 只能用于运输，创建 decoder 前必须解析或失败。

`MediaCodecConfig` 使用 `{ format, data: Vec<u8> }`，允许同时保存 Annex-B 参数集与 AVCC/HVCC record。单项最大 1 MiB，最多 8 项，总大小最大 4 MiB；越界返回 `InvalidArgument`。

## 3. ImageMediaInfo

必须包含：

- pixel format：YUV420P/YUV422P/YUV444P/NV12/NV21/RGB24/BGR24/RGBA/BGRA/Gray8；
- coded width/height；
- visible rect 与 crop rect；
- color primaries/transfer/matrix/range；
- image flags；
- sample type 与 interleaved/planar layout；
- 1–4 个 `MediaPlaneLayout { offset, stride, len }`；
- 可选 fence id。

所有 offset/len 使用 checked arithmetic；plane 数必须与 pixel format 一致；stride 不得小于有效行字节；最后一行有效区域必须落在 Buffer size 内。

## 4. MediaFrameMeta 迁移

```rust
pub struct MediaFrameMeta {
    pub stream_id: Option<String>,
    pub tags: BTreeMap<String, String>,
    pub media_info: Option<MediaInfo>,
    // deprecated compatibility field, remove after one release
    pub stream_metadata: Option<MediaStreamMetadata>,
}
```

- `media_info` 是唯一权威来源。
- 旧字段存在而新字段缺失时，兼容 mapper 生成 `media_info`。
- 两者同时存在且 codec/timebase/track/keyframe 冲突时返回错误，不选边覆盖。
- 新 producer 必须只以 `media_info` 构造数据；兼容出口可同步填充旧字段。

## 5. Tensor 转换规则

- packed Host image 可生成普通 TensorDesc，但 image layout 必须留在 metadata。
- planar/semi-planar 或 device image 不得伪装成 NHWC packed Tensor。
- `MediaFrame::into_tensor` 必须保留 metadata 的运输通道，或者只用于明确会同步 PacketMeta 的调用方。
- 外部 device Buffer 不允许通过 `read_bytes()` 变成长度为 0 的 Tensor。

## 6. 执行体任务

- [ ] 在 `dg-core` 新增媒体值对象、构造器、校验器和 rustdoc。
- [ ] 为所有 enum 增加 Unknown/Unsupported 处理，mapper 不得 panic。
- [ ] 将 `MediaStreamMetadata` 迁移为兼容 DTO，避免继续扩展其模糊 `CanonicalH26x` 语义。
- [ ] 更新 `MediaFrame` 构造器，使 metadata 可显式传入和保留。
- [ ] 增加 codec config 数量/大小、timebase denominator、rect overflow、plane bounds 属性测试。
- [ ] 增加 H264 Annex-B、H265 HVCC、NV12 padded 和 CUDA external layout golden tests。

## 7. 完成条件

- 中立类型不依赖 dg-media、dg-stream、avcodec 或厂商 SDK。
- encoded/image 两类 metadata 均可 clone、比较并跨 Graph Packet 往返。
- 非法 timebase、布局和 codec config 在进入 backend 前失败。
- 旧 `stream_metadata` 有唯一、可测试的兼容优先级。

