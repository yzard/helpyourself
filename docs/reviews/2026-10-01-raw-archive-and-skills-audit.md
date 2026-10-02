# 原始档案重构与全仓库 skills 审查

后续：F01/F02 已于 2026-10-02 整改并通过负载与生命周期验证，当前结论见[服务端补齐验收](2026-10-02-server-lifecycle-and-load.md)；下文保留发现当日的证据与状态。

历史记录：服务目录、OCR 架构和当前验证证据已由 [独立 OCR 服务审查](2026-10-01-backend-ocr-service.md) 更新；以下保留当时的实现及结果。

日期：2026-10-01。范围是整个 helpyourself 工作树（含此前未提交改动），不是 /home/zyin/dev 的其他项目。审查包括 Rust/SQLite、iOS、Python 辅助与测试、构建/Compose、文档和运行数据边界。适用技能已经读取；同名的 dev/skills 和 .agents/skills 版本也核对，不把无关平台/工具技能强套到本仓库。

结论：主档案重构和本环境验证完成；**尚不能宣称全部适用 skills 完全通过**。Axum 的阻塞工作隔离还有两项实际缺口；Swift 的完整 SDK/设备证据尚缺。原件目录、schema、归属、接口、调用者和导出/删除已对齐。详见下方逐项结果。

## 用户明确要求与当前实现

- API 是服务器主档案，SQLite 保存所有已接收健康载荷、OCR/解析回复、人工项目和修订。
- Apple Health 原始 JSON 在 data/raw/apple_health；照片原字节在 data/raw/photos；未来 health_connect 来源对应 data/raw/google_health；PDF 在 data/raw/documents。全部进一步按 user_id 和不透明 ID 隔离。
- HEIC/HEIF 原件与 JPEG 识别副本分别归档；PNG/JPEG 不统一重新压缩。扫描每页 PNG 单独上传，当前每页一份报告。
- schema.sql 直接定义当前 v6；按“playground 为空、无需 migration”移除旧升级 SQL/夹具。仅支持新库或 v6 重启，旧库明确拒绝、不自动删除。
- HealthKit 目录扩大到可用数量、分类、组合、临床类型及特殊样本；加入序列、CDA、活动环、特征和药物快照。保存可读样本 secure archive，而非只挑少量汇总字段。范围详见 [支持矩阵](../validation/support-matrix.md)。
- OCR/结构化 parser 分阶段保存完整最终 HTTP 回复，解析失败也保留。reports/extraction/get 可取回；报告索引仅查询元数据，避免读入全部回复正文。
- 手工/OCR 已确认 glucose 提供显式 HealthKit 写回；用户核对采样时间，先保存 API 新修订，再保存 HealthKit。其他报告字段仍全量在 API 保存，不强行映射错误的 HealthKit 类型。
- 导出 v2 收录 SQLite JSONL、辅助 CSV 及索引到的报告/健康修订原件。平台删除、报告删除、账户删除清理对应内容；墓碑防重放复活。

## 适用 skills 逐项核对

本表“符合”是当前审查范围与已执行证据的结论，不是对任意未测试输入的保证。技能路径使用本次读取的实际文件。

