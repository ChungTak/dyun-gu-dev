# 12. 多 Profile 与硬件生产接入

## 1. 首发支持矩阵

| Profile | dyun 首发状态 | 必须证据 |
|---|---|---|
| NativeFree | production | 真实 H.264/H.265/image |
| Software | production | FFmpeg 6/7/8 |
| NV Host | production | NV 真机 Host 媒体 |
| NV Device-frame | production | CudaDevice 零 staging |
| RKMPP/OneVPL/AMF | unverified | compile/error contract only |

## 2. 多 feature

同一 binary 可编译多个 Profile，但每个 Element/stream 必须保存显式 `VideoProfile`。NativeFree、Software、
NV 分别断言 selected backend；一个 Profile 的 backend 不得因 Registry 中存在其他 backend 而改变。

## 3. Fallback

只有 `*-fallback` Profile 可回退。report 必须证明 primary 不可用和实际 selected fallback。non-fallback
Profile primary 失败必须返回错误，dyun 不做二次尝试。

## 4. NV 零拷贝

Host 与 Device-frame 使用不同 bridge/session 路径。Device-frame public Image 必须为 CudaDevice external
handle，`allow_staging=false`，copy/staging 为零；不支持 processor 时配置失败。

## 5. 未签字硬件

可以在开发 build 开启 feature 验证编译，但 CLI/docs/capability 输出标记 unverified，不承诺生产 SLA。获得
上游真机签字后，只更新支持矩阵和 runner，不添加 dyun backend 组装。

## 6. 完成条件

- [ ] 首发 Profile 全部有真实证据。
- [ ] 组合 build backend 不串栈。
- [ ] fallback 事实来自 report。
- [ ] 未签字 family 未误广告。

