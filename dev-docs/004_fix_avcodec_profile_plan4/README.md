# dyun-gu-dev Plan 4：avcodec-rs 生产验收与稳定升级

## 1. 定位

接纳上游不可变 RC 并完成 dyun 生产签字。首发范围：NativeFree、Software、NV Host、NV Device-frame。
RKMPP / OneVPL / AMF 保持 `unverified`。

## 2. 当前事实（完成态）

| 字段 | 值 |
|---|---|
| Plan-4 验收 pin | `dd3190008f2b544b51a74a9f4a225d52befc120a`（tag **`0.2.0`**） |
| **运行中 pin** | `cff861a8893c3391fafce7815f24be42cc9554d2`（见 `dg-media-avcodec`） |
| 前序 | `0.2.0-rc.3` / `0.2.0-rc.2` 不可变 |
| 矩阵 | Software / NV Host+device-frame 生产；NativeFree = unverified JPEG 路径 |
| 上游问题 | UP4-001 Closed；UP4-002 Verified |

## 3. 需求

| ID | 状态 |
|---|---|
| INT4-01～10 | **全部 Done** |

## 4. 文档索引

[01](01_current_state_and_rc2_admission.md)～[11](11_execution_order_and_final_acceptance.md)；
[EXECUTION_STATUS.md](EXECUTION_STATUS.md)；[UPSTREAM_ISSUES.md](UPSTREAM_ISSUES.md)；
[AVCODEC_RC2_ACCEPTANCE.md](AVCODEC_RC2_ACCEPTANCE.md)；[ROLLBACK.md](ROLLBACK.md)。

## 5. 完成定义

- [x] INT4-01～10 Done。
- [x] Plan-4 曾固定 stable `0.2.0` / `dd31900`；后续已前移，以 manifest 为准。
- [x] NativeFree/Software/组合/NV 通过。
- [x] handoff 与回滚记录完整。
