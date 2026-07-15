# 01. 执行契约与上游接纳门禁

## 1. 上游必须提供

- 不可变 `0.2.0-rc.1` tag 和解引用 commit；
- commit 已包含 Plan 5 diagnostics、Profile/report 和 residual fix；
- NativeFree、FFmpeg 6/7/8、NV Host/device-frame artifact；
- 推荐 API/feature/support matrix 和迁移说明；
- 未签字 RK/OneVPL/AMF 的明确状态。

缺少任一项时在 `UPSTREAM_ISSUES.md` 记录 blocker，不在 dyun 使用 branch 或本地 path 绕过。

## 2. 基线任务

```bash
git status --short
git rev-parse HEAD
rustc --version --verbose
cargo --version --verbose
rg -n 'avcodec.*(git|rev|version)' Cargo.toml crates -g Cargo.toml
cargo tree -p dg-media-avcodec -e features
```

记录旧 SDK revision、现有 feature、工作区测试状态和平台环境。先添加能证明旧 Factory V2/Registry/
descriptor 存在的 source scan，作为迁移前失败基线。

## 3. Revision 更新规则

manifest 使用上游维护者交付的不可变 commit；Cargo.lock source 必须与之相同。禁止仅更新 manifest 不更新
lock，禁止使用 `branch=main`。上游候选改变后，全部下游验收失效并重跑。

## 4. 禁止项

- 不复制上游源码到 dyun；
- 不直接依赖 backend/codec/sys crate；
- 不在 dyun 添加 backend 候选循环；
- 不用 `Box::leak`、unsafe Sync 或全局可变 Registry 解决生命周期；
- 不因 Software 环境缺失改成 NativeFree 成功并标记 Software 通过。

## 5. 完成条件

- [ ] RC tag/commit/artifact 已验证。
- [ ] dyun 基线和失败 source scan 已提交。
- [ ] manifest 与 lock 指向同一 SDK commit。
- [ ] 状态文件记录接纳决定。

