# Plan 6 上游与外部问题

## 1. 记录规则

每项包含：ID、状态、影响的 CORE6、上游 revision、最小复现、环境、期望/实际、typed error、临时策略、
owner 和关闭证据。不得在 dyun 复制完整上游 runtime/protocol 实现作为永久 workaround。

## 2. 初始事项

### UP6-001 — Cheetah subscriber timeout/cancel

- 状态：`To Audit`
- 影响：CORE6-06，R6-003
- 问题：确认上游 subscriber recv 是否支持 deadline、close wakeup 和 runtime shutdown。
- 最小要求：recv timeout 可区分 TimedOut/EOS/error；close 在 100 ms 内唤醒；不创建 detached thread。
- 临时策略：能力不足时真实 Cheetah 产品路径保持 Blocked，不用无界 blocking wrapper。
- 关闭：固定 revision 的真实 connector timeout/close/fault test 通过。

### UP6-002 — avcodec allocation/frame limit propagation

- 状态：`To Audit`
- 影响：CORE6-02/03/06
- 问题：确认 decode/processor/encode 在 SDK 内部分配前能否接收 frame/physical bytes limit。
- 最小要求：已知 output metadata 先校验；未知尺寸 decode 设置最大 coded dimensions/bytes；错误可分类。
- 临时策略：dyun 在 submit 和 poll output 两侧校验，但不能宣称阻止上游内部先分配。
- 关闭：上游 hook 或明确 bounded API 合入固定 revision，并有 oversized stream 测试。

### UP6-003 — Vendor backend cancel contract

- 状态：`To Audit`
- 影响：CORE6-04/05
- 问题：OpenVINO/TensorRT/RKNN/Sophon 对 in-flight cancel、wait deadline 和资源释放能力不同。
- 最小要求：backend capability 诚实标记；cancel 返回报告；不支持者不能宣称可中断 product path。
- 临时策略：同步/不可取消 backend 只保留 experimental/unverified，硬件计划分别验收。
- 关闭：各启用 backend 的公共 cancel contract 与实机故障测试通过。

### UP6-004 — Sanitizer/hardware runner

- 状态：`External Required`
- 影响：CORE6-10
- 问题：TSan 与厂商 SDK/driver 可能不兼容，且硬件 runner 资源有限。
- 最小要求：纯 Rust/core/C ABI 路径必须跑 sanitizer；SDK 路径至少跑 ASan 可支持子集与长期资源计数。
- 临时策略：工具不兼容必须保存最小复现，不能把 job 标 success skip。
- 关闭：required runner 连续通过并由 acceptance 引用。

## 3. 新问题模板

```text
### UP6-NNN — Title

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

