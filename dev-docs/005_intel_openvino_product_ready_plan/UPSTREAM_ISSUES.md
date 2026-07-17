# Plan 5 上游与外部问题

## 记录格式

每项包含：ID、状态、影响的 INT5、上游 revision、最小复现、环境、期望/实际、typed error、临时策略和关闭证据。
不得在 dyun 复制上游 backend/protocol 实现作为永久 workaround。

### UP5-001 — OpenVINO live device/plugin API

- 状态：`To Audit`
- 影响：INT5-06
- 问题：确认当前 `openvino` crate 是否安全暴露 runtime version、available devices和plugin properties。
- 策略：优先使用上游安全 API；缺失时在 `dg-openvino-sys` 增加最小边界并向上游提交需求。

### UP5-002 — OpenVINO async infer request

- 状态：`To Audit`
- 影响：INT5-07
- 问题：确认异步 start/wait/poll API及 tensor/request生命周期。
- 策略：不以线程包装同步 `infer()` 冒充原生 async capability；如需兼容包装必须明确 capability。

### UP5-003 — Cheetah reconnect/error passthrough

- 状态：`To Audit`
- 影响：INT5-05
- 问题：核对四协议 connector 的 retryable、timeout、cancel、readiness和source chain能否完整映射。
- 策略：dyun只实现产品策略与脱敏；协议状态机缺陷回传 Cheetah。

### EXT5-001 — Intel iGPU self-hosted runner

- 状态：`External Required`
- 影响：INT5-06、INT5-10
- 要求：稳定 `/dev/dri`、固定 PCI ID/driver、容器权限和 artifact retention。
- 关闭：required job连续通过并由 release acceptance引用；无设备 skip不能关闭。

