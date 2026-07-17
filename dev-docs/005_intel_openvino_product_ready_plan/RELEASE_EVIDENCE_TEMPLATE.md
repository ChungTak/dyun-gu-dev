# Release Evidence Template

## 1. 身份

```text
dyun commit:
git describe:
Cargo.lock sha256:
OCI ref/digest:
SBOM sha256:
signature identity:
```

## 2. 构建环境

```text
rustc/cargo:
target:
features:
base image digest:
OpenVINO runtime/plugin:
software codec/libs:
Cheetah revision:
```

## 3. 硬件环境

```text
CPU:
iGPU PCI ID/name:
kernel:
Intel driver:
/dev/dri permissions:
container runtime:
```

## 4. 测试结果

每条记录完整命令、开始/结束时间、exit code、测试数量、skip数量、artifact URL。至少包括：

- fmt/clippy/test/deny/fuzz；
- OpenVINO CPU/iGPU regression；
- protocol E2E与故障注入；
- SIGTERM/reload/reconnect；
- 性能 baseline/candidate；
- 24h RSS/thread/fd/request曲线；
- OCI scan/SBOM/signature verify；
- rollback smoke。

## 5. 运行诊断

保存脱敏后的 capability、selected device、backend/plugin、precision、in-flight、copy bytes、吞吐、p95/p99、
drop/backpressure/reconnect和最终 shutdown report。不得包含 token、URL userinfo或完整私有 stream key。

## 6. 结论

```text
required failures:
approved exceptions + expiry:
acceptance: pass/fail
reviewer:
```

