# 004 执行状态 — **Plan 完成**

## 生产 pin

| Field | Value |
|---|---|
| Plan-4 验收 tag | **`0.2.0`** / `dd3190008f2b544b51a74a9f4a225d52befc120a` |
| **当前 dyun pin** | `cff861a8893c3391fafce7815f24be42cc9554d2`（main，**新于** 0.2.0；含 Plan 8） |
| crate version | `0.2.0` 线 |
| 权威来源 | `crates/dg-media-avcodec/Cargo.toml` + `Cargo.lock` |
| toolchain | rustc 1.94.1 / FFmpeg 8 / GTX 1070 |

## INT4-01～10

全部 **Done**（证据见 `AVCODEC_RC2_ACCEPTANCE.md`、本会话重验日志、CI）。

## 关键提交

| 仓 | 关键 SHA / tag |
|---|---|
| avcodec-rs | tag `0.2.0` → `dd31900`；RC3 `3f80f55`；UP4-002 `f3c1c04` |
| dyun-gu-dev | pin stable + device-frame bridge + NV gated tests |

## 遗留（非阻塞）

- RKMPP / OneVPL / AMF：`unverified`（首发范围外）
