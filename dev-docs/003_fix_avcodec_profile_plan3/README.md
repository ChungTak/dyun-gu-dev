# dyun-gu-dev 集成 avcodec-rs 0.2 高层 SDK 执行计划

## 1. 文档定位

本计划替代 Plan 2 中基于旧 revision 和 Factory V2 的接入方式。目标是固定 avcodec-rs Plan 5 产生的
不可变 RC，删除 dyun 对 backend policy、Profile descriptor、I/O topology、staging、Registry 和底层
Transcoder 的重复组装，只保留业务配置、媒体 bridge、pump、Element/Graph 和可观测性。

首批生产 Profile 为 NativeFree、Software、NVCodec Host、NVCodec Device-frame。RKMPP、OneVPL、AMF
在上游无真机签字前只保留配置识别和编译契约，不作为 dyun 生产支持能力。

## 2. 已确认问题

- 当前依赖固定旧 commit `fc728aa9ea3e0a85401d2cd4de1b762ffcf92a51`。
- `profile.rs` 自行实现 backend policy 和 I/O plan，且 NativeFree 错误映射为 Software policy。
- `session.rs` 仍使用 `default_registry_builder`、`VideoSessionFactoryV2` 和低层 config stamp。
- `transcoder.rs` 仍构造低层 `VideoTranscoderRequest` 并通过 `Box::leak` 延长 Registry 生命周期。
- `dg-media-avcodec` 仍暴露 `codec-*` 底层 feature alias 和低层 Factory 类型。

## 3. 需求编号

| ID | 要求 | 证据 |
|---|---|---|
| INT3-01 | 固定已验收的不可变 SDK RC | manifest/lock/hash |
| INT3-02 | 只直接依赖 `avcodec` | cargo tree contract |
| INT3-03 | 只转发 Profile feature | manifest/source guard |
| INT3-04 | 本地 Profile 只映射 `VideoProfile` | mapping tests |
| INT3-05 | 服务对象只持有 `VideoSdk` | lifecycle tests |
| INT3-06 | Decode/Encode 使用 Owned Session | real media tests |
| INT3-07 | 图片处理使用高层 Request | resize/CSC tests |
| INT3-08 | Transcode 使用高层 Session | transcode/thread tests |
| INT3-09 | bridge 保持所有权和零拷贝 | domain/copy tests |
| INT3-10 | 多 Profile 不串栈 | selected backend assertions |
| INT3-11 | 错误/report/diagnostics 不丢上下文 | snapshot tests |
| INT3-12 | legacy 配置有明确迁移期 | config tests/docs |
| INT3-13 | 首发软件/NV 生产验收 | media/hardware artifacts |
| INT3-14 | 未签字硬件不被广告 | capability/docs guard |
| INT3-15 | CI、发布和回滚可重现 | release record |

## 4. 文档索引

按编号顺序执行 [01](01_execution_contract_and_upstream_admission.md)～
[15](15_release_acceptance_rollback_and_handoff.md)。上游问题只写入
[UPSTREAM_ISSUES.md](UPSTREAM_ISSUES.md)，执行事实只写入
[EXECUTION_STATUS.md](EXECUTION_STATUS.md)。

## 5. 执行规则

1. 上游 RC 未满足接纳门禁时不得先重构生产路径。
2. 先写失败 source/dependency tests，再删除旧实现。
3. 不在 dyun 修补 SDK policy、domain、staging 或 backend fallback。
4. `allow_staging=false` 不允许 bridge 静默 copy。
5. 不增加容器 demux/mux；保持现有媒体帧边界。
6. 不引用 `vendor`；必要契约直接写入本文档。
7. 用户工作树中的无关改动必须保留。
8. 状态 Done 必须绑定 commit、命令和 artifact。

## 6. 全局完成定义

- [ ] INT3-01～15 全部完成。
- [ ] 生产源码不存在低层 SDK 组装符号。
- [ ] NativeFree、Software、NV 首发矩阵通过。
- [ ] 多 Profile 必须显式选择且 backend 不串栈。
- [ ] 真实 dyun 结果回传上游 handoff，形成 RC2 输入。

