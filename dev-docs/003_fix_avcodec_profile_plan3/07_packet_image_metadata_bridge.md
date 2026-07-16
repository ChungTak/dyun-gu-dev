# 07. Packet、Image 与 Metadata Bridge

## 1. 边界

bridge 只转换 dg `MediaFrame`/Graph packet 与 SDK `Packet`/`Image` 的数据和 metadata，不选择 backend、
Profile、domain 或 staging。压缩流与原始图像必须通过不同函数，避免错误解释 plane。

## 2. Packet

保留 payload 所有权、PTS、DTS、duration、time base、keyframe/flags、stream index 和 bitstream metadata。
提交 Again 时原 Packet 必须仍由调用方持有或可安全重试；不得在失败前消费不可恢复数据。

## 3. Host Image

校验 format、宽高、plane 数、stride、offset 和 buffer bounds。尽量移动/共享 owned buffer；若业务边界必须
复制，生成明确 TransferReport 并计数，不伪装为 SDK staging。

## 4. 外部内存

DmaBuf、DrmPrime、CudaDevice 使用显式 external handle、owner/release callback 和 MemoryDomain。Host pointer
转换函数不得接受 CudaDevice。`allow_staging=false` 路径不允许 bridge 下载/上传到 Host。

## 5. 测试

metadata roundtrip、奇数 stride、多 plane bounds、空/损坏输入、Again 所有权、Host copy 计数、外部 handle
drop 恰一次、domain mismatch。测试使用小型自包含 fixture，不引用 vendor。

## 6. 完成条件

- [x] bridge 不 import backend/policy。
- [x] 所有权和释放规则有单测。
- [x] 零拷贝路径无静默 copy。
- [x] stream metadata 不丢失。
- [x] Host external/owned import：`import_host_image_*` / `import_host_packet`；设备句柄经 `import_external_*`。