| Skill | 检查重点、仓库证据 | 结果 |
| --- | --- | --- |
| [add-modify-codebase](/home/zyin/dev/skills/add-modify-codebase/SKILL.md) | 新原件/处理字段、upload 参数和 checkpoint.requests 已更新所有调用者、Core、夹具及测试；移除旧迁移而非兼容包装 | 符合；原生 App 类型检查待外部验证 |
| [general-coding](/home/zyin/.agents/skills/general-coding/SKILL.md) | typed TOML、显式输入、校验早返回、重放冲突、错误/取消传播；不从业务代码添加新环境读取 | 符合所查契约；best-effort 本机退出撤销和 Drop 清理按其实际限制说明 |
| [project-structure](/home/zyin/dev/skills/project-structure/SKILL.md) | src/backend、src/ios，镜像 tests，docs，playground，build/dist；后端 Cargo 清单随组件，根 Package.swift 管理跨树 Core 测试 | 符合；没有平行旧源树或空 Android/OCR 服务 |
| [axum-server](/home/zyin/.agents/skills/axum-server/SKILL.md) | AppState/应用工厂、统一错误、认证提取、POST action/GET stream、durable worker、命名 SQL；原件目录不直接公开 | 部分符合；F01/F02 阻塞工作隔离仍需修复 |
| [sql-coding](/home/zyin/.agents/skills/sql-coding/SKILL.md) | 当前 schema、绑定参数、归属复合外键、明确事务、命名查询；无 migration 的用户政策优先 | 符合；新库/外键/重启、真实 SQLite 查询计划已核对 |
| [naming-conventions](/home/zyin/.agents/skills/naming-conventions/SKILL.md) | relative_path/processing_path/raw_path 跨 DTO、SQL、worker、导出及文档一致 | 符合；health_connect 是既有协议名，google_health 是用户指定磁盘目录，映射明确 |
| [restful-api-design](/home/zyin/.agents/skills/restful-api-design/SKILL.md) | /api/v1 组件操作 POST；下载 GET；新增 raw/extraction 读取均 owner scoped，重放/限额写入契约 | 符合；不因一般 REST 风格改变既有协议 |
| [docker-build](/home/zyin/.agents/skills/docker-build/SKILL.md) | 根入口、前台 playground、组件锁文件、BuildKit 缓存、镜像检查关口、ignore 排除运行数据、非 root/Caddy、真实隔离部署测试 | 符合；无发布/真实服务部署，iOS 构建独立 |
| [write-swift](/home/zyin/dev/skills/write-swift/SKILL.md) | Swift 6 严格并发、Approachable Concurrency、App 默认 MainActor、Core nonisolated；值类型 checkpoint 和原生 async query；回调共享操作通过 NSLock 同步且有取消/单次恢复 | 可移植部分通过；F03 仍缺 Apple SDK 类型检查和真机证据 |
| [code-formatting](/home/zyin/.agents/skills/code-formatting/SKILL.md) | 无 Python 项目配置，按 Black 120、skip string normalization/magic trailing comma 和相容 isort 检查全部现有 Python | 通过 |

同名技能对应 /home/zyin/dev/skills 下版本也已读取。Python 格式化两个版本的文字默认稍有差异，本仓库使用此前已验证的 Black-compatible isort 配置；代码行为要求一致。用户明确选择和已有项目契约优先于技能中的其他项目范例：不改成 Receipt Master 双服务/Android 布局，不替换已建立的持久会话摘要为 JWT，不公开明文 API，不恢复 migration。

## 尚存发现及验证缺口

| 编号 | 位置与影响 | 后续处理 |
| --- | --- | --- |
| F01 | src/backend/files.rs PendingFile、worker.rs Scratch、maintenance.rs ExportScratch 的 Drop 用同步 std::fs 清理；特别是递归清理可能阻塞 Tokio 工作线程 | 将临时文件生命周期纳入既有清理执行域并覆盖取消/退出/启动恢复，不能仅换成无归属 detached task；本轮如实记录，尚未修复 |
| F02 | src/backend/health.rs 同步大 JSON 编解码及批次摘要，files.rs 流式块摘要仍在 async 调用域；新上限使 Health payload 可达 32 MiB/批次 40 MiB | 在已有有界执行机制中隔离重 CPU 工作并测量并发内存/延迟；当前有限额和串行文件变更，但不满足 Axum 全部阻塞隔离要求 |
| F03 | Linux 只解析 App 源码；HealthKit 目录/特殊查询、严格 actor 设置、secure archive roundtrip 与写回尚无 Apple SDK 编译/真机证据 | 在 macOS 用 build_ios.sh 编译并运行 tests/ios，再真机覆盖权限、Watch、重试、删除及大序列；不把 Core 成功当作 iOS 成功 |
| F04 | 完整序列/CDA 快照有 24 MiB 上限，payload 32 MiB；超限会暂停该类型，活动环/药物快照也尚未测真实历史规模 | 后续分块附件/分页设计；当前报错且保留重试，不截断或伪称全历史成功 |
| F05 | HEIC 测试是合成容器头加 JPEG，OCR 使用合成 PDF/图片及模拟 HTTP 模型 | 真机原图和用户真实报告/配置模型验证仍待执行；这不是准确性或完整 HEIC 解码验收 |

