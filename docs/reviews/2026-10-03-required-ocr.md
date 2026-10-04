# OCR 为必要服务

日期：2026-10-03。按用户要求移除 API `[ocr].enabled`。当前 `[ocr]` 只有 url、timeout_seconds 与内嵌 api_key；凭据约束沿用[内嵌 TOML 审查](2026-10-03-embedded-credentials.md)。分析服务的独立配置不属于本次 OCR 开关调整。

- API 配置加载始终校验 OCR URL、超时与有效密钥，旧 enabled=true/false 均被未知字段校验拒绝；空密钥模板必须配置后才可校验、建号或启动。
- 上传归档事务直接创建 queued 任务、error_code=null，不再创建 blocked/provider_disabled 后另行激活；旧激活方法、SQL 和关闭分支已删除。新库任务状态约束移除 blocked，当前 schema v7 与无 migration 约定保持。
- worker 始终处理提取队列，probe-providers 始终检查 OCR，server/status 始终声明 document_extraction=true；能力表示已实现功能，不是当前 OCR 可用性检查。
- OCR 临时不可达时，原件归档和人工复核继续可用，提取任务明确 failed/extraction_failed，服务恢复后可以重试；没有“关闭 OCR”的静默跳过路径。
- 源模板、Docker 模板和实际 playground API TOML 均删除该字段。初始化脚本会原子移除旧字段，保留已有密钥和其他设置；没有打印或轮换原凭据。
- iOS 删除过时的“OCR is off”显示，直接展示提取状态。

验证：75 项 Rust 测试、fmt 与严格 Clippy 通过，涵盖旧字段/空密钥拒绝、上传直接排队、跨用户隔离、故障原件保留和重试。配置准备 7 项及其他开发脚本共 13 项、前端 11 项测试通过；Black/isort、双 Docker 镜像构建通过。ReportsView.swift 的 Swift 6.2 语法解析通过，本轮没有再次运行 Apple SDK 或真机准确性验收。

隔离故障部署闭环通过：不设置 enabled，停止合成测试的 OCR 容器后上传仍归档，后台任务明确失败，人工复核/趋势/导出、重启、停服复制恢复和删除成功。默认部署测试现在实测这个故障流程，--ocr 仅选择是否执行真实 GPU 验证，不是服务功能开关。

真实 Qwen3.8 + NInfer GPU 验证通过：图片、文字 PDF、扫描／混合 PDF 的提取与单位／参考范围转换成功，模型惰性加载与闲置卸载成功，停止 OCR 后 API 继续提供归档和复核功能。HTTPS、重启持久化、停服复制恢复和删除闭环均通过。本轮没有运行浏览器交互或健康数据负载测试。

日志位于 build/backend_api/2026-10-03-required-ocr-{tests,clippy}.log 和 build/backend_ocr/2026-10-03-required-ocr-{build,outage,gpu}.log；仅使用隔离合成配置和数据，未部署用户 playground 或修改健康内容。
