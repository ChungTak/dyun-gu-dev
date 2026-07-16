# 004 执行状态记录

## Baseline

| Field | Value |
|---|---|
| current SDK pin | `3f80f558e48ced6d3dc2c1e067307bfd12bec89d` |
| SDK tag | `0.2.0-rc.3` (annotated; peels to pin SHA) |
| prior RC2 | `0.2.0-rc.2` / `20684324…` |
| toolchain | `rustc 1.94.1` / FFmpeg 8.0.1 / GTX 1070 |

## Requirement Status

| ID | Status | Evidence |
|---|---|---|
| INT4-01 | Done | RC2 admitted; production pin RC3 |
| INT4-02 | Done | manifest/lock/contract → `3f80f55` / `0.2.0-rc.3` |
| INT4-03 | Done | 1.94.1；FFmpeg 8 |
| INT4-04 | Done | source/dependency guard |
| INT4-05 | Done | native-free + software on RC3 |
| INT4-06 | Done | multi-profile isolation |
| INT4-07 | Done | Host + device-frame media (`DYUN_NV_HW=1`) |
| INT4-08 | Done | CudaDevice bridge zero-copy；Host encode reject |
| INT4-09 | Done | CI locked；docs；UP4-002 Verified |
| INT4-10 | Done | RC3 tag + pin + rollback docs；stable `0.2.0` 待 freeze |

## Final Blockers

- [x] RC3 tag + dyun pin + matrix
- [x] NV Host/device-frame 真机
- [x] UP4-002 Verified
- [ ] 上游 `0.2.0` stable（freeze 后；非 Plan4 阻塞）
