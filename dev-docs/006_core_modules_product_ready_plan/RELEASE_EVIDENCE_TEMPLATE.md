# Plan 6 Release Evidence Template

## 1. 候选身份

```text
dyun commit:
git describe:
dirty status:
Cargo.lock sha256:
Graph schema sha256:
C header sha256:
C library sha256/SONAME:
artifact/OCI ref:
artifact/OCI digest:
risk register revision:
```

## 2. 构建环境

```text
rustc/cargo:
host/target:
features:
base image:
dependency source/pins:
C/C++ compiler:
sanitizer/Miri/nightly:
```

## 3. ResourcePolicy

保存进程硬上限、图请求上限和 effective limits。至少包含：

```text
config bytes/include depth/include count:
nodes/connections/workers/queue:
tensor/frame/model:
collector/cache/affinity/metadata:
recv/drain/shutdown deadlines:
```

说明 runtime limits 的可信来源；不得记录 secret 或私有 endpoint。

## 4. 测试结果

每条记录完整命令、开始/结束时间、exit code、test/ignored/skipped 数和 artifact URL：

- fmt/clippy/test/deny/lock；
- limit boundary/property/Miri；
- runtime/scheduler/graph fault/concurrency；
- media/stream/elements；
- C11/C++17 ABI v2、symbol/SONAME、ASan/LSan/TSan；
- fuzz corpus/crash/minimized；
- nightly 2h；
- performance baseline/candidate；
- release 24h soak；
- shutdown/reload/reconnect 100 次；
- rollback smoke。

## 5. 长稳曲线

保存 warmup 与结束时：

```text
RSS:
threads/fds:
workers/queues/current bytes:
backend requests/in-flight:
affinity/cache/registry/sink entries:
external callbacks acquired/released:
metrics histogram storage:
readiness/root cause:
```

## 6. 性能与正确性

```text
throughput baseline/candidate/delta:
p50/p95/p99 baseline/candidate/delta:
metrics scrape overhead:
copy count/bytes:
resource rejects:
output/reference mismatch:
```

## 7. 风险与结论

```text
open P0/P1:
accepted P2 exceptions + expiry:
required check failures:
hardware blockers:
acceptance: pass/fail
reviewer:
```

