# avcodec-rs RC 接纳与回传

## 上游身份

| 字段 | 值 |
|---|---|
| 生产 pin tag | **`0.2.0-rc.3`** |
| 生产 pin commit | `3f80f558e48ced6d3dc2c1e067307bfd12bec89d` |
| crate version | `0.2.0-rc.3` |
| 历史 RC2 | `0.2.0-rc.2` / `20684324…`（不可变） |
| UP4-002 | Verified in RC3 |

## dyun 接纳

| 字段 | 值 |
|---|---|
| manifest/lock | `3f80f55…` |
| toolchain | `rustc 1.94.1` |
| NativeFree | pass |
| Software | pass（libavcodec ≥ 58；libx264 for H.264） |
| Multi Profile | pass |
| NV Host | pass（GTX 1070） |
| NV device-frame | pass（Host Packet→CudaDevice Image；no Host staging） |
| CI | `--locked`；libavcodec major ≥ 58 |

## 回传摘要

- dyun 生产 pin：**RC3** `3f80f55` / tag `0.2.0-rc.3`
- RC2 保留为考古基线；不回写 tag
- 稳定 `0.2.0` 待 freeze 窗口后从 RC3 谱系切割
