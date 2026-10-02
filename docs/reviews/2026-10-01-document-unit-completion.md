# 文档证据、单位与参考区间补齐验收

后续：F01/F02 已于 2026-10-02 整改并通过负载与生命周期验证，当前结论见[服务端补齐验收](2026-10-02-server-lifecycle-and-load.md)；下文保留发现当日的证据与状态。

日期：2026-10-01。范围是 helpyourself 整个仓库中的本轮变更及已有 skills 发现复核。用户授权按[补齐计划](../implementation-document-units.md)逐项实施；未使用其他项目或代理，未提交 Git、发布镜像或启动用户 playground，也未改动真实健康数据。

结论：计划中 7 项文档输入/单位补齐已实现并通过本环境验收。API schema v7、导出 v3、document-evidence-v1、laboratory-page-v2、lab-units-v2 一致；没有迁移或旧接口兼容包装。**全仓库适用 skills 尚不能标成全部通过**：F01/F02 原有阻塞工作隔离仍需整改，Apple SDK/设备及真实报告准确性缺少外部证据。

## 实现与数据职责

1. PDF 所有页继续渲染图片并交给 OCR，同时提取 UTF-8 文本、归一化词坐标及原始 Poppler bbox XML；文字/无文字/仅页眉的混合页均保持整页视觉。文档文字是非可信证据，不能替代模型系统指令。
2. 文本 32 KiB、4096 词，XML 2 MiB，提取 30 秒；空文本/不可用/超限明确记录，没有静默截断后冒充成功。文本失败仍可视觉识别，任务记录需要人工核对。
3. extraction_inputs 保存每个 run/page 的完整已取得证据；reports/get 仅返回索引，reports/input/get 鉴权按需返回正文。和原始 OCR envelope 一起 owner scoped、导出、删除；取消/删除后不能发布晚到结果。
4. Python/Rust 非空坐标统一为 [left,top,right,bottom]；有限值、范围、日期和字段边界检验。模型 metric_id 保持 null，候选 pending，失败完整回复仍可取回。
5. 指标专属 Decimal 规则覆盖当前 10 项常见单位与受控别名，数值和参考边界分别产生可重算解释，含版本/规则/出处。原文及人工修订不覆盖；[转换契约](../laboratory-units.md)列明官方依据与保守拒绝条件。
6. 精确值、比较符和简单区间保持不同语义；只有精确点进趋势。复杂条件参考、未知单位和歧义数字保留原文及原因；不制造统一医学阈值。API、iOS 和 CSV 明确原单位/标准单位。
7. 修复任务续租并发：续租等待 SQL 写锁时保持提取 future 继续推进，避免暂停持锁事务；长于初始租约的 HTTP 识别回归成功，完成时续租 future 同步取消，无 detached 任务。

原件目录仍为 data/raw/apple_health、photos、documents 和预留 google_health。原 PDF 完整保留；页图片是可重建识别副本，文本/XML 和模型回复为派生证据。独立 OCR 无业务 data/SQLite 挂载，保持固定 Qwen3.8/NInfer 镜像、独立密钥、内部网络与惰性加载。

## 实际证据

| 检查 | 结果及边界 |
| --- | --- |
| Rust | 60 项测试、fmt 和严格 Clippy 通过。覆盖真实 Poppler 文字/无文字/混合 PDF，单位/独立参考单位/别名/比较符/区间/溢出/极小值，证据鉴权/跨用户/导出删除和长任务续租。 |
| Python OCR | 10 项测试、isort/Black 通过。新增非空数组坐标、非有限/越界/错误日期、非可信文字证据与视觉同时存在、拒绝部分失败文字层；原有进程/取消/队列测试保持。 |
| 开发脚本 | 8 项检查通过；统一 Docker 入口包含实际配置/网络/密钥及 iOS 失败流程测试。 |
| Swift | build_ios.sh --core-only 解析全部 App 源码，7 项 Swift 6.2 Core 测试通过，含原/标准参考展示与未知范围不伪造。Linux 不证明 SwiftUI/HealthKit Apple SDK 类型检查通过。 |
| 两个 Docker 镜像 | backend_api/backend_ocr 构建成功，构建内部执行相应测试与格式关口；NInfer/model 固定来源和 SHA 保持。 |
| 真实 GPU | RTX 5090 + Qwen3.8/NInfer 合成 PNG 识别 LDL 120 mg/dL；文字 PDF 识别 LDL 2.586 mmol/L，人工映射后趋势 100 mg/dL、参考上界 100 mg/dL，原值 2.586、原件字节、文本/词坐标/XML 和完整原生回复保留。 |
| 部署闭环 | 隔离临时配置/端口/卷，可信测试 CA HTTPS、下载/导出 v3、重启持久化、停服 data 复制恢复、报告/账户删除，惰性加载/空闲卸载和 OCR 停止后 API 响应通过。临时容器和卷已清理。 |

