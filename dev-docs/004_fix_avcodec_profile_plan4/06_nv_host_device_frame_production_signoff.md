# 06. NV Host 与 Device-frame 生产签字

## 1. 原则

上游 NV clean evidence 证明 SDK backend，但不能代替 dyun 的 MediaFrame bridge、Element/Graph、配置和 pump。
INT4-07 必须运行真实 dyun binary/test。

## 2. NV Host

验证 H.264/H.265 支持范围内 decode/encode/roundtrip、resize/CSC、flush/reset、重复 create/drop 和并发 worker。
Host Image/Packet 必须为 Host domain；TransferReport 对必要 copy诚实。

## 3. Device-frame

验证 Host Packet→CudaDevice Image→Host Packet；external handle ownership/drop；不支持 resize 在 create 阶段
失败；无 Host pointer 混用；`allow_staging=false`；copy/staging 为零。

## 4. Fixture/Harness

增加 gated dyun NV integration test，只有显式环境变量和 GPU runner运行真实媒体。普通 CPU CI执行 compile和
错误契约。硬件 job 设置 `--test-threads=1` 处理消费级 NVENC 限额，同时保留独立并发生命周期 stress。

## 5. Evidence

记录 dyun/SDK commit、GPU、driver、CUDA、libs、features、媒体 hash、命令、report、diagnostics、copy counts
和资源结果。无设备使用 `device_absent`，但发布状态仍 Partial。

## 6. 完成条件

- [ ] dyun NV Host 真机通过。
- [ ] dyun Device-frame 真零 staging。
- [ ] bridge handle 释放正确。
- [ ] artifact 绑定 RC2 和 dyun commit。

