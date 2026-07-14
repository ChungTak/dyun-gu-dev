# 10. 硬件 Profile 集成与上游门禁

## 1. 通用规则

- 本章在 UP-01/02/03/04/06 完成并固定上游 revision 后实施。
- dyun 不硬编码 backend id 或厂商调用；只消费 Profile descriptor、Session Factory 和 external descriptors。
- 所有 hardware session 创建前执行 probe/preflight。
- 无对应硬件或 runtime 时返回 Unsupported/skip，不退回软件，除非 Profile 名含 fallback。

## 2. RKMPP Host

- Packet/Image 对外均为 Host。
- Profile 允许显式 staging，TransferReport 必须显示 copy count。
- decoder、encoder 使用 rkmpp，processor 使用 librga；fallback variant 才允许 ffmpeg/libyuv。
- 验收 H264/H265 decode 和 encode，至少一个 resize/CSC。

## 3. RKMPP/RGA 零拷贝

冻结角色拓扑：

```text
Host compressed Packet
  -> RKMPP decoder
  -> DrmPrime NV12 Image (dma-buf fd)
  -> librga Resize/CSC
  -> DmaBuf NV12 Image (dma-buf fd)
  -> RKMPP encoder
  -> Host compressed Packet
```

“零拷贝”仅描述中间图像链；Host compressed ingress/egress 不计入图像链 copy_count。要求：

- decode output fd 有效、guard 存活；
- RGA 不 mmap/repack 到 Host；
- resize output 是 DmaBuf；
- encoder 直接消费该 fd；
- Image chain TransferReport copy_count=0。

MppBuffer 仅用于 Profile descriptor 明确支持且不经过 RGA 的角色，不作为整个链默认 domain。

## 4. NV Host

- 使用 Host Packet、Host NV12 Image 和显式 staging。
- decoder/encoder concrete session 与 device-frame session 完全隔离。
- host fallback variant 才允许 ffmpeg。
- 验收 H264/H265；其他 codec 按运行时 capability，不写死承诺。

## 5. NV device-frame

冻结真实语义：

```text
Host H264/H265 Packet -> NV device decoder -> CudaDevice NV12 Image
CudaDevice NV12 Image -> NV device encoder -> Host H264/H265 Packet
```

- 配置名使用 `nvcodec-device-frame`。
- 旧 `nvcodec-cuda` 仅为弃用别名。
- 不称“完整 CUDA zero-copy”。
- raw device pointer 非零、pitch/UV offset/len 必须有效。
- 当前无 CUDA processor 时 media_resize/CSC 返回 Unsupported。
- 不允许自动 Cuda→Host→Cuda 回退。

## 6. OneVPL/AMF

本期只做 Host Profile：

- OneVPL decoder/encoder + libyuv processor；
- AMF encoder（及实际 capability 支持的 decoder）+ libyuv；
- 只声明 create path 真实可用的 codec/direction；
- 设备 surface 零拷贝另立计划。

## 7. 执行体任务

- [ ] 增加 hardware profile probe 命令和结构化报告。
- [ ] 实现 RK Host 与 fallback 测试。
- [ ] 实现 RK DrmPrime→DmaBuf image chain 测试。
- [ ] 断言 RK external fd、plane stride、guard 和 copy_count。
- [ ] 实现 NV Host 与 device-frame 分离测试。
- [ ] 为 NV resize 增加确定性 Unsupported 测试。
- [ ] 增加 OneVPL/AMF Host gated smoke。
- [ ] 验证 Required profile 在 backend unavailable 时没有软件输出。

## 8. 完成条件

RKMPP/RGA 硬件链有真实设备证据；NV 命名和行为与实际 I/O 一致；任何 unsupported operation 都不会隐式 staging 或 fallback。

