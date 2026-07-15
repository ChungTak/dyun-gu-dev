# 05. VideoSdk 服务与生命周期

## 1. 服务形状

用 `AvcodecSdkService`（名称可保留）包装一个 `VideoSdk` 或其 clone。构造函数只调用 `VideoSdk::new`，测试
可注入预构造 SDK；服务不公开 Registry，不接受 BackendPolicy/descriptor。SDK 可在多个 Element 创建时共享，
每个返回 Session 自己持有保持 backend 存活所需的所有权。

## 2. 服务方法

方法仅转发 `create_decoder(profile, VideoDecoderRequest)`、`create_encoder`、`create_image_processor`、
`create_transcoder`，并将上游 build error 映射为 dg error。返回上游 `Created*` 或拆分后的 Owned Session+
Owned report，不返回 `Box<dyn Decoder>` 等 raw trait。

## 3. 生命周期

- service drop 后已创建 Session 继续有效；
- SDK clone 可在 worker 上创建独立 Session；
- 同一 Session 仅由其 Element/worker 可变拥有；
- flush 后 drain 到 EOS，reset 成功后才进入下一 generation；
- create 失败不残留半创建 Element；
- 禁止 Registry leak、unsafe Sync 和隐藏后台 pump。

## 4. 测试

service drop、clone 并发 create、创建失败原子性、Session move、flush/reset/error context。测试不得通过
`sdk.registry()` 绕过高层面。

## 5. 完成条件

- [ ] service 只持有 VideoSdk。
- [ ] 返回类型全部为高层拥有型类型。
- [ ] Registry lifetime workaround 删除。
- [ ] 生命周期测试通过。

