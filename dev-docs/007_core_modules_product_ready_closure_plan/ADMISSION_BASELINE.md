# CORE7-01 接纳基线

> 本文件保存 Plan 7 创建时的只读审计事实，不代表执行验收。

## 基线身份

| 字段 | 值 |
|---|---|
| 日期 | 2026-07-19 |
| HEAD | `feddd3add23ec8647f91b61fd3c15837342b790a` |
| 分支 | `main` |
| 工作树 | clean |
| Rust/Cargo | `1.94.1` |
| Host | `x86_64-unknown-linux-gnu` |
| Cargo.lock SHA-256 | `a8e90170594e0ae54295eb6fbf45433fc255e65bed57c5ffa07b29c7b890bb87` |
| C header SHA-256 | `fda5d29bd035210828edf7e3d8a872d51bd03c05a2b721becb12873974887648` |

## 只读验证

| 门禁 | 结果 | 说明 |
|---|---|---|
| `cargo fmt --all -- --check` | Passed | 当前工作树 |
| workspace clippy locked | Passed | 独立 target dir |
| workspace tests locked | Passed | 默认 SDK-free paths |
| cargo-deny | Not Run Locally | 本机未安装；当前 main CI 总结为 success |
| current main CI | Passed | GitHub Actions run `29683580914` |
| latest nightly | Failed | run `29674706044`，`reload-transitions` fuzz 失败；SHA 为 `a86413c` |

## 基线限制

- 默认 workspace tests 不启用 Cheetah、真实硬件或硬件 avcodec。
- 历史 OpenVINO CPU job 不替代新候选 release evidence。
- nightly soak 成功只证明重复 tests 两小时，不满足 Plan 7 soak 合同。
- Plan 6 acceptance 的 `1a9a0a5` 不是当前 HEAD，不能沿用其候选身份。

