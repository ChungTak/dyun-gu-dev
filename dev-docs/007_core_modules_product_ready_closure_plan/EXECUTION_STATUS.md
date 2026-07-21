# 007 执行状态 — **In Progress（核心软件合同接近闭环）**

## 审计基线

| 字段 | 值 |
|---|---|
| 计划创建日期 | 2026-07-19 |
| 计划创建基线 | `main@feddd3add23ec8647f91b61fd3c15837342b790a` |
| 当前审计 HEAD | 本地工作树（plan7 闭环修复，未提交） |
| 工作树 | dirty |
| 范围 | core product-ready closure；不新增 vendor capability |
| GraphSpec | 保持 `dg/v1`，process policy 为可信外层 |
| C ABI | views + runtime hard limits + header/examples + package smoke |
| 当前决定 | 核心软件路径已基本可验证；正式 Accepted 仍待 sanitizer 实跑与 24h soak 证据 |

## CORE7 状态

| ID | 状态 | Evidence | Blocker |
|---|---|---|---|
| CORE7-01 | Done | gap matrix / baseline | — |
| CORE7-02 | Mostly Done | CLI limits + C init + RuntimeOption 注入 | C deadline/pool 全字段可选 |
| CORE7-03 | Mostly Done | bounded model + pre-copy policy + `core7_policy_bridge` tests | device CAP |
| CORE7-04 | Partially Done | cancel metrics 导出 | 硬件 CAP |
| CORE7-05 | Mostly Done | BadFrame：yolo/resnet/retinaface/ppocr/bytetrack/softmax；readiness+drop 聚合指标 | 少量路径仍可继续统一 |
| CORE7-06 | Mostly Done | tokio timeout + policy open/bridge | 实网 CAP7-001 |
| CORE7-07 | Mostly Done | views 全主入口；package smoke 脚本 + CI job | CI runner 首次 green |
| CORE7-08 | Partially Done | sanitizers.yml | runner 实跑 artifact |
| CORE7-09 | Partially Done | soak candidate + nightly smoke | 固定 runner 24h |
| CORE7-10 | Mostly Done | `tools/package_smoke.sh` + ci/release 接入 | 跨 target 矩阵 smoke |
| CORE7-11 | Pending | acceptance Pending | 同一候选全量 evidence |

## 本轮（相对上次）新增

1. **Ops metrics schema v2**：`dg_graph_ready`、`dg_graph_has_root_cause`、`dg_graph_frame_local_drops_total`、`dg_backend_cancels_total`；readyz 二次校验 reconnecting。
2. **FrameLocal 扩展**：bytetrack / ppocr det / ppocr rec 坏帧 drop+continue。
3. **Package smoke**：`tools/package_smoke.sh` 验证 `.so.2`/SONAME/符号/C11 编译运行；接入 `ci.yml` 与 `release.yml`。
4. **Policy bridge 测试**：`dg-stream/tests/core7_policy_bridge.rs`。

## 本地门禁

- `cargo fmt --all`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

## 仍阻塞 Accepted

- R7-010：Miri/ASan/TSan CI 实跑
- R7-011：固定 runner 24h + 性能基线
- Capability：Cheetah 实网 / GPU / 硬件 codec
