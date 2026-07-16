# 11. 实施顺序与最终验收

## Phase 0：RC2 门禁与基线

验证 handoff/tag，记录当前软件测试和 guard，不改生产行为。退出条件是 RC2输入完整。

## Phase 1：Pin 与环境

原子更新 manifest/lock/contract；验证精确 1.94.1；清理 Software脚本。独立 commit，便于回滚。

## Phase 2：软件重验

执行 NativeFree、Software、组合、bridge/state/error/report。失败先判断 dyun还是上游并记录最小复现。

## Phase 3：NV 真机

增加/运行 dyun NV Host/device-frame gated测试，生成 artifact。不得只跑 compile或引用上游日志。

## Phase 4：Handoff 与发布

更新 status/capability/examples，回传上游，执行 RC2接纳和 stable/rollback流程。

## 最小命令

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p dg-media --locked --features avcodec-profile-native-free
cargo test -p dg-media --locked \
  --features avcodec-profile-native-free,avcodec-profile-software
cargo check -p dg-media --locked --features avcodec-profile-nvcodec-host
cargo check -p dg-media --locked --features avcodec-profile-nvcodec-device-frame
```

NV runtime命令由 06 的 harness固定并写入状态。

## 最终清单

- [x] INT4-01～08 Done；INT4-09 dyun 侧 Done；INT4-10 回滚文档 Done / stable pin 待 tag。
- [x] RC2 tag/manifest/lock一致（`20684324`）。
- [x] 高层 SDK boundary guard通过。
- [x] NativeFree/Software/组合/NV真实验收。
- [x] dyun handoff 与回滚记录完整；上游 ACK 待外部。

