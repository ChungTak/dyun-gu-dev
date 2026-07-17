# 02. 工具链、质量门禁与可复现构建

## 1. 工具链单一来源

以根 `rust-toolchain.toml` 为执行工具链、workspace `rust-version` 为 MSRV；CI 和 release 不得另写不同版本。
修正当前 release workflow 的版本漂移。clean runner 必须打印实际版本；镜像/镜像源失败按环境问题处理，
不能改用未知 stable 后声称精确工具链通过。

## 2. 基础门禁

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo deny check
git diff --exit-code Cargo.lock
```

先以独立 PR 修复 fmt，再启用 required check。构建脚本不得改写已跟踪文件；C header 再生必须有显式命令和 diff 检查。

## 3. Feature 矩阵

- 默认 SDK-free workspace；
- `product-intel = openvino + cheetah + avcodec-profile-software`；
- OpenVINO backend 单独 clippy/test；
- software codec 和 Cheetah connector 单独测试；
- 厂商 SDK feature 不使用 `--all-features` 伪造跨平台兼容。

`dg-cli` 与 `dg-capi` 都要转发产品 feature；`cargo tree -e features` 保存为 release evidence。

## 4. 供应链

为 `deny.toml` ignore 增加原因、依赖路径、负责人、到期日和移除条件。生成 license notice 和 SBOM；
git dependency 必须固定完整 rev，lock source 与 manifest 一致。新增依赖需说明维护状态和最小 feature。

## 5. 完成条件

- [ ] fmt/clippy/test/deny 在 clean runner 全绿。
- [ ] CI/release/MSRV 不再漂移。
- [ ] `product-intel` 可从 clean lock 构建。
- [ ] 构建前后 tracked files 与 Cargo.lock 无变化。

