# avcodec-rs RC2 接纳与回传

> Plan 4 RC2 接纳记录。上游 tag、dyun pin、测试矩阵与 handoff 状态持续更新。

## 上游输入

| 字段 | 值 |
|---|---|
| RC2 tag / dereferenced commit | `0.2.0-rc.2` / `2068432426793c94cd5d415b56a4b2e9a3c1ee73` |
| tag object | `06ac7302f83a94fe40cb321c01bbc3cb794d9e64` |
| crate version | `0.2.0-rc.2` |
| clean proof | cargo fetch/lock 一致；`cargo tree` 所有 avcodec workspace git packages 解析到同一 commit |
| strict Software/FFmpeg artifact | 待上游提供；dyun 侧在 FFmpeg 6.1 下仍复现 `BackendHintCapabilityMismatch`（UP4-002） |
| clean NV artifact | 待 NV 真机 |
| public API/support matrix | V3 `VideoSdk` / owned sessions / `VideoProfile` presets 与 Plan 3 保持一致 |

## dyun 接纳

| 字段 | 值 |
|---|---|
| dyun commit | `137b7b80896395cf8164e8c2172a345d9bc857fd` + pin update |
| manifest/lock SDK commit | `2068432426793c94cd5d415b56a4b2e9a3c1ee73` |
| toolchain/target | `rustc 1.94.1` / `x86_64-unknown-linux-gnu` |
| source/dependency guard | 通过；`dependency_contract` 预期 SHA 已同步为 `20684324` |
| NativeFree | 通过：`cargo test -p dg-media --locked --features avcodec-profile-native-free` 84 passed / 0 failed |
| Software | 部分通过：NativeFree 测试全通过；Software H.264 encoder create 失败（UP4-002） |
| Multi Profile | 部分通过：构建通过；3 个 Software encoder 相关测试失败 |
| NV Host/device-frame | compile-only 通过；真机待 GPU |
| external/zero-copy | NativeFree bridge / ownership 测试通过；device-frame 待 NV 真机 |
| artifact/checksum | `Cargo.lock` diff 仅 avcodec rc.1 -> rc.2，SHA 一致 |
| upstream issues | UP4-001 Closed；UP4-002 Open |

签字条件：tag、pin、lock同 commit；生产路径只有高层 SDK；软件和 NV真实媒体通过；结果回填上游 Plan 6。
