# helpyourself

可自托管的健康档案系统：Rust + SQLite `backend_api`、独立 Qwen3.8 + NInfer `backend_ocr` 和原生 iPhone 客户端。手机连接 API，API 归档照片并调用 OCR 获取结构化候选；分析使用单独配置的模型接口。

Web GUI 由 API 直接提供，并嵌入 API Docker 镜像。打开服务器根地址即可登录，浏览报告、原件、指标趋势与已同步的健康明细，运行服务器配置的血脂 AI 预设。报告导入、复核及健康平台读写由移动端完成。[网页使用与验证](docs/runbooks/webgui.md)。

已实现报告上传/识别/人工复核、可追溯趋势、健康原始数据归档、完整导出删除和默认关闭的个人血脂分析。SQLite 保存健康载荷、OCR 完整回复和人工修订；原件分别存入 `data/raw/apple_health/`、`data/raw/photos/`，预留 `data/raw/google_health/`。当前 schema 直接重构，不提供历史迁移。

PDF 保留逐页文字与图像证据；当前 10 项指标支持常见单位换算，原始数值、单位和参考范围与标准化结果分别展示。[补齐计划与状态](docs/implementation-document-units.md)记录实施顺序及剩余验收。

后端及合成部署测试通过；iOS 模拟器构建、iPhone unsigned Release 构建及 HealthKit 模拟器主流程已验证。真机、真实传感器/临床记录和真实报告准确性仍待验收，设备安装需要签名。[iOS 验证记录](docs/validation/ios-simulator-2026-10-02.md)。

- [后端启动](docs/runbooks/backend.md)
- [OCR 服务配置](docs/runbooks/ocr.md)
- [iPhone 构建与试用](docs/runbooks/ios.md)
- [API 契约](docs/api.md)
- [最新配置：OCR 必须启用](docs/reviews/2026-10-03-required-ocr.md)
- [凭据与 skills 约束](docs/reviews/2026-10-03-embedded-credentials.md)
- [服务端生命周期与全仓库审查](docs/reviews/2026-10-02-server-lifecycle-and-load.md)
- [设计与实施计划](docs/README.md)

- [2026-09-27 历史结构审查](docs/reviews/2026-09-27-structure-audit.md)
