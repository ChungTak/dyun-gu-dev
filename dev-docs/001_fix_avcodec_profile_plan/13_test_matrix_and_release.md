# 13. 测试矩阵、发布门禁与回滚

## 1. 测试层级

### Unit

- metadata validation/mapping；
- codec/format/profile parser；
- Buffer ownership/copy accounting；
- core state machine。

### Contract

- MediaFrame↔Graph Packet 无损往返；
- Stream Track/AVFrame↔MediaInfo；
- avcodec Packet/Image↔dyun bridge；
- Profile Required/fallback 语义。

### Integration

- native-free H264/H265/JPEG roundtrip；
- decode→resize→encode graph；
- stream pull→decode 和 encode→stream push；
- CLI/C API config load。

### Hardware

- RKMPP Host；
- RKMPP DrmPrime→RGA DmaBuf→RKMPP；
- NV Host/device-frame；
- OneVPL/AMF Host。

## 2. 必测边界

- H264 Annex-B 与 AVCC、H265 Annex-B 与 HVCC 不混淆。
- PTS/DTS 负值、reorder、timebase 1/90000。
- key/lost/corrupt flags。
- 多 track stream_index。
- NV12/I420 odd dimensions、padded stride、offset overflow。
- external guard drop once。
- allow_staging=false 无 hook 调用。
- Again/Pending/flush/Error 所有组合。
- drain timeout 与 stop。
- fallback 只发生在带后缀 Profile。

## 3. 命令矩阵

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --no-default-features -- -D warnings
cargo test --workspace --no-default-features
cargo test -p dg-media --features avcodec-profile-native-free
cargo check -p dg-cli --no-default-features --features media,avcodec-profile-native-free
cargo check -p dg-capi --no-default-features --features media,avcodec-profile-native-free
cargo tree -p dg-cli -e features
```

系统/硬件 Profile 分独立 job，不加入无 SDK 默认 job。

## 4. 硬件 Gate

使用：

```text
AVCODEC_HW_TEST_LEVEL=off|probe|functional
AVCODEC_HW_TEST_BACKENDS=rkmpp,nvcodec,onevpl,amf
AVCODEC_HW_TEST_TIMEOUT_MS=<bounded milliseconds>
```

- 普通 CI 缺省 off，输出 skip 原因。
- probe job 必须实际创建 capability/probe report。
- functional job 选择 backend 后，关键测试 skip 视为失败。
- 报告记录硬件、驱动/runtime 版本、Profile、backend、format/domain 和 copy count。

## 5. 阶段提交

1. D0 工具链/依赖基线。
2. D1 MediaInfo/PacketMeta。
3. D2 Stream metadata。
4. D3 Profile features/config。
5. D4 Host bridge。
6. D5 async core/driver。
7. D6 Host end-to-end。
8. D7 上游 Profile V2 pin。
9. D8 RKMPP/RGA。
10. D9 NV/OneVPL/AMF、入口和发布。

每阶段必须可单独 review；不得用最终大提交掩盖中间不通过状态。

## 6. 回滚规则

- 可回滚某个 hardware Profile feature 或示例。
- 不得回滚 MediaInfo 运输、Again 状态机、设备缓冲错误和显式 staging 规则。
- 上游 revision 回滚必须证明仍满足所启用 Profile 的 API/contract。
- 兼容入口移除失败时可延长一期，不能恢复静默行为。

## 7. 全局完成条件

- [ ] 默认、native-free、software Profile 通过。
- [ ] metadata/bridge/state-machine contract 全通过。
- [ ] RKMPP/RGA functional 报告证明 image chain 0 copy。
- [ ] NV 文档、日志和 API 只称 device-frame。
- [ ] C ABI snapshot 无意外变化。
- [ ] 所有文档任务已附 commit 和测试证据。
- [ ] 无 vendor 路径、本地跨仓链接、未登记 TODO 或生产占位。

