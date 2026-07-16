# avcodec-rs 生产接纳（Plan 4 终态）

## 上游身份

| 字段 | 值 |
|---|---|
| **stable tag** | `0.2.0` |
| **dereferenced commit** | `dd3190008f2b544b51a74a9f4a225d52befc120a` |
| crate version | `0.2.0` |
| 历史 RC3 | `0.2.0-rc.3` / `3f80f55…` |
| 历史 RC2 | `0.2.0-rc.2` / `20684324…` |

## dyun 接纳

| 字段 | 值 |
|---|---|
| pin | `dd31900…` / tag `0.2.0` |
| NativeFree | pass |
| Software | pass（libavcodec ≥ 58 + libx264） |
| Multi-profile | pass |
| NV Host | pass |
| NV device-frame | pass（CudaDevice external bridge；Host encode 拒绝） |
| UP4-002 | Verified |

## 回传

- 生产 pin：**stable `0.2.0`**
- RC tag 仅作考古；禁止重写
