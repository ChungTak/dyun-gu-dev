# 14. 测试矩阵、CI 与真实媒体

## 1. 分层

- 单元：Profile mapping、Request、bridge、pump、error/diagnostics。
- 合同：dependency/source guard、feature mapping、禁止低层符号。
- 软件真实媒体：NativeFree 和 FFmpeg 6/7/8。
- 硬件：NV Host/device-frame 真机；其他 compile-only/unverified。
- workspace：fmt、clippy、tests、examples、CAPI snapshot。

## 2. 必测媒体

NativeFree/Software 均完成 H.264 decode、encode、roundtrip、resize/CSC、H.264→H.265 transcode、flush/reset。
NV 在设备支持 codec 范围执行等价路径。fixture 固定 hash、帧数、尺寸、格式和期望输出，不依赖网络下载。

## 3. Feature jobs

no profile、NativeFree、Software、NativeFree+Software、NV Host、NV Device-frame、三者组合；RK/OneVPL/AMF
分别 compile-only。组合 job 必须断言 report selected backend，不以“创建成功”作为隔离证明。

## 4. 推荐命令

```bash
cargo fmt --all -- --check
cargo clippy -p dg-media --features avcodec-profile-native-free --all-targets -- -D warnings
cargo test -p dg-media --features avcodec-profile-native-free
cargo test -p dg-media --features avcodec-profile-native-free,avcodec-profile-software
cargo test --workspace --locked
```

Software/NV runner 补充环境脚本和专用测试命令，写入状态文件。strict job 不允许环境降级。

## 5. Soak 与性能

首发 Profile 至少验证重复 create/drop、长帧序列、reset generation、队列背压和资源稳定。性能数字不作为
正确性替代；零拷贝必须以 domain/handle/copy 证据证明。

## 6. 完成条件

- [ ] source/dependency guard required。
- [ ] 软件和 NV 真实媒体通过。
- [ ] skip reason 结构化。
- [ ] artifact 绑定 SDK/dyun commit。

