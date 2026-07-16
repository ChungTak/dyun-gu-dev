# 004 执行状态记录

> 初始状态：未执行。允许 `Not started`、`In progress`、`Blocked`、`Done`。Plan 3 结果只作 baseline；
> Done 必须绑定 commit、命令和 artifact。

## Baseline

| Field | Value |
|---|---|
| Plan start dyun commit | 待填写（审查 `872b449`） |
| current SDK pin | `7faba6f`（post-RC1 main，不是 RC1 tag） |
| accepted RC2 | 待填写 |
| toolchain/target | 1.94.1 / 待 clean runner确认 |
| Software | historical pass，待 RC2重验 |
| NV | compile-only，待真机 |

## Requirement Status

| ID | Requirement | Status | Commit | Command/Artifact |
|---|---|---|---|---|
| INT4-01 | RC2 admission | Not started | — | — |
| INT4-02 | Revision/lock consistency | Not started | — | — |
| INT4-03 | Toolchain/environment | Not started | — | — |
| INT4-04 | High-level boundary | Not started | — | — |
| INT4-05 | NativeFree/Software | Not started | — | — |
| INT4-06 | Multi Profile | Not started | — | — |
| INT4-07 | NV production | Not started | — | — |
| INT4-08 | External/zero-copy | Not started | — | — |
| INT4-09 | CI/status/handoff | Not started | — | — |
| INT4-10 | Stable/rollback | Not started | — | — |

## Phase Log

### Phase 0 — RC2 admission
- Status: Not started
- Evidence/blockers: —

### Phase 1 — Pin and environment
- Status: Not started
- Commits/commands: —

### Phase 2 — Software revalidation
- Status: Not started
- Commands/artifacts: —

### Phase 3 — NV hardware
- Status: Not started
- Device/driver/commands/artifacts: —

### Phase 4 — Handoff and stable
- Status: Not started
- SDK/dyun commits/tags: —

## Final Blockers

- [ ] RC2 不可变候选可接纳。
- [ ] Software/组合在 RC2 上通过。
- [ ] dyun NV Host/device-frame 真机通过。
- [ ] 上游接受 handoff。
- [ ] stable/rollback完成。

