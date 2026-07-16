# 004 执行状态记录

> 允许 `Not started`、`In progress`、`Blocked`、`Done`。Done 必须绑定 commit、命令和 artifact。

## Baseline

| Field | Value |
|---|---|
| Plan start dyun commit | `137b7b80896395cf8164e8c2172a345d9bc857fd` |
| current SDK pin | `2068432426793c94cd5d415b56a4b2e9a3c1ee73`（`0.2.0-rc.2`） |
| accepted RC2 | Done — tag object `06ac7302…` / commit `20684324…` |
| toolchain/target | `rustc 1.94.1` / `x86_64-unknown-linux-gnu` |
| Software | FFmpeg 8.0.1 / libavcodec 62.11.100 全通过 |
| NV | GTX 1070 / driver 580.159.03；`DYUN_NV_HW=1` Host+device-frame 通过 |

## Requirement Status

| ID | Requirement | Status | Evidence |
|---|---|---|---|
| INT4-01 | RC2 admission | Done | tag `0.2.0-rc.2` / `20684324` |
| INT4-02 | Revision/lock | Done | manifest/lock/contract → `20684324` |
| INT4-03 | Toolchain | Done | 1.94.1；FFmpeg 8.0.1 |
| INT4-04 | High-level boundary | Done | source_scan / dependency_contract |
| INT4-05 | NativeFree/Software | Done | native-free 84；software-only 78；combo 90 |
| INT4-06 | Multi Profile | Done | multi_profile encoder isolation |
| INT4-07 | NV production | Done | Host encode/decode；device-frame create |
| INT4-08 | External/zero-copy | Done | bridge + device-frame `allow_staging=false` |
| INT4-09 | CI/status/handoff | Done | CI `--locked` + libavcodec≥62；handoff 文档；上游 ACK 外部 |
| INT4-10 | Stable/rollback | Partial | `ROLLBACK.md` 可重放；stable tag 不存在 |

## Phase Log

### Phase 0–1 — RC2 pin / env
Done. Pin `20684324`；env `scripts/env-software-avcodec.sh` 声明 FFmpeg 8.x。

### Phase 2 — Software
Done.
```bash
source scripts/env-software-avcodec.sh
cargo test -p dg-media --locked --features avcodec-profile-native-free
cargo test -p dg-media --locked --features avcodec-profile-software
cargo test -p dg-media --locked --features avcodec-profile-native-free,avcodec-profile-software
```

### Phase 3 — NV
Done.
```bash
DYUN_NV_HW=1 cargo test -p dg-media --locked --features avcodec-profile-nvcodec-host nvcodec_host -- --test-threads=1
DYUN_NV_HW=1 cargo test -p dg-media --locked --features avcodec-profile-nvcodec-device-frame nvcodec -- --test-threads=1
```

### Phase 4 — Handoff / CI / rollback
Done（dyun 侧）:
- CI：`--locked`、Software libavcodec major≥62 门禁、NV compile 与 runtime 证据分离
- 文档：`AVCODEC_RC2_ACCEPTANCE.md`、`ROLLBACK.md`、user-guide support 表
- 测试：software-only 不再硬编码 `native-free` profile

### Phase 5 — Upstream fix in local avcodec-rs（UP4-002）
Fixed candidate（已提交，未 pin 到 dyun 生产）:
- 本地提交：`avcodec-rs` `f3c1c04` — FFmpeg 58/59+ 门禁；Software `ffmpeg+jpeg`
- dyun 生产 pin 仍为 RC2 `20684324`（无 workspace patch）
- path 重验：native-free / software / 组合 / NV Host 通过
- 下一步：上游 push/tag；dyun 原子改 pin 并重跑矩阵

## Final Blockers

- [x] RC2 可接纳
- [x] Software/组合通过（FFmpeg 8；path 修复后 JPEG 亦可）
- [x] NV 真机通过
- [x] dyun CI/status/handoff 包
- [ ] 上游修复合入并打 tag（UP4-002 Fixed candidate）
- [ ] dyun 去掉 `[patch]` 并 pin 新 commit
- [ ] 上游 ACK / stable pin
