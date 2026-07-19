# Plan 7 Release Evidence Template

## 1. 候选身份

```text
dyun commit:
git describe:
dirty status:
Cargo.lock sha256:
ProcessRuntimePolicy schema/hash:
Graph API/schema/hash:
C ABI version/header hash:
C library SONAME/hash:
package/OCI ref:
package/OCI digest:
SBOM/provenance/signature:
risk register revision:
support matrix revision:
```

所有 required gate 必须引用以上同一身份。重建导致 digest 变化时创建新候选，不覆盖旧证据。

## 2. 构建与运行环境

```text
started/finished UTC:
runner identity/image digest:
rustc/cargo/nightly:
host/target/kernel/libc:
features:
dependency source/pins:
C/C++ compiler/linker:
Miri/sanitizer/model-checker/fuzzer:
CPU/memory/cgroup limits:
backend/device/driver/firmware:
protocol/upstream revision:
```

## 3. Effective Policy

分别保存可信进程硬上限、Graph 请求值、最终 effective 值和来源：

```text
config/model/include bytes and depth/count:
nodes/connections/workers/queue items+bytes:
tensor logical+physical bytes:
frame planes+bytes+dimensions:
device allocation/import bytes:
collector/cache/affinity/registry/sink/output:
connect/recv/send/drain/cancel/shutdown deadlines:
```

证据必须证明 Graph 不能放大 process policy，且超限在读取、复制、分配、导入或 SDK 调用前发生。不得保存
secret、token 或私有 endpoint。

## 4. Required Gate 结果

每条记录完整命令、开始/结束时间、exit code、passed/failed/ignored/skipped 数和不可变 artifact URL：

- admission、fmt、workspace clippy/test、deny、Cargo.lock；
- policy 入口一致性与 `limit-1/limit/limit+1`；
- model/tensor/frame/device pre-consumption；
- runtime/backend cancel、capability、永久 pending；
- graph/element error scope、metrics、readiness、reload；
- stream deadline/reconnect/pre-copy/shutdown；
- C11/C++17 package、symbol/SONAME/header/library、ABI misuse；
- Miri、ASan/LSan/TSan、并发模型；
- 每个 fuzz target、corpus revision、crash/minimized artifact；
- nightly 2h、release 24h、性能和 rollback。

required job 因环境缺失未运行时记 `Blocked`，不能记 Passed 或 success skip。

## 5. 长稳与故障曲线

保存 warmup、周期采样、峰值和结束值：

```text
RSS/allocator/device bytes:
threads/tasks/fds:
workers/queues items+bytes:
backend requests/in-flight/cancel outcomes:
affinity/cache/registry/collector/sink entries:
external callbacks acquired/released:
metrics cardinality/storage/scrape time:
accepted/dropped/rejected frames by error scope:
readiness/root cause transitions:
reload/reconnect/shutdown deadline outcomes:
```

附采样间隔、允许阈值、线性增长判断方法和原始数据，不能只保存最终摘要。

## 6. 性能与正确性

```text
baseline artifact digest:
candidate artifact digest:
workload/dataset hash:
throughput baseline/candidate/delta:
p50/p95/p99 baseline/candidate/delta:
RSS/device memory/thread/fd delta:
metrics scrape overhead:
copy count/bytes:
resource rejects:
output/reference mismatch:
threshold result:
```

## 7. Capability Evidence

对每个启用的 protocol/backend/device 单独记录：

```text
capability:
declared state:
hardware/network identity:
upstream/SDK/driver revision:
contract tests:
long soak:
known limitations:
evidence URL:
reviewer:
```

没有此节的能力只能为 `Blocked` 或 `Unverified`。

## 8. 风险与结论

```text
open core P0/P1:
accepted P2 exceptions + expiry:
required check failures:
blocked capabilities:
rollback result:
core acceptance: Pending/Accepted/Rejected
reviewer:
decision timestamp:
```
