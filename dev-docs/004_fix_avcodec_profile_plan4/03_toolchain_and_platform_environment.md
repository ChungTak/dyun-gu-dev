# 03. Toolchain 与平台环境

## 1. Rust

workspace `rust-version`、`rust-toolchain.toml`、CI 均冻结 1.94.1。clean runner 必须实际输出
`rustc 1.94.1`。本地镜像 404 分类为 distribution mirror 问题；可使用已安装精确 toolchain或正确 dist server，
不得仅用未知版本 stable 通过后称 MSRV 已验收。

## 2. Software

记录 FFmpeg pkg-config、headers/libs、动态库路径、libclang 和 `LIBYUV_TARGET`。更新
`env-software-avcodec.sh`，删除已修复的 FFmpeg 8 pointer blocker，保留真实环境诊断。脚本不得修改仓库文件。

## 3. NV

记录 GPU、driver、CUDA include/runtime、NVENC/NVDEC 库、设备权限和 session limit。Host 与 device-frame
测试使用同一 RC2 dyun binary；无设备结构化 skip，但生产签字失败。

## 4. CI/本地一致性

命令日志必须同时保存工具链和依赖环境。CI apt 安装 `libavformat` 可以作为系统包传递安装，但 SDK 测试
不得调用 demux/mux API。

## 5. 完成条件

- [ ] 精确 1.94.1 clean runner通过。
- [ ] Software 脚本无过时 blocker。
- [ ] FFmpeg/NV 环境可重建。
- [ ] 环境缺失与代码失败分类清楚。

