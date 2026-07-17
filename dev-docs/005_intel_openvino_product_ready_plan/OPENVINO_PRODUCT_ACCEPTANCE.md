# Intel OpenVINO Product Acceptance

> 本文件是发布接纳记录。实施前保持模板状态；不得预先勾选。

## 候选身份

| 字段 | 值 |
|---|---|
| dyun commit | 待填写 |
| Cargo.lock hash | 待填写 |
| OCI reference | 待填写 |
| OCI digest | 待填写 |
| OpenVINO version | `2026.2.1`（实施时复核） |
| GraphSpec API | `dg/v1` |
| C ABI | v1，待冻结 |

## 环境

| Gate | Runner/Device | Driver/Plugin | Result | Evidence |
|---|---|---|---|---|
| OpenVINO CPU | 待填写 | 待填写 | Pending | - |
| Intel iGPU | 待填写 | 待填写 | Pending | - |
| Protocol E2E | 待填写 | Cheetah/software codec | Pending | - |
| 24h soak | 待填写 | 同 release OCI | Pending | - |

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

