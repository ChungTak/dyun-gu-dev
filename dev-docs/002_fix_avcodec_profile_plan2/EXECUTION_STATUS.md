# 002 执行状态记录

> 执行日：2026-07-14  
> 工具链：`RUSTUP_TOOLCHAIN=stable` → rustc/cargo **1.97.0**  
> avcodec-rs revision：`fc728aa9ea3e0a85401d2cd4de1b762ffcf92a51`  
> 相关 commit 起点：`4de8615`（升级 revision + Factory V2）+ 审查修复（本记录）

## 审查结论

**软件主路径：可视为完成。**  
**全计划（含硬件真机）：未 100% 完成**——真机 RK/NV/OneVPL/AMF 与 software 本机 FFmpeg 仍依赖环境。

### 审查中发现并已修复的严重缺陷

| 缺陷 | 影响 | 修复 |
|---|---|---|
| Processor 请求使用**未启用 processor** 的 base Profile descriptor | Host resize/CSC 在 Factory V2 下会 `RequestMemoryDomainMismatch` | `profile_to_sdk_descriptor_with_processor` + Host→Host 拓扑 |
| Decoder/Encoder/Processor config **未 stamp** Profile IO domain/`allow_staging` | zero-copy/device-frame 与默认 Host config 冲突，创建必失败 | `align_decoder/encoder/processor_config` |
| Element `memory_domain` 可静默覆盖 Profile 拓扑 | 破坏零拷贝声明 | `reject_memory_domain_conflict` |
| `legacy.rs` 仍含 **backend candidate 循环** 创建会话 | 违反 INT2-03 / Phase 06 | 删除 create_* 循环，仅保留 `hw`→Profile mapper |
| 示例仍写 “UP-03/UP-06 未落地” | 文档与 `fc728aa` 不符 | 更新 yaml 注释 |
| 缺失 `abi_snapshot.txt` | workspace `dg-capi` 测试失败 | 补齐快照 |
| Session build 错误未带 Profile/domain | 违反 Plan 12 可定位性 | `map_session_build_error` 解析 `AvError::WithContext` |
| Diagnostics 缺六方向 I/O 域 | 无法证明拓扑 | `MediaSessionDiagnostics` 导出 IO plan 字段 |
| AMF host 仍尝试 decode | 与“不保证 decode”声明不符 | `ensure_decoder` 对 `AmfHost` 直接 Unsupported |
| `hw=auto`→software 在仅 native-free 构建时硬失败 | 兼容入口不可用 | 唯一已编译 Profile 时回退并告警 |
| software 示例写 `allow_staging: true` | 与冻结 Host 拓扑 `false` 矛盾 | 更正示例注释 |

### 仍开放（非本机可关）

| ID | 说明 |
|---|---|
| HW-SKIP | 无 RK/NV/OneVPL/AMF 设备，真机 smoke/soak/`copy_count==0` 未跑 |
| **UP2-FFMPEG-01** | **FFmpeg 8 开发库已安装；bindgen 可用 `scripts/env-software-avcodec.sh`；上游 `avcodec-codec-ffmpeg` 因 `*const AVCodec` 编译失败。见 [UP2-FFMPEG-01.md](UP2-FFMPEG-01.md)** |
| SEND-TRANSCODE | `Registry` 非 `Sync`，融合转码仅库 API |
| UP2-TEST-01 | 上游 Transcoder diagnostics 偶发顺序失败 |

---

## 基线（Phase 0 / 01–02）

- [x] dyun 固定 avcodec-rs `fc728aa…`，Cargo.lock 与 manifest 一致
- [x] `dg-media-avcodec` 仅直接依赖 `package = "avcodec"`
- [x] 删除生产路径 backend 候选 / SessionBuilder / `create_*_with_trace`
- [x] `legacy.rs` 仅 hw mapper + deprecated（无 registry 遍历）
- [x] `source_scan` / `dependency_contract` 自动化

## 依赖与边界（Phase 1 / 03–04）

- [x] 十二个 `avcodec-profile-*` 四层一对一转发
- [x] `AvcodecSdkService` + `VideoSessionFactoryV2`
- [x] facade 重导出高层类型

## Profile / Factory（Phase 2 / 05–06）

- [x] Profile→`VideoBackendPolicy` + `VideoIoMemoryPlan` + `validate()`
- [x] **Config 与 Profile 域对齐**（审查修复）
- [x] **Processor 请求启用正确拓扑**（审查修复）
- [x] NV device-frame 无 processor；AMF 不保证 decode

## Bridge / Elements（Phase 3 / 07–09）

- [x] Packet/Image 桥接 + TransferReport
- [x] Decode/Encode/Resize 经 Factory V2（resize 现已可创建）
- [x] AsyncPump + native-free 真实媒体

## Transcoder / 诊断 / 入口（Phase 4–5 / 10–13）

- [x] `TranscodeCore` + 配置对齐
- [x] Diagnostics 拥有型快照
- [x] 图 `profile` / legacy `hw` 冲突与映射
- [x] 示例 yaml（注释已更新）
- [ ] 硬件真机验收

## 测试与发布（Phase 6 / 14–15）

- [x] CI `avcodec-profile-matrix`（native-free + software+FFmpeg）
- [x] 默认 workspace 无 codec 可测
- [ ] 硬件 soak / 发布签字

## 验证命令与结果

```text
export LIBYUV_TARGET=ubuntu-24.04_x86_64 RUSTUP_TOOLCHAIN=stable

cargo fmt --all -- --check
cargo clippy -p dg-media --features avcodec-profile-native-free --all-targets -- -D warnings
cargo test -p dg-media --features avcodec-profile-native-free
cargo test -p dg-media --features avcodec-profile-native-free,avcodec-profile-rkmpp-zero-copy --lib
cargo test --workspace
```

预期：全部通过。零拷贝/device-frame **域对齐单测**在 dual-feature 下通过；真机会话创建仍依赖设备。

## INT2 覆盖

| ID | 状态 |
|---|---|
| INT2-01 revision | 完成 |
| INT2-02 feature 转发 | 完成 |
| INT2-03 删除重复后端选择 | **完成**（legacy 循环已删） |
| INT2-04 Factory 创建 | 完成（含 config 对齐） |
| INT2-05 Packet/Image | 完成（主路径） |
| INT2-06 native-free 媒体 | 完成；software 待 FFmpeg |
| INT2-07 硬件 | **部分**（descriptor+对齐+无设备错误路径；真机未跑） |
| INT2-08 诊断/入口 | 完成 |