F01/F02 是技能符合性缺口；F03–F05 是证据/能力限制，不能用自动测试通过掩盖。没有未经测量的延迟或完整性承诺。

## 未适用技能

- cpp-coding、fastapi-server、oop-code、refactoring、pyproject-toml：仓库无 C++/FastAPI/Python 业务类或 Python 包配置变更；Python 是开发与测试脚本，按 general-coding 和 code-formatting 检查。
- animate、animate-expo、animation-vocabulary、apple-design、find-animation-opportunities、review/improve-animations、各 UI/品牌/图像设计技能：本任务是数据档案与接口重构，没有动画或视觉改版要求；原生系统控件沿用现有 UI。
- imagegen、image-to-code、frontend-media-player、brandkit、图像生成移动/网页技能：无需生成素材或媒体播放器。
- documents、pdf、presentations、spreadsheets：本轮不制作 Office/PDF/电子表格交付物；应用接收 PDF 不等于调用文档制作技能。
- openai-docs、skill-creator、skill-installer、find-skills、plugin-management、template-creator、full-output-enforcement、grilling：未涉及 Codex/OpenAI 产品用法、技能安装创建、外部插件或指定工作流。
- pages、sites、wiki、visualize、work-pets：没有对应服务/知识库/可视化/宠物任务；仓库文档直接维护 docs。

这些不是“不通过”，是触发条件不适用。没有为了 audit 安装插件、修改其他仓库或启动代理。

## 本环境实际验证

| 检查 | 结果与证据 |
| --- | --- |
| Rust | 50 项测试通过；fmt、严格 Clippy 通过；含来源原件/历史、跨用户、重放、通配删除、HEIC 双输入、失败 OCR 完整回复、导出/账户删除 |
| Swift | build_ios.sh --core-only 全部源码语法解析；5 项 Core Swift Testing 通过，含字节/500 条批次边界与离线检查点 |
| 脚本/配置 | 6 项 Python 流程/网络辅助测试、isort/Black、bash -n、Compose config、git diff --check 通过 |
| Docker | build_docker.sh 成功生成 helpyourself:local；镜像构建内 fmt/Clippy/50 项测试/release 编译通过 |
| 合成部署 | tests/docker/deployment.py 在独立容器/卷完成可信测试 CA HTTPS、登录/上传/审核/趋势/原件/ZIP、重启持久化、停服复制恢复、报告/账户删除，结束清理测试资源 |
| SQLite 查询计划 | HEALTH_RANGE 使用 health_records_range，有局部排序；原件清理使用 health_revisions_record；索引列表按用户走复合唯一索引。不是大规模性能基准 |
| 未执行 | macOS/Xcode、签名、模拟器执行、iPhone/Watch、真实报告/实际 OCR 模型、真实临床准确性 |

可复查日志在 build/backend/2026-10-01-docker-build.log、build/backend/2026-10-01-deployment.log、build/ios/2026-10-01-core.log（运行产物，不进 Git）。未清空 playground 或任何用户数据库，未发布镜像，未提交、重置或重新暂存此前工作。

## 官方依据

目录和权限约束参考 [Apple HealthKit data types](https://developer.apple.com/documentation/healthkit/data-types)、[quantity identifiers](https://developer.apple.com/documentation/healthkit/hkquantitytypeidentifier)、[category identifiers](https://developer.apple.com/documentation/healthkit/hkcategorytypeidentifier)、[Health Records](https://developer.apple.com/documentation/healthkit/accessing-health-records)。类型存在与读取可用性需分开验证。

血糖写回去重参考 [HealthKit saving data](https://developer.apple.com/documentation/healthkit/saving-data-to-healthkit) 和 [sync identifier](https://developer.apple.com/documentation/healthkit/hkmetadatakeysyncidentifier)。不能据此声称任意 OCR 化验项目都能写成临床记录。