测试只使用合成输入。扫描/混合页的视觉保留由真实 Poppler + 模拟服务覆盖，不能称为模型真实扫描报告准确性验收。GPU 只实测上述两份合成报告。

运行日志不提交：build/backend_api/2026-10-01-document-units-tests.log、2026-10-01-document-units-clippy.log，build/backend_ocr/2026-10-01-document-units-build.log、2026-10-01-document-units-gpu.log，build/ios/2026-10-01-document-units-core.log，build/deployment-check/result.json。

## Skills 复查

本轮复查以下适用技能；项目专属 Receipt Master 条款不应用于 helpyourself，用户明确的 TOML/current schema/无迁移和既有会话契约优先。同名重复技能沿用[独立服务审查](2026-10-01-backend-ocr-service.md)已读取版本的对应要求。

| Skill | 本轮检查与结论 |
| --- | --- |
| [project-structure](/home/zyin/dev/skills/project-structure/SKILL.md) | 新 documents/laboratory 源码和镜像测试、Core 文件/测试、组件锁文件、docs/build 路径符合；旧目录不恢复。 |
| [add-modify-codebase](/home/zyin/dev/skills/add-modify-codebase/SKILL.md) | 必填 text_layer 更新所有生产者/消费者/夹具；移除旧转换入口；schema/导出/调用者/检查同步，回归实际行为。 |
| [general-coding](/home/zyin/dev/skills/general-coding/SKILL.md) | 显式参数、错误早返回、受限子进程与取消、失败证据不伪装成功；续租无独立遗留任务。 |
| [axum-server](/home/zyin/dev/skills/axum-server/SKILL.md) | 保持工厂/AppState/统一错误/持久租约，新增 owner scoped action；OCR envelope 解析/证据编码在 blocking 域。部分符合：原有 F01/F02 仍在。 |
| [fastapi-server](/home/zyin/.agents/skills/fastapi-server/SKILL.md) / [oop-code](/home/zyin/.agents/skills/oop-code/SKILL.md) | typed 必填证据/输出、单一运行时生命周期、取消传播/显式限额；不增加数据库或手机配置入口。 |
| [sql-coding](/home/zyin/dev/skills/sql-coding/SKILL.md) | schema v7、中央命名查询/参数绑定、run/page/owner 外键、事务内核验租约，导出和级联删除覆盖。 |
| [naming-conventions](/home/zyin/dev/skills/naming-conventions/SKILL.md) / [restful-api-design](/home/zyin/dev/skills/restful-api-design/SKILL.md) | input/extraction/interpretation 名称贯穿契约、调用者与文档；POST reports/input/get 和已建立 action 风格一致。 |
| [pyproject-toml](/home/zyin/.agents/skills/pyproject-toml/SKILL.md) / [code-formatting](/home/zyin/.agents/skills/code-formatting/SKILL.md) | 组件 Python 配置/锁保持，Black/isort 实际检查通过。 |
| [docker-build](/home/zyin/.agents/skills/docker-build/SKILL.md) | 双镜像构建和真实隔离 GPU 验收，无运行数据/服务密钥进入镜像，无发布或真实 playground 操作。 |
| [write-swift](/home/zyin/dev/skills/write-swift/SKILL.md) | Core 值格式化不实现重复换算，App 展示及所有调用者更新；7 项 Core 和语法解析通过，Apple SDK/设备未完成。 |

无对应触发条件的动画/品牌/素材生成、C++ 源码修改、Office/PDF 制作、Pages/Sites/wiki/宠物、OpenAI 产品/技能安装等不机械应用。接收 PDF 的服务实现不等于制作 PDF 文档交付任务。

## 尚未完成

- F01：files/worker/maintenance 的同步 Drop 清理；F02：Health 大 JSON 编解码、批次和上传块摘要仍在 async 域。新 OCR/证据 CPU 隔离修复不代表这些既有发现已全部解决；方案排入计划后续队列。
- iOS SDK/真机、真实 HEIC、真实报告漏项和数值/单位列错配评估仍缺外部输入与环境。
- 超大平台序列分块及条件化参考范围/其他指标属于后续能力；当前限额/保守拒绝行为可见，不能宣称 Apple 内部数据库全历史镜像或任意化验自动比较。
