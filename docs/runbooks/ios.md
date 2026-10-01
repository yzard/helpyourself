# iPhone 构建与试用

源码在 `src/ios/`，SwiftUI + HealthKit + PDFKit/VisionKit，目标 iOS 17 起，Swift 6 严格并发。Core 的 Swift Package 使用工具链 6.2；完整构建使用带对应 SDK 的 macOS/Xcode（例如 Xcode 26）和 XcodeGen。

当前 Linux 已编译 Core 并运行测试，也解析了全部 Swift 源码；没有运行 Apple SDK 类型检查、签名构建、模拟器或真机。因此本指南是交接步骤，不是已经交付可安装 IPA 的声明。

## 统一构建入口

在 macOS 安装 Xcode 与 XcodeGen，选择完整 Xcode 工具链后，从任意目录执行仓库的 `build_ios.sh`：

```bash
./build_ios.sh
# 可选：指定已安装的模拟器执行 Apple SDK 测试
./build_ios.sh --test-destination 'platform=iOS Simulator,id=YOUR_SIMULATOR_ID'
```

脚本运行 Core 测试，将 XcodeGen 工程生成到 `build/ios/`，编译模拟器测试目标，再构建 unsigned iPhone Release App。最终产物为 `dist/ios/Helpyourself.app`，中间文件在 `build/ios/DerivedData/`。这是未签名设备 App，不是可直接安装的 IPA；安装仍须自己的开发团队、Bundle ID 和 provisioning。

Linux 可执行 `./build_ios.sh --core-only`：在 Swift 6.2 容器内做全部 Swift 语法解析和可移植 Core 测试。未传此选项时 Linux 明确拒绝完整构建，不把 Core 检查当成 iOS build。

所有源文件在 `src/ios/`，测试镜像在 `tests/ios/`；没有旧平台树或兼容副本。生成工程后可在 Xcode 打开 `build/ios/Helpyourself.xcodeproj` 做签名和真机调试。XcodeGen [配置文档](https://github.com/yonaskolb/XcodeGen/blob/master/Docs/Usage.md)是相关设置的参考。

当前只验证了 Linux Core 路径和脚本失败分支。尚无 Apple SDK 构建、签名或真机结果。HealthKit 权限、真实记录与 Watch 来源必须再用真机验收。

## 试用流程

1. 先按后端指南建号，输入受信任的 HTTPS 服务器地址和凭据。HTTP 只允许 localhost 开发入口；不跳过证书验证，不跟随重定向转发 token。
2. Reports 导入 PDF/照片或扫描纸质报告。PNG/JPEG 保留所取文件原字节；HEIC/HEIF 同时上传原件和 JPEG 识别副本。扫描器返回的每页 PNG 单独上传，当前各页分别是一份报告。上传前持久化全部待上传字节和 UUID；响应丢失重用 UUID。待上传行可左滑丢弃。
3. 进入报告看原件、提取状态、页级文字、完整 OCR/解析回复和候选。点选候选，核对原值、单位、采样日期、参考区间、指标映射与出处，再选 Confirmed。也可人工加行、保留 Pending 或 Rejected，全部由 API 保留历史。
4. Trends 选择指标，可同时比较第二项，各自使用独立纵轴。点击图表附近的点或日期行回到原件页。
5. Health 连接权限并同步。每个查询页拆分出的全部批次先落盘，全部确认后推进锚点；下次进入前台或手动同步继续。样本 secure archive、可读序列和快照都归档到 API。类型随 OS/权限动态可用，范围与缺口见支持矩阵；临床记录读取另需 Health Records entitlement 和设备支持。
6. Insights 仅在服务器启用 analysis 后允许创建个人血脂回顾，查看引用、缺失资料、其他解释和就医问题，记录反馈。下拉刷新查看后台任务结果。
7. Settings 创建、刷新、下载并分享完整 ZIP，也可删除导出、报告或账户。

已确认 glucose 项提供显式 Save to Apple Health：仅接受精确数值及 mg/dL 或 mmol/L。用户须核对真实采样日期和时间；先将该时间作为新修订保存 API，再请求 HealthKit 写权限并保存血糖样本。稳定 sync identifier/version 用于重试及修订去重。日期级旧修订仍保留，不默认伪造采血时间。若已启用 Health 同步，保存后尝试回读归档；否则需用户开启同步。其余报告项目保持 API 全量归档，不将 LDL/HbA1c 映射成错误的膳食类型。此写回流程尚待 Apple SDK/真机验证。

服务器报告删除与 Apple Health 样本删除是独立操作；当前删除报告不会替用户删除已经写入 Apple Health 的样本。

## 本机数据

token 存 Keychain，ThisDeviceOnly，不进入普通偏好文件。缓存按服务器地址和 user_id 的摘要分目录，包含最近刷新时间、已打开原件/报告和待上传队列，排除 iCloud 备份并使用 iOS 文件保护。成功刷新列表时清除服务器已删报告的缓存。

退出立即清理本机缓存和 Keychain，并尽力撤销服务器会话；离线撤销失败时旧服务器会话等到期或管理员撤销。本机无云端强制清除能力：离线设备保留的旧缓存需重新连线刷新、退出或删除 App；手动分享出的 ZIP 不会被远程删除。

HealthKit 读取授权无法用授权状态 API 判定；用户拒绝和没有样本可能同样呈现为空。实现显示未知而非声称已授权，依据 [Apple 授权文档](https://developer.apple.com/documentation/healthkit/authorizing-access-to-health-data)。当前没有后台 observer 推送，采用前台/手动可靠路径，不承诺后台实时同步。
