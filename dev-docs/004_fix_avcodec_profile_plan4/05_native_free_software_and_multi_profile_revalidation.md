# 05. NativeFree、Software 与多 Profile 重验

## 1. NativeFree

RC2 上执行 H.264 encode/decode/roundtrip、H.265 transcode、JPEG Element、resize、CSC 到 RGB24、flush/reset
第二 generation、PTS/DTS/time-base/stream-index。report 断言 Rust H.264/H.265/JPEG 和 libyuv。

## 2. Software

在可重建 FFmpeg 环境执行 H.264 encode/decode、transcode、resize/CSC、flush/reset和 session invariants。
report 必须选择 FFmpeg codec/libyuv processor。pkg-config 或 loader失败是环境失败，不能回退 NativeFree冒充。

## 3. 组合 Profile

同一 build 启用 NativeFree+Software，配置必须显式选择。相同请求分别创建，断言 selected backend 不串栈；
错误和 runtime diagnostics 保留各自 profile。

## 4. Bridge/状态机

Again 重试不丢 Packet/Image；Pending 不 busy-loop；flush drain EOS；reset 清空 pending/CSC/transcoder invariant；
Host unique buffer move、shared buffer clone 和 external drop guard 恰一次。

## 5. 验证

```bash
cargo test -p dg-media --locked --features avcodec-profile-native-free
cargo test -p dg-media --locked \
  --features avcodec-profile-native-free,avcodec-profile-software
```

保存命令、RC2 SHA、FFmpeg/toolchain和结果。

## 6. 完成条件

- [x] 软件真实媒体全部通过。
- [x] selected backend 与 Profile一致。
- [x] diagnostics/report/error 不丢上下文。
- [x] 状态机和 bridge 无回归。

