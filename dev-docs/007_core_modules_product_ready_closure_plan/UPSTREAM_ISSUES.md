# Plan 7 上游与外部问题

## 1. 记录与状态规则

每项必须包含：ID、状态、影响的 CORE7/risk、上游固定 revision、最小复现、环境、期望/实际、typed error、
临时策略、owner 和关闭证据。

状态只使用 `To Audit / Reproduced / Upstream Open / Fixed Upstream / Qualified / Blocked`。上游修复合入不等于
产品资格完成；只有固定 revision 被 dyun 消费，并在相应真实 runner 通过合同与长稳，才可标 Qualified。

不得在 dyun 以 detached thread、无界 blocking wrapper 或复制完整上游实现作为永久 workaround。

## 2. 初始事项

### UP7-001 — Cheetah subscriber 原生 deadline 与 close wakeup

- 状态：`To Audit`
- 影响：CORE7-06，R7-004，Cheetah capability
- 问题：当前 adapter 为每次 timeout 创建 timer OS thread；需确认并补齐上游 subscriber 原生 deadline/cancel。
- 最小要求：timeout 可区分 TimedOut/EOS/error；close/cancel 在合同 deadline 内唤醒；反复 timeout 不增加线程。
- 临时策略：真实 Cheetah 产品路径保持 Blocked；mock/hub 证据只用于 Core Software。
- 关闭：固定 revision 的真实 connector timeout/close/reconnect/2h 与目标资格 soak 通过，线程/FD 曲线有界。

### UP7-002 — avcodec 解码分配前 frame 上限

- 状态：`To Audit`
- 影响：CORE7-03/06，R7-002/R7-003，avcodec capability
- 问题：确认 coded dimensions、plane/physical bytes 能否在上游内部大分配前被限制。
- 最小要求：已知 metadata 先校验；未知尺寸设置明确最大 dimensions/bytes；oversized 输入返回可分类错误。
- 临时策略：dyun 在 submit 与 poll output 两侧校验并诚实说明无法阻止的上游内部消费。
- 关闭：bounded API/hook 合入固定 revision，oversized/corrupt stream 和长流资源测试通过。

### UP7-003 — Vendor backend in-flight cancel 与 allocator 可观测性

- 状态：`To Audit`
- 影响：CORE7-03/04，R7-002/R7-005，device capabilities
- 问题：OpenVINO/TensorRT/RKNN/Sophon 的 cancel、wait deadline、allocation/import 和释放能力不同。
- 最小要求：逐 backend 声明 capability；cancel 返回结果；device bytes 与释放可计数；unsupported 不伪装成功。
- 临时策略：同步或不可取消 backend 保持 Blocked/Unverified；不影响不含这些 feature 的核心制品。
- 关闭：每个启用 backend 在对应实机通过共享合同、故障注入、正确性和 soak，分别引用证据。

### UP7-004 — Sanitizer 与硬件 runner 可用性

- 状态：`Blocked`
- 影响：CORE7-08/09/11，R7-010/R7-011
- 问题：TSan 与部分 SDK/driver 可能不兼容，固定性能和硬件 runner 也可能不可用。
- 最小要求：纯 Rust/core/C ABI required 路径必须运行；SDK 路径记录支持子集、最小不兼容复现和替代实机压力证据。
- 临时策略：环境缺失或不兼容标 Blocked，不能 success skip；Core Software 不启用无证据 capability。
- 关闭：required runners 在候选 SHA 连续通过，原始报告和环境身份由 acceptance 引用。

## 3. 新问题模板

```text
### UP7-NNN — Title

- 状态:
- 影响:
- 上游 revision:
- 环境:
- 最小复现:
- 期望:
- 实际:
- typed error:
- 临时策略:
- owner:
- 关闭证据:
```
