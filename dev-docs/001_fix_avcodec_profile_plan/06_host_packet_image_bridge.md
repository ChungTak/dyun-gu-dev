# 06. Host Packet/Image Bridge

## 1. 目标

Host bridge 是第一阶段可发布路径。它允许显式 Host materialization，但每次所有权移动或复制必须可区分、可计数，不允许把 Host copy 描述为零拷贝。

## 2. Packet 映射

MediaFrame→avcodec Packet：

- payload 必须 Host-readable；否则仅在 Profile 允许 staging 且存在 mapper 时显式下载。
- codec、bitstream format、stream_index、PTS/DTS、flags、timebase 来自 MediaInfo。
- DecoderConfig.parameters 从与 packet bitstream format 匹配的 codec config 构造。
- H264/H265 Annex-B parameter sets 按 `00 00 00 01 + NAL` 拼接。
- 未知 codec/format 组合返回 Unsupported，不回退 JPEG。

avcodec Packet→MediaFrame：

- `host_bytes() == None` 是 domain mismatch，不是空 payload。
- 输出 metadata 必须保留所有 Packet 字段。
- Buffer 长度必须等于 BufferSlice len，而不是底层 handle 总大小。
- key/lost/corrupt flags 必须无损映射。

## 3. Image 映射

支持格式和形状：

| Pixel format | Plane | Shape 展示 |
| --- | ---: | --- |
| Gray8 | 1 | H×W×1 |
| RGB/BGR24 | 1 | H×W×3 |
| RGBA/BGRA | 1 | H×W×4 |
| YUV420P | 3 | 以 ImageMediaInfo 为准，不伪装 packed shape |
| NV12/NV21 | 2 | 以 ImageMediaInfo 为准 |

Host padded plane 使用 row-wise copy helper：只复制每行有效字节或在目标同样支持 stride 时保留 padding。不得要求 `stride == packed_stride`。

## 4. Buffer API 加固

在 `dg-core::Buffer` 增加：

```rust
pub fn try_into_host_bytes(self) -> Result<Vec<u8>>;
pub fn is_host_readable(&self) -> bool;
```

- unique Host storage 可移动 Vec，不计 copy。
- shared Host storage clone bytes，计一次 copy。
- external host-mapped storage按实际所有权计数。
- external-only device storage返回 Buffer error。
- 旧 `read_bytes`/`into_host_bytes` 不删除，但标 deprecated，并禁止 codec bridge 使用。

## 5. TransferReport

新增或扩展 path 类型以区分：

- `OwnershipMove`：0 copy；
- `SharedExternal`：0 copy；
- `HostClone`：1 copy；
- `DomainStaging`：每个 domain crossing 计一次；
- `RowRepack`：1 copy。

报告包含 operation、source/target domain、source/target layout、copy_count 和原因。

## 6. Format adaptation

- Decoder 默认输出原生 image，不自动 CSC。
- 显式要求 RGB/BGR 时，通过同一 Profile 的 processor 创建 CSC。
- Encoder 输入格式不被 backend 接受时，按 Profile descriptor 的首选 encoder format 创建 CSC。
- software inter-frame 默认 YUV420P；RK/NV/OneVPL/AMF Host 默认 NV12；JPEG 优先保留受支持的 packed 输入。
- processor submit/poll 由状态机驱动，不在 bridge 内同步 poll 一次。

## 7. 执行体任务

- [ ] 拆分 packet bridge、image bridge、format mapper 和 transfer accounting。
- [ ] 删除空 Vec fallback 和固定 stream_index/format。
- [ ] 实现 Buffer 安全 Host API及弃用标记。
- [ ] 实现 packed/planar/semi-planar host copy helper。
- [ ] 将 CSC 从 bridge 的同步临时 processor 移入 Decode/Encode core 状态机。
- [ ] 为每种 transfer path 增加精确 copy_count 测试。
- [ ] 添加 H264/H265/VP9/AV1/JPEG packet metadata roundtrip。
- [ ] 添加 I420、NV12 padded、BGR、BGRA image roundtrip。

## 8. 完成条件

Host Profile 可处理真实压缩视频和常见像素格式；任何复制都有报告；device-only Buffer 不可能被转换为空 Host frame。

