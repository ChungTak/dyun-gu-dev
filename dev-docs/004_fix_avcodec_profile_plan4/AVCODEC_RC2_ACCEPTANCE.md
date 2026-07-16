# avcodec-rs RC2 接纳与回传

> Plan 4 RC2 接纳记录。上游 tag、dyun pin、测试矩阵与 handoff 状态持续更新。

## 上游输入

| 字段 | 值 |
|---|---|
| RC2 tag / dereferenced commit | `0.2.0-rc.2` / `2068432426793c94cd5d415b56a4b2e9a3c1ee73` |
| tag object | `06ac7302f83a94fe40cb321c01bbc3cb794d9e64` |
| crate version | `0.2.0-rc.2` |
| clean proof | cargo fetch/lock 一致；`cargo tree` 所有 avcodec workspace git packages 解析到同一 commit |
| strict Software/FFmpeg artifact | dyun：FFmpeg 8.0.1 / libavcodec 62.11.100 下 Software 矩阵通过 |
| clean NV artifact | dyun GTX 1070 / driver 580.159.03 / CUDA 12.4；Host roundtrip + device-frame create |
| public API/support matrix | V3 `VideoSdk` / owned sessions / `VideoProfile` presets 与 Plan 3 保持一致 |

## dyun 接纳

| 字段 | 值 |
|---|---|
| dyun base commit | `14e2b6e`（pin RC2）+ worktree NV gated tests |
| manifest/lock SDK commit | `2068432426793c94cd5d415b56a4b2e9a3c1ee73` |
| toolchain/target | `rustc 1.94.1` / `x86_64-unknown-linux-gnu` |
| source/dependency guard | 通过；`dependency_contract` 预期 SHA `20684324` |
| NativeFree | 通过：`cargo test -p dg-media --locked --features avcodec-profile-native-free` 84 passed |
| Software | 通过（FFmpeg 8.0.1）：`--features avcodec-profile-native-free,avcodec-profile-software` 90 passed |
| Multi Profile | 通过：`multi_profile_encoder_backends_do_not_cross_stack` |
| NV Host/device-frame | 真机通过：`DYUN_NV_HW=1` Host H.264 256x256 Nv12 encode/decode；device-frame create + `allow_staging=false` |
| external/zero-copy | NativeFree bridge；device-frame no-staging 契约通过 |
| artifact/checksum | pin 与 lock 指向 `20684324` |
| upstream issues | UP4-001 Closed；UP4-002 Closed（环境：FFmpeg 8+） |

签字条件：tag、pin、lock同 commit；生产路径只有高层 SDK；软件和 NV真实媒体通过；结果回填上游 Plan 6。

## 回传摘要（待上游确认）

- RC2 pin 在 dyun 生产路径可用。
- Software 生产签字依赖 FFmpeg 8.x（libavcodec 62+）；4.4/6.1 下 encoder capability 探测失败。
- NV Host 真机 encode/decode 在 GTX 1070 通过；device-frame profile create 与 resize 拒绝契约通过。
- 待：上游 handoff 确认；`0.2.0` stable 发布后执行 INT4-10。
