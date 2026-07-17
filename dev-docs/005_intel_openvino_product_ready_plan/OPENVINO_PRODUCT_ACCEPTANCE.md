# Intel OpenVINO Product Acceptance

> 本文件是发布接纳记录。实施前保持模板状态；不得预先勾选。

## 候选身份

| 字段 | 值 |
|---|---|
| dyun commit | `1bae8c85c732b08705c47cb26e9375bad66b77bc` |
| Cargo.lock hash | `375e51d95233f9e0114223895ca4d5dc40fa6ec3a432fd334cf89e9f24c2be5e` |
| OCI reference | 待 release 阶段生成 |
| OCI digest | 待 release 阶段生成 |
| OpenVINO version | `2026.2.1`（实施时复核） |
| GraphSpec API | `dg/v1` |
| C ABI | v1，待冻结 |

## 环境

| Gate | Runner/Device | Driver/Plugin | Result | Evidence |
|---|---|---|---|---|
| OpenVINO CPU | `devin-box` / x86_64 | OpenVINO 2026.2.1 (runtime 待安装) | Compile & unit tests pass | fmt/clippy/test/deny green |
| Intel iGPU | - | `/dev/dri` not present on this runner | Blocked | 需自托管 iGPU runner |
| Protocol E2E | `devin-box` | Cheetah/software codec | Pending | INT5-05/08 |
| 24h soak | - | 同 release OCI | Pending | INT5-10 |

## 接纳清单

- [ ] 所有 required checks通过且无 skip/soft-fail。
- [ ] CPU与iGPU使用同一模型/输入 fixture通过精度阈值。
- [ ] iGPU 不可用时显式失败，不回退 CPU。
- [ ] 4路 RTSP→OpenVINO GPU→RTMP E2E通过。
- [ ] stop、reload、reconnect、backpressure和SIGTERM故障注入通过。
- [ ] 24h soak和性能门禁通过。
- [ ] OCI scan、SBOM、provenance、签名和回滚通过。
- [ ] support matrix只宣称有证据的能力。

## 决定

`Pending / Accepted / Rejected`：**Pending**

决定人、时间、例外和到期日：待填写。

