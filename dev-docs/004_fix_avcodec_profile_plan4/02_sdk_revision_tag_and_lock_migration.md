# 02. SDK Revision、Tag 与 Lock 迁移

## 1. 原子更新

将 `dg-media-avcodec` avcodec `rev` 改为 RC2 解引用完整 SHA；执行 cargo update 只更新该 git source及必要
传递依赖；同步 dependency contract 中预期 SHA。manifest、lock、测试不得处于不同 commit。

## 2. 身份校验

记录 RC2 tag、tag object、dereferenced commit、Cargo metadata crate version和 lock source。禁止把 crate 自报
rc2但未打 tag 的 main commit称为 RC2。禁止 branch、短 SHA和 floating tag。

## 3. Lock 合同

CI 使用 `--locked`；运行前后 `git diff --exit-code Cargo.lock`。验证所有 avcodec workspace git packages
解析到同一 source fragment。保留经上游验证的依赖版本；若 `shiguredo_nvcodec/amf` 更新破坏编译，记录
上游问题而不是修改 dyun backend。

## 4. Dependency tree

`dg-media-avcodec` depth 1 只有 avcodec；NativeFree 排除 FFmpeg/硬件；Software只激活 FFmpeg/libyuv；NV
只激活对应 Profile 依赖。Cargo.lock 中传递 crate 合法。

## 5. 完成条件

- [x] manifest/lock/contract 同一 RC2 SHA。
- [x] `cargo fetch --locked` 和 metadata 通过。
- [x] direct dependency 仍只有 avcodec。
- [x] pin 更新形成独立可回滚 commit。

