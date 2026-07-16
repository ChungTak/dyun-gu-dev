# 07. External Memory Bridge 与零拷贝

## 1. 所有权模型

Host unique buffer 可 move；shared buffer clone并计 copy；external descriptor 携带完整 plane/stride/domain和
drop guard；device handle 只能通过 unsafe facade导入。Image/Packet 最后一个 clone drop时释放一次。

## 2. 安全边界

`dg-media` 保持 `forbid(unsafe_code)`；unsafe 仅位于 `dg-media-avcodec::external` 的小块，并在每块上方写明
allocation lifetime、handle validity、plane bounds 和 callback invariant。禁止直接暴露 Rust reference 跨 FFI。

## 3. Again/Session

submit Again 时 bridge object和 guard 必须继续存活，不能 move 后丢失；pending 输入被接受后才释放。reset/drop
清理所有 pending exactly once。Host/CudaDevice 转换没有显式 staging能力时返回错误。

## 4. 测试

Host move/clone、packed/planar bounds、stream metadata、drop once、Again retry、reset pending、same-domain share、
cross-domain reject/staging report、CudaDevice 无 Host dereference。

## 5. 完成条件

- [x] 所有权测试覆盖成功和错误路径。
- [x] unsafe 范围最小且有 invariant。
- [x] zero-copy 以 handle/domain/copy计数证明。
- [x] 业务层不接触 backend 私有 API。

