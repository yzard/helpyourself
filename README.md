# helpyourself

可自托管的健康档案系统：Rust + SQLite 后端、原生 iPhone 客户端，OCR 和分析连接用户自己的 OpenAI 兼容服务。

已实现报告上传/识别/人工复核、可追溯趋势、健康原始数据归档、完整导出删除和默认关闭的个人血脂分析。SQLite 保存健康载荷、OCR 完整回复和人工修订；原件分别存入 `data/raw/apple_health/`、`data/raw/photos/`，预留 `data/raw/google_health/`。当前 schema 直接重构，不提供历史迁移。

后端及合成部署测试通过；iPhone 完整构建、HealthKit 真机和真实报告准确性仍待外部验收，尚无可安装 IPA。

- [后端启动](docs/runbooks/backend.md)
- [iPhone 构建与试用](docs/runbooks/ios.md)
- [API 契约](docs/api.md)
- [最新实现与全仓库 skills 审查](docs/reviews/2026-10-01-raw-archive-and-skills-audit.md)
- [设计与实施计划](docs/README.md)

- [2026-09-27 历史结构审查](docs/reviews/2026-09-27-structure-audit.md)
