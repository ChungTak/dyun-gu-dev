# 09. 图片处理 Element 迁移

## 1. 支持范围

按上游 `ImageOp` 当前能力实现 resize、CSC、crop、rotate/flip；不要在 dyun 重新实现像素算法。组合操作若
SDK Request 只接受单 op，则由 Graph 串联多个高层 processor，不能调用 backend 私有 API。

## 2. 创建

根据业务配置构造 `ImageProcessorRequest`，通过 `VideoSdk::create_image_processor` 获得
`VideoImageProcessorSession`。删除 processor-enabled descriptor 和 Host→Host topology 拼装。

## 3. Profile 行为

NativeFree/Software/NV Host 使用 Profile 规定的 processor。NV Device-frame 不支持 Host processor，必须在
配置/创建阶段返回明确错误；不得隐式下载。RK 等未签字 Profile 只执行上游已声明行为。

## 4. 执行与测试

遵守 submit/poll/flush/reset；验证输出 format、尺寸、stride、metadata 和 diagnostics。测试 identity、缩放、
CSC、非法区域、unsupported op、Again/Pending、NV device-frame 拒绝和 NativeFree 真实像素结果。

## 5. 完成条件

- [ ] 不再构造低层 processor config/descriptor。
- [ ] Device-frame 无隐式 Host 路径。
- [ ] 输出和 metadata 可验证。
- [ ] Graph 组合规则明确。

