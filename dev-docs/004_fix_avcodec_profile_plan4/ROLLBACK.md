# 升级与回滚

## 当前生产

| 项 | 值 |
|---|---|
| tag | `0.2.0` |
| commit | `dd3190008f2b544b51a74a9f4a225d52befc120a` |

## 回滚到 RC3

```bash
# rev = 3f80f558e48ced6d3dc2c1e067307bfd12bec89d
# 同步 dependency_contract + cargo update -p avcodec
cargo test -p dg-media --locked --features avcodec-profile-native-free
```

## 回滚到 RC2

```bash
# rev = 2068432426793c94cd5d415b56a4b2e9a3c1ee73
# 注意：无 UP4-002 修复；Software 在 libavcodec 58 上可能失败
```

禁止只改 manifest 不改 lock/contract。禁止运行期回退低层 backend。
