# 004 执行状态记录

## Baseline

| Field | Value |
|---|---|
| current SDK pin | `f3c1c04b87edd7b61e45feaf5adb3797bfa9ea5f`（UP4-002 fix on main；crate still `0.2.0-rc.2`） |
| prior RC2 tag | `0.2.0-rc.2` / `20684324…` |
| toolchain | `rustc 1.94.1` / FFmpeg 8.0.1 / GTX 1070 |

## Requirement Status

| ID | Status | Evidence |
|---|---|---|
| INT4-01 | Done | RC2 admitted; post-fix pin `f3c1c04` |
| INT4-02 | Done | manifest/lock/contract → `f3c1c04` |
| INT4-03 | Done | 1.94.1；FFmpeg 8 |
| INT4-04 | Done | source/dependency guard |
| INT4-05 | Done | native-free + software on `f3c1c04` |
| INT4-06 | Done | multi-profile isolation |
| INT4-07 | Done | `DYUN_NV_HW=1` Host + device-frame |
| INT4-08 | Done | bridge + device-frame no-staging |
| INT4-09 | Done | CI locked；docs；UP4-002 Verified |
| INT4-10 | Partial | `ROLLBACK.md`；stable `0.2.0` tag 未发布 |

## Phase 5 — UP4-002 pin

- Upstream pushed：`f3c1c04` → `origin/main` on avcodec-rs-develop
- dyun：manifest + lock + dependency_contract 原子更新
- 重验矩阵：native-free / software / combo / NV Host / device-frame create — all pass

## Final Blockers

- [x] RC2 + UP4-002 修复 pin 与矩阵
- [x] NV 真机
- [x] UP4-002 Verified
- [ ] 上游 formal RC3/stable tag（可选；当前用 main commit pin）
- [ ] 上游 handoff 外部 ACK
