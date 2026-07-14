# 01. 执行契约、技术基线与依赖版本

## 1. 前置事实

- dyun 当前通过 `dg-media-avcodec` 依赖 avcodec SDK package，依赖固定在旧 revision。
- 当前 `rust-toolchain.toml` 和 workspace `rust-version` 使用不可从配置镜像获取的 `1.96.1`，构建在编译前失败。
- 使用已安装的 Rust `1.94.1` 并忽略错误 MSRV 声明时，当前 `dg-media --features avcodec` 可以完成检查。
- avcodec-rs 审计基线为 `36646c0ef15ad61f916a67d0f3e0df1ac17382b5`；最终硬件 Profile 必须固定到上游 plan2 完成 revision。

## 2. 冻结版本策略

1. 将根 `rust-toolchain.toml` channel 和 workspace `rust-version` 同步为 `1.94.1`。
2. 不使用 `stable`、branch、tag 漂移或 `--ignore-rust-version` 作为正式构建方案。
3. Host 阶段允许以审计基线验证 API；合并硬件 Profile 前必须将 avcodec git `rev` 更新为 plan2 README 登记的 40 位 commit。
4. `Cargo.lock` 与 manifest 在同一提交更新；`cargo update` 不得升级无关依赖。
5. 依赖更新后用 `cargo tree -e features` 保存每个 Profile 的实际 backend 清单。

## 3. 目录和分层约束

| 层 | 允许职责 | 禁止职责 |
| --- | --- | --- |
| `dg-core` | 中立媒体值对象、Buffer/ownership | avcodec 类型、backend 名称、I/O |
| `dg-graph` | Packet metadata 运输、element 驱动 | codec 选择、图像转换 |
| `dg-media-avcodec` | SDK re-export、unsafe 互操作边界 | graph element、协议类型 |
| `dg-media` | bridge、Profile adapter、Sans-I/O media core | demux/mux、厂商 FFI |
| `dg-stream` | track/AVFrame 与中立 metadata 映射 | codec backend 创建 |
| `dg-cli`/`dg-capi` | feature 转发、配置入口 | 重复媒体实现 |

## 4. 执行体任务

- [ ] 记录改造前 `git rev-parse HEAD`、`cargo metadata --no-deps` 和失败的默认 `cargo check` 输出。
- [ ] 把 Rust 版本改为 `1.94.1`，安装 rustfmt/clippy 后验证工具链可获取。
- [ ] 用 `cargo check --workspace --no-default-features` 建立无 SDK 基线。
- [ ] 用旧 avcodec revision 跑一次 `dg-media --features avcodec`，记录现状而不将其作为最终验收。
- [ ] 在上游 plan2 完成后读取其 README 中登记的完成 revision，并写入 `dg-media-avcodec` manifest。
- [ ] 检查最终 dependency package 仍为顶层 `avcodec`，不直接依赖底层 backend crate。
- [ ] 更新 lockfile 后确认只有预期 avcodec 依赖图和工具链相关差异。
- [ ] 建立 `dev-docs/001_fix_avcodec_profile_plan/evidence/` 约定：只提交文本报告和小型 fixture manifest，不提交构建产物。

## 5. 基线检查命令

```bash
rustc --version
cargo --version
cargo metadata --no-deps --format-version 1
cargo check --workspace --no-default-features
cargo check -p dg-media --no-default-features --features avcodec-profile-native-free
cargo tree -p dg-media -e features
```

## 6. 完成条件

- 默认命令不再尝试下载不存在的工具链。
- 无 avcodec 构建和至少一个 Host Profile 构建通过。
- 最终 manifest 使用精确 40 位 plan2 revision。
- 未引入本地 path dependency、底层 backend 直接依赖或无关 lockfile 升级。

