# 2026-10-02 iOS 构建与模拟器验收

环境：Xcode 27.0（27A266a）、Swift 6.4、iOS 26.4、独立 iPhone 17 Pro 模拟器 `helpyourself-tests`（CE049AFA-1D5B-45B6-BB01-6BBE37C42CAA）。网络测试使用 `tests/ios/simulator_fixture.py` 的 loopback 合成 API；没有连接生产账户、OCR GPU 服务或分析模型。

## 已确认的问题与修复

- XcodeGen 生成工程到 build/ios 后，原来的 Info.plist 和 entitlement 路径指向不存在的文件。两者现在直接生成到 build/ios，测试目标自动生成自己的 plist；模拟器采用 ad-hoc 签名，使 HealthKit entitlement 在运行测试时存在。
- 临床 FHIR 的 `resourceType` 是 `HKFHIRResourceType`，归档时需要使用 `rawValue`；原实现无法通过 Apple SDK 编译。
- 普通 `HKUnit(from: "mmol/L")` 不兼容 HealthKit 血糖的质量/体积类型，会抛出 Objective-C 异常导致进程退出。写回改用携带 `HKUnitMolarMassBloodGlucose` 的摩尔单位。5.5 mmol/L 转换到 99.085734 mg/dL 的测试通过。
- 批量 Health 授权包含了 vision prescriptions、medication dose events 和 user annotated medications，导致 `NSInvalidArgumentException`。批量请求现在排除逐条授权类型和 medication dose events，另设“Select prescriptions and medications”入口；前台同步不自动弹出逐条选择页面。药物剂量的读取授权来自被选择的药物。[Apple 逐条授权说明](https://developer.apple.com/documentation/healthkit/hkobjecttype/requiresperobjectauthorization%28%29)、[药物 API 说明](https://developer.apple.com/videos/play/wwdc2025/321/)。
- 报告结果行与导出行的多个按钮使用 List 默认样式，一次点击会触发同一行多个动作。改用 borderless 样式，分别执行预览、Health 保存、导出下载与删除。
- 报告关系选项改为直接按钮，避免嵌套菜单无法稳定打开；照片导入改为菜单按钮控制父视图的 photosPicker，修复选择器不弹出。趋势预览改由父视图持有 sheet，并以数值类型读取 value，修复原件预览不弹出和数值为空。
- 旧 HealthKit 测试创建了不合法的嵌套 metadata，导致测试进程崩溃。测试改用 HealthKit 接受的 metadata，并验证 secure archive 还原后的 UUID、时间、单位和 metadata。

## 已通过的验证

| 范围 | 验证内容 |
| --- | --- |
| Swift Package | 7 项测试：JSON、服务器地址/传输限制、标准化与原始参考范围、磁盘检查点、路径穿越拒绝、Health 批次大小与完整性 |
| Apple SDK | 7 项测试：类型和单位兼容性、授权类型过滤、样本 secure archive、血糖采样时间与稳定重试标识、非法单位/非精确值拒绝、睡眠/血压关联/运动、真实 HealthKit 查询、同步失败检查点与恢复 |
| AppModel | 上述 SDK 测试中包含真实 Keychain 保存/恢复/清理、20 MiB 限额与原件配对拒绝、503 后重启重试并保留上传 UUID、原件下载、404 清除缓存、退出清理 |
| 界面主流程 | 登录、Keychain 恢复、文件/照片选择器打开与取消、五个标签页、双指标开关、analysis 关闭时按钮禁用、Health 授权取消、报告上下文保存、PNG 原件预览、核对采样时间后真实 HealthKit 血糖授权与保存、ZIP 下载及分享准备页面、退出 |

主流程界面测试 `testLoginTabsAndHealthAuthorization` 完整通过，98.264 秒。它明确检查 Blood Glucose 写权限、保存窗口关闭和 Health 页面成功状态，不把底层报告页面仍可见当作保存成功。

第二组界面测试 `testReportEditingEvidenceRelationshipsAndAnalysis` 完整通过：人工录入、修订历史、文档文字证据、原始 OCR 输出、报告关联/取消关联、带数据的趋势列表与原件跳转、分析创建/结果/反馈、导出删除、账户删除。

最终统一脚本全部通过：7 项 Core 测试、7 项原生 SDK/AppModel 测试、2 项界面测试，共 16 项。模拟器 build-for-testing、test-without-building 与 iPhone arm64 Release build 均成功。Release 产物是未签名 app，不是可直接安装的 IPA。

## 复现与产物

```bash
./build_ios.sh --test-destination 'platform=iOS Simulator,id=CE049AFA-1D5B-45B6-BB01-6BBE37C42CAA'
```

脚本启动 loopback 合成 API，并在退出时清理该进程。需要 Python 3、Xcode 与 XcodeGen，端口 18765 不应被其他进程占用。直接从 Xcode 运行 native tests 时，须先单独启动 `python3 tests/ios/simulator_fixture.py`。

最终完整日志保存在 `build/ios/validation/final-build-and-tests.log`；Xcode 的 `.xcresult` 保存在 `build/ios/DerivedData/Logs/Test/`。iPhone Release 产物为 `dist/ios/Helpyourself.app`（版本 0.1.0，arm64）。构建目录与 dist 不提交。

## 验证边界

合成 API 验证客户端的请求、持久化、展示和操作流程，不能替代真实 Rust 后端、OCR/分析模型准确性或真实部署端到端验收。文件/照片选择器验证了打开与取消；上传重试在 AppModel 层验证，没有从系统图库选择实际图片进行端到端上传。未将相机实拍、真实 Watch ECG/路线、医院临床 FHIR、真实处方及药物样本的端到端读取计入通过。普通类型的空结果不能用于推断读取授权。真实传感器/临床内容在该模拟器没有数据，当前测试没有伪称这些数据已读取成功。
