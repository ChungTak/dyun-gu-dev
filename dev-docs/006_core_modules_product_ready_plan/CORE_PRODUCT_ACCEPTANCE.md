# Core Modules Product Acceptance

> 本文件是最终接纳记录。实施完成前保持 Pending，不得预先勾选。

## 候选身份

| 字段 | 值 |
|---|---|
| dyun commit | `1a9a0a595831db70a43f16f663ae5280a347d094` |
| Cargo.lock SHA-256 | `a8e90170594e0ae54295eb6fbf45433fc255e65bed57c5ffa07b29c7b890bb87` |
| GraphSpec API/schema hash | `dg/v1` / 稳定（YAML/JSON/TOML round-trip 通过 property test） |
| C ABI/header hash | v2 / `include/dg_capi.h` 与 `tests/abi_snapshot.rs` 同步 |
| library SONAME/hash | `libdg_capi.so.2`（Linux `-soname`） |
| OCI/artifact reference | 待硬件 release  runner 构建后回填 |
| artifact digest | 待硬件 release  runner 构建后回填 |
| risk register revision | `dev-docs/006_core_modules_product_ready_plan/CORE_RISK_REGISTER.md`（含 CORE6-10 追加记录） |

## 环境与门禁

| Gate | Environment | Result | Evidence |
|---|---|---|---|
| fmt/clippy/test/deny | clean Linux runner / stable 1.94.1 | Passed | PR #14~#26 CI；`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked`、`cargo deny check`、`git diff --exit-code Cargo.lock` 全绿 |
| Resource/Core contract | SDK-free + property/fuzz | Passed (CPU/mock) | `core6_properties.rs`、`<dg-core/tests/core.rs>`、`Buffer`/`Tensor`/`ExternalDropGuard` 单元测试；`cargo check --manifest-path fuzz/Cargo.toml` |
| Runtime/Scheduler/Graph | concurrency/fault runner | Passed (mock) | `core6_runtime_scheduler.rs`、`core6_graph_execution.rs`、`dg-scheduler` property tests、`dg-capi/tests/concurrency.rs` |
| Media/Stream/Elements | software/Cheetah runner | Passed (software) / External (Cheetah) | `dg-media --features avcodec-profile-native-free` 全绿；`core6_stream_io.rs`、`core6_media_bridge.rs`、`core6_elements.rs`；真实网络/Cheetah 长稳待硬件 runner |
| C ABI v2 | C11/C++17 + unit/fuzz | Passed (compile & unit) / Pending (sanitizer) | `dg-capi` 单元测试、ABI snapshot、4 个新 fuzz target 编译通过；ASan/LSan/TSan 待 nightly/sanitizer runner |
| Nightly 2h | fixed runner | Pending | `.github/workflows/nightly.yml` 已就绪，尚未触发 |
| Release 24h | candidate artifact | Pending | `tools/soak.sh` 已就绪，需候选制品与目标硬件 runner |
| Product hardware | per support matrix | External | GPU/NPU/dmabuf/VASurface 证据不在 CPU CI 范围内 |

## 接纳清单

- [ ] CORE6-01～11 全部 Done。
- [ ] P0/P1 风险全部 Closed。
- [ ] 每个资源限制在复制/解析/分配/导入前执行并有边界测试。
- [ ] external buffer/tensor 和 callback 所有权 exactly once。
- [ ] stream/backend/queue pending 均在正式 deadline 内 shutdown。
- [ ] pool metrics、histogram、affinity、cache、sink 和 registry 长期有界。
- [ ] Graph reload 故障注入证明 rollback 或明确 fail-closed。
- [ ] C ABI v2 unknown discriminant、view、owned result/error、destroy 和线程合同通过。
- [ ] Miri、sanitizer、并发模型和 fuzz 无报告。
- [ ] Nightly 2h 与 release 24h soak 通过。
- [ ] 性能、copy 和 metrics scrape 阈值通过。
- [ ] support matrix 只声明有实机证据的 backend/device。
- [ ] rollback 演练通过且前一制品可恢复。

> 当前 CPU/mock 与软件 profile 已完成；未勾选项阻塞于硬件/Miri/sanitizer/soak/performance 实机证据。

## 例外

| Risk ID | 等级 | 理由 | Owner | 到期日 | 监控/关闭条件 |
|---|---|---|---|---|---|
| - | - | - | - | - | - |

只允许 P2 例外；到期未关闭时 acceptance 自动回到 Pending。

## 阻塞项

| 阻塞项 | 等级 | 需要的证据 | 建议 runner |
|---|---|---|---|
| R6-002 tensor/frame/model 真实消费边界 | P0 | `MemoryPool`/device allocator 计数 +  soak 无泄漏 | GPU/NPU runner with `dg-elements` end-to-end |
| R6-003 stream 真实网络 long pending | P0 | Cheetah/real-RTSP 网络断开/重连 soak | Cheetah runner |
| R6-011 host allocation 与 cache 容量合同 | P1 | allocator failure + cache eviction soak | constrained-memory runner |
| R6-018 reload drain phase failure 注入 | P1 | injected phase failures + rollback proof | fault-injection runner |
| R6-019 bridge frame limit + typed conversion | P1 | real stream frame pre-copy rejection | Cheetah runner |
| Miri `dg-core` buffer/tensor/external guard | P0/P1 | `cargo +nightly miri test -p dg-core` 无 UB | nightly runner |
| ASan/LSan/TSan C ABI + scheduler/graph | P0/P1 | sanitizer build 无 report | nightly/sanitizer runner |
| Nightly 2h / Release 24h soak | P0 | 资源曲线 + 性能阈值 | fixed performance runner |

## 决定

`Pending / Accepted / Rejected`：**Pending（CPU/mock 完成，硬件与 sanitizer 证据缺失）**

- 决定人：待硬件验收后由 owner/reviewer 填写
- 时间：2026-07-18
- 适用制品：commit `1a9a0a595831db70a43f16f663ae5280a347d094`（CPU/mock 候选）
