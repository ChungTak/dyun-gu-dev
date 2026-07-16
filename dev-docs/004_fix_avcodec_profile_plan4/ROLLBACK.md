# RC2 pin 回滚与 stable 升级演练

> INT4-10 部分交付：在上游 `0.2.0` stable 发布前，生产候选为 `0.2.0-rc.2`。
> 本文件记录可重放的 pin 回滚步骤；stable 升级在 tag 出现后按同一模板执行。

## 当前生产候选

| 项 | 值 |
|---|---|
| SDK pin（当前） | `3f80f558e48ced6d3dc2c1e067307bfd12bec89d` / tag `0.2.0-rc.3` |
| 前一 pin | `f3c1c04…`（UP4-002 无 version bump）或 `20684324…`（RC2） |
| SDK RC2 tag | `0.2.0-rc.2` / `20684324…`（不可变） |

## 回滚到 pin 前状态

同时恢复以下文件（不得只改 manifest）：

1. `crates/dg-media-avcodec/Cargo.toml` 中 `avcodec` `rev`
2. `Cargo.lock` 全部 avcodec workspace git packages
3. `crates/dg-media/tests/dependency_contract.rs` 预期 SHA
4. 相关示例 / capability 文档（若 pin 期间改过）

```bash
# 回退到 RC2 原 pin（无 UP4-002 修复）
# 将 crates/dg-media-avcodec/Cargo.toml rev 与 dependency_contract 改回
#   2068432426793c94cd5d415b56a4b2e9a3c1ee73
# 然后：
cargo update -p avcodec
cargo fetch --locked
cargo test -p dg-media --locked --features avcodec-profile-native-free
```

禁止：运行期切换低层 backend、隐式改变 Profile 语义、跳过 lock。

## 升级到 stable（待 tag）

1. `git ls-remote --tags … | grep '0.2.0$'` 确认 annotated tag 与解引用 commit
2. 原子更新 manifest `rev` / lock / `dependency_contract`
3. 重跑 plan4 最小命令矩阵（见 `11_execution_order_and_final_acceptance.md`）
4. NV：`DYUN_NV_HW=1` Host + device-frame
5. 更新 `AVCODEC_RC2_ACCEPTANCE.md` 为 stable 接纳记录并回传上游

## 验证回滚成功

- `dependency_contract` 与 lock 中 git rev 一致
- NativeFree 测试通过
- 无未提交的 `Cargo.lock` 漂移
