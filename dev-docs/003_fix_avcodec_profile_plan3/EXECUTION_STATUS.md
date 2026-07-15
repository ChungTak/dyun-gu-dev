# 003 执行状态记录

> 初始状态：未执行。允许 `Not started`、`In progress`、`Blocked`、`Done`。Done 必须绑定 commit、命令和
> artifact；Plan 2 的结果不能自动继承。

## Baseline

| Field | Value |
|---|---|
| dyun start commit | 待执行者填写 |
| old SDK revision | `fc728aa9ea3e0a85401d2cd4de1b762ffcf92a51`（待确认） |
| accepted SDK RC/commit | 待填写 |
| toolchain/target | 待填写 |
| FFmpeg runners | 待填写 |
| NV runner | 待填写 |
| dirty worktree | 待填写 |

## Requirement Status

| ID | Requirement | Status | Commit | Command/Artifact |
|---|---|---|---|---|
| INT3-01 | Immutable SDK RC | Not started | — | — |
| INT3-02 | Single direct SDK dependency | Not started | — | — |
| INT3-03 | Profile-only features | Not started | — | — |
| INT3-04 | Thin profile mapping | Not started | — | — |
| INT3-05 | VideoSdk service | Not started | — | — |
| INT3-06 | Decode/Encode sessions | Not started | — | — |
| INT3-07 | Image processing | Not started | — | — |
| INT3-08 | Transcoder/Graph | Not started | — | — |
| INT3-09 | Bridge/zero-copy | Not started | — | — |
| INT3-10 | Multi Profile isolation | Not started | — | — |
| INT3-11 | Errors/reports/diagnostics | Not started | — | — |
| INT3-12 | Legacy migration | Not started | — | — |
| INT3-13 | Software/NV production | Not started | — | — |
| INT3-14 | Unverified HW gating | Not started | — | — |
| INT3-15 | CI/release/rollback | Not started | — | — |

## Phase Log

### Phase 0 — Upstream admission and failing guards
- Status: Not started
- Commits/commands/results: —
- Blockers: —

### Phase 1 — Dependency/Profile/Service
- Status: Not started
- Commits/commands/results: —
- Blockers: —

### Phase 2 — Elements and bridge
- Status: Not started
- Commits/commands/results: —
- Blockers: —

### Phase 3 — Transcoder, diagnostics and entrypoints
- Status: Not started
- Commits/commands/results: —
- Blockers: —

### Phase 4 — Real media, hardware and release
- Status: Not started
- SDK/dyun revisions: —
- Commands/artifacts: —
- Blockers: —

## Final Blockers

- [ ] SDK RC1 已接纳。
- [ ] 旧 Factory/Registry/descriptor 路径已删除。
- [ ] NativeFree/Software/NV 真实媒体通过。
- [ ] 多 Profile 和零拷贝通过。
- [ ] 上游 handoff 已回填并固定 RC2。

