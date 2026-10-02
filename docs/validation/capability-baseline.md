# P00 能力基线

历史记录：服务目录、OCR 架构和当前验证证据已由 [独立 OCR 服务审查](../reviews/2026-10-01-backend-ocr-service.md) 更新；以下保留当时的实现及结果。

历史记录：保留当时的实现和测试事实；当前数据库策略、原件布局、HealthKit 范围及验证结果以 [2026-10-01 审查](../reviews/2026-10-01-raw-archive-and-skills-audit.md) 为准。

更新：2026-09-06。

| 项目 | 当前证据 | 状态 |
| --- | --- | --- |
| 后端 | Linux/Rust、SQLite v1→v5、46 项测试、严格 Clippy | 已验证 |
| 容器 | 最新 release、Poppler、隔离 Compose/Caddy HTTPS 和合成数据闭环 | 已验证 |
| Swift Core | Swift 6.2 编译、3 项 Swift Testing、所有 App 源码语法解析 | 已验证到不依赖 Apple SDK 的边界 |
| iPhone | SwiftUI、Keychain、HealthKit、扫描、分享源码和 XcodeGen spec | 没有 Mac/Xcode/签名/真机运行证据 |
| 报告 | 合成 PDF/PNG、PDF 渲染、模拟 HTTP 模型、复核/趋势 | 没有真实 Function Health 报告，准确性未验证 |
| 模型 | 视觉结构化及 Unlimited-OCR→parser 两路径，重试/引用校验 | 没有实际部署模型端点，性能与语义未验证 |
| HealthKit | 29 个类型源代码适配，服务器同步/来源/删除/聚合测试 | 真机支持列表尚未验收，不代表完整平台镜像 |

具体粒度见 [支持矩阵](support-matrix.md)，完整审核记录见 [本轮验收](phase-a-results.md)。P00 外部关口仍未全部完成；这些缺口不能靠模拟测试消除。
