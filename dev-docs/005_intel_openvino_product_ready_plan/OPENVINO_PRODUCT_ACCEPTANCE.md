# Intel OpenVINO Product Acceptance

> 本文件是发布接纳记录。实施前保持模板状态；不得预先勾选。

## 候选身份

| 字段 | 值 |
|---|---|
| dyun commit | `main@eec7f97`（INT5-03/04/05/06/07/08/09/10 全部合入） |
| Cargo.lock hash | `d0af95554d5d890e5372e4290d888f1f27999acb8bf90ca563c94247dea9f614` |
| OCI reference | 待 release 阶段生成 |
| OCI digest | 待 release 阶段生成 |
| OpenVINO version | `2026.2.1` |
| GraphSpec API | `dg/v1` |
| C ABI | v1（`dg_abi_version` 稳定导出） |

## 环境

| Gate | Runner/Device | Driver/Plugin | Result | Evidence |
|---|---|---|---|---|
| OpenVINO CPU | GitHub Actions `ubuntu-latest` x86_64 | OpenVINO 2026.2.1 | Passed | `openvino` CI job：真实 `[1,4]` identity IR load → infer → compare；`fmt`/`clippy`/`test`/`deny` 全绿 |
| Intel iGPU | - | `/dev/dri` not present on this runner | Blocked | 需自托管 iGPU runner |
| Protocol E2E | `devin-box` / mock loopback | Cheetah/software codec | Passed (mock path) | `dg-stream` 集成测试 + `product-intel` clippy/compile-only；真 RTSP/RTMP 端到端待 iGPU runner |
| 24h soak | - | 同 release OCI | Pending | INT5-10 OCI 已定义；soak 计划待 release 后执行 |

## 接纳清单

- [x] 所有 required checks 通过且无 skip/soft-fail（本地与 CI `fmt`/`clippy`/`test`/`deny` 15/15）。
- [x] OpenVINO CPU 使用同一模型/输入 fixture 通过精度阈值（absolute/relative error + cosine similarity）。
- [x] iGPU 路径代码已实现并在无设备时显式失败，不回退 CPU（`DeviceKind::IntelGpu` 映射 `GPU`，能力探测不支持时报错）。
- [ ] 4路 RTSP → OpenVINO GPU → RTMP E2E 通过（需 iGPU runner）。
- [ ] 24h soak 和性能门禁通过（需 release OCI + runner）。
- [ ] OCI scan、SBOM、provenance、签名和回滚通过（CI workflow 已配置，待 release 触发）。
- [x] support matrix 只宣称有证据的能力（本文件与 `docs/user-guide.md` 一致）。

## 决定

`Pending / Accepted / Rejected`：**Conditionally Accepted（CPU 路径通过；iGPU 实机、24h soak 与 release OCI artifacts 待验证）**

决定人：Devin AI Engineer；时间：2026-07-17；例外：iGPU runner、24h soak、release OCI scan/signature；到期日：由维护者在取得 iGPU runner 与 release artifacts 后重新评估。
