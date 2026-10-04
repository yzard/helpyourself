# helpyourself 设计与实施计划

更新：2026-10-03。状态：本地 API、SQLite、来源分类原件归档、独立 Qwen3.8/NInfer OCR 和随 API 交付的 Web GUI 已实现，服务端生命周期、并发负载及真实 GPU 合成图片/文字/扫描/混合 PDF 验证通过；真实样本/Apple SDK 真机验收尚未完成。数据库采用当前 schema，不提供历史迁移。

helpyourself 是可自托管的个人健康档案与分析系统。先把报告和健康平台明细变成可信、可追溯的数据，再验证主动健康风险发现。

## 阅读顺序

1. [产品范围与已批准决定](product.md)：做什么、分几阶段、哪些暂缓。
2. [系统架构与部署](architecture.md)：Rust、iPhone、SQLite、模型服务和 TOML。
3. [数据模型与计算规则](data-model.md)：归属、版本、原始数据、去重和删除。
4. [用户流程与接口契约](workflows.md)：导入审核、同步、趋势、任务及分析。
5. [实施任务与依赖](implementation-plan.md)：可独立验收的工作包与交付顺序。
6. [验证与发布关口](validation.md)：准确性、故障恢复、真实设备与 AI 验证。
7. [决定记录与参考资料](decisions-and-references.md)：事实依据、设计选择、待实测事项。

## 如何使用这些计划

- 用户批准的产品决定以 product.md 为准；其余文档给出实施基线，未实测能力明确标记。
- 工作包编号用于后续跟踪；完成时补上证据、限制及关联变更，不能仅凭代码存在勾选完成。
- 第一阶段完成后才能进入个人 AI 风险分析。公众版与 Android 不属于第一阶段。
- 所有后续设计、接口规格和验收记录继续放在 `docs/`。

## 当前交付

Web GUI 的范围与拆分见 [网页实施计划](implementation-webgui.md)，使用与验收见 [网页运行](runbooks/webgui.md)。页面随 API 镜像交付，移动端继续负责导入和平台读写。
本次网页的实现、验证和审查见 [Web GUI 验证记录](reviews/2026-10-03-webgui.md)。

两个服务的 `--data-dir`、固定 config.toml、独立挂载及旧 Playground 布局迁移见 [数据根目录验证记录](reviews/2026-10-03-data-roots.md)。

文档与单位补齐见 [计划及逐项状态](implementation-document-units.md)、[转换契约与依据](laboratory-units.md)。

已有数据闭环、模型适配、个人分析和部署实现。阅读 [后端运行](runbooks/backend.md)、[iPhone 构建](runbooks/ios.md)、[API 契约](api.md)。[最新全仓库审查](reviews/2026-10-02-server-lifecycle-and-load.md)区分已验证实现、skills 缺口和外部验收；[支持矩阵](validation/support-matrix.md)列出归档粒度与限制。真实健康数据不提交到仓库。

此前目录和构建调整见 [2026-09-27 历史审查](reviews/2026-09-27-structure-audit.md)。其中迁移、schema 版本和测试数量已被本次记录替代。

后台服务全部配置凭据直接内嵌其 config.toml；运行时没有独立密钥文件。当前配置与 skills 更新见[内嵌凭据审查](reviews/2026-10-03-embedded-credentials.md)。
