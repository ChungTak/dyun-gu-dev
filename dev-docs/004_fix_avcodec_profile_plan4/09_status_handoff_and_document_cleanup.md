# 09. Status、Handoff 与文档清理

## 1. 状态修正

Plan 3 中“accepted SDK RC1=`7faba6f`”改为“post-RC1 main commit”；RC2 后填写真实 tag commit。INT3-13
保留 Partial 历史，INT4-07 只由 dyun NV 真机关闭。

## 2. 过时文档

删除/改写 `env-software-avcodec.sh` 关于 FFmpeg 8 pointer blocker仍存在的注释。Plan 2 UP2 条目保留历史，
明确已由当前上游修复。示例不得再出现 Factory V2、手工 domain或 staging。

## 3. Handoff

[AVCODEC_RC2_ACCEPTANCE.md](AVCODEC_RC2_ACCEPTANCE.md) 同时记录上游 tag/commit/artifact和下游 pin/tests。
回传上游 Plan 6 handoff；双方引用同一 dyun/SDK commit。

## 4. Capability

NativeFree/Software/NV 只有当前 RC2测试通过后标 production。RKMPP/OneVPL/AMF保持 unverified并在启动时
按现有策略告警。配置可识别不等于生产签字。

## 5. 完成条件

- [ ] RC 身份描述准确。
- [ ] Software blocker注释不再过时。
- [ ] 双向 handoff完整。
- [ ] support level与 artifact一致。

