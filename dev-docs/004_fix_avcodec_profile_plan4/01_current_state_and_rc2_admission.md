# 01. 当前状态与 RC2 接纳门禁

## 1. 基线

记录 dyun HEAD、dirty 状态、toolchain、target、当前 SDK pin、Cargo.lock source、features、FFmpeg/NV 环境。
审查基线为 dyun `872b449` + SDK `7faba6f`。

```bash
git status --short
git rev-parse HEAD
rustc --version --verbose
cargo --version --verbose
rg -n 'avcodec.*rev' crates/dg-media-avcodec/Cargo.toml Cargo.lock
cargo tree -p dg-media-avcodec -e features
```

## 2. 上游 RC2 必备输入

- annotated `0.2.0-rc.2` 及远端解引用 SHA；
- clean commit proof；
- 三个 consumer `--locked`；
- canonical strict Software/FFmpeg 6/7/8 passed；
- clean NV Host/device-frame evidence；
- support matrix、MIGRATION、CHANGELOG 和 handoff。

缺项写入 `UPSTREAM_ISSUES.md`，不使用 branch/local path 临时接入。

## 3. 当前已完成边界

Plan 3 的 VideoSdk service、薄 Profile mapping、四类 Session、bridge、pump、Element/Graph、错误/report/
diagnostics 保留。为防止计划执行者重做，Phase 0 source guard 必须先在当前代码通过。

## 4. 当前缺口

RC2 未创建；当前 pin 不是 RC1 tag；NV 仅 compile-only；Software 脚本有过时 blocker 注释；上游 Plan 5
状态未正确回填真实 dyun。

## 5. 完成条件

- [ ] RC2 handoff 完整且 SHA 可从远端验证。
- [ ] 当前软件基线和 guard 已记录。
- [ ] NV runner/设备可用性明确。
- [ ] 接纳决定写入状态文件。

