# 03. 依赖与 Feature 迁移

## 1. 目标 manifest

`dg-media-avcodec` 只有一个可选 direct dependency：`package = "avcodec"`、`default-features=false`、固定
RC commit。工作区其他 crate 通过 `dg-media-avcodec`/`dg-media` 使用 SDK，不再直接声明 avcodec backend。

## 2. Profile feature

保留一对一转发：NativeFree、Software、RK Host/fallback/zero-copy、NV Host/fallback/device-frame、OneVPL
Host/fallback、AMF Host/fallback。首发 CI required 集合只包含 NativeFree、Software 和 NV；其他 feature
做 compile contract，不标生产签字。

删除 `codec-*` alias。若外部用户仍使用这些 feature，作为破坏性迁移写入 CHANGELOG；不允许 alias 继续
开启低层 backend 绕过 Profile。

## 3. 依赖合同

`cargo tree --depth 1` 只看到 avcodec direct dependency。Cargo.lock 中出现 SDK 传递 crate 合法；失败条件是
dyun manifest 直接声明 backend/codec/sys crate，或生产源码 import 其符号。

## 4. 矩阵

- no profile：dg 默认 workspace 可编译，媒体 SDK 路径不可用；
- NativeFree；Software；NativeFree+Software；
- NV Host；NV Device-frame；NativeFree+Software+NV；
- 每个未签字硬件 feature 的 compile-only job。

## 5. 完成条件

- [ ] manifest/lock 固定同一 RC commit。
- [ ] 低层 feature alias 删除。
- [ ] dependency contract 自动化。
- [ ] 多 feature 不引入 dyun 侧 backend 选择代码。

