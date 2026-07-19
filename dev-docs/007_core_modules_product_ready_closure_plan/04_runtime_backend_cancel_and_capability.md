# 04. Runtime、Backend Cancel 与 Capability

> 需求 ID：CORE7-04

## 1. 执行能力模型

backend capability 明确区分：

- `NativeAsync`：submit 非阻塞，poll 可返回 Pending/Ready/EOS；
- `BoundedSync`：同步调用有已验证最大时长，graph 通过有界 blocking worker 执行；
- `NonInterruptible`：不能在 deadline 内 cancel/wait，只允许 Unverified capability。

不得根据实现了默认 trait 方法就宣称 async/cancel。capability snapshot 包含 execution mode、
max in-flight、cancel support、external memory 和已验证设备范围。

## 2. Cancel 合同

`CancelReport` 保留 requested/completed/abandoned，并增加 unsupported/failed 诊断。规则：

- 每个成功 submit 恰好一次完成、取消或 abandoned；
- cancel 失败不提前减少 in-flight；
- shutdown deadline 到期保留可重试状态和首个根因；
- sync backend 不能用 detach worker 表示成功取消；
- backend 不支持 cancel 时 capability 明确，graph 在 product preflight 阶段拒绝要求可中断的配置。

## 3. Policy 与 Metadata

Runtime 在 backend init 前完成 model preparation，并在 backend probe 后验证：

- input/output count、rank、dtype、layout、device、stride 和 physical bytes；
- requested device/precision/deploy/zero-copy/cancel 与 live probe 一致；
- reshape 后重新验证全部 metadata；
- output metadata 变化不能绕过 tensor/frame limit；
- unsupported 不回退其他 device/backend，也不静默降级 execution mode。

## 4. Scheduler 与 Pool

- pool 全实例共享 metrics 和 process policy；
- affinity capacity/TTL 来自 process policy；
- lease release underflow/poison 进入 invariant failure；
- pool shutdown 对每个 Runtime 收集 cancel report；
- capability/product label 按 backend+device 的有限集合生成，不使用 model path 或 stream ID label。

## 5. 公共 Contract Test

为 mock 和每个 feature backend 复用同一测试套件：

- init/probe/metadata；
- submit/poll out-of-order、max in-flight；
- submit error、poll error、cancel success/failure/unsupported；
- reshape 后 metadata 与 policy；
- shutdown deadline、资源回零和 metrics 对账；
- external input Shared/Staged/Unsupported；
- capability 与实际行为一致。

无 SDK 的 job 只验证 adapter/type contract，并标 compile/mock；真实 capability 只有目标 runner 结果可关闭。

## 6. 支持矩阵

Core Software acceptance 只要求 mock 与可用 CPU backend 的公共合同。TensorRT/RKNN/Sophon/OpenVINO
GPU/NPU 等行在未取得实机结果时保持 `Unverified`，不阻塞未启用它们的核心制品。

## 7. 完成条件

- [ ] backend execution/cancel capability 与真实行为一致。
- [ ] Runtime 将同一 policy 传给 backend 并验证全部 metadata。
- [ ] shutdown 不把 detached/noninterruptible worker 视为成功。
- [ ] pool metrics/cancel report 与实例总和对账。
- [ ] support matrix 的每个 product-supported 行均有目标 runner 证据。

