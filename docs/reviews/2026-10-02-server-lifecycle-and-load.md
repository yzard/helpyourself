# 服务端生命周期、负载与 OCR 补齐验收

日期：2026-10-02。用户要求继续补齐 Apple iOS 准确性验收之外的事项，Mac 构建及 iOS 真机由用户负责。本轮落实此前 F01/F02 服务端 skills 发现，追加真实模型扫描/混合 PDF 和 Health 大载荷验收；未修改本轮 iOS 源码、提交 Git、发布镜像或操作用户真实 playground 数据。

此前文档输入与单位处理见[补齐计划](../implementation-document-units.md)和[历史验收](2026-10-01-document-unit-completion.md)。当前 schema v7、导出 v3、document-evidence-v1、laboratory-page-v2、lab-units-v2 保持；日聚合与分析快照统一升级为 health-daily-v2。用户明确无需 migration，未添加迁移或清库行为。

## F01：临时资源归服务生命周期所有

- 新 TemporaryFiles 为上传、raw 暂存、OCR scratch、导出暂存注册 lease；阻塞闭包保留使用权。取消请求只释放自己的 lease，不会删除仍在被使用的文件。
- 删除由既有维护周期通过异步文件 API 执行，不在 Drop 中同步删除，也没有 detached 清理任务。删除失败保留注册和持久清单供重试。
- 服务启动取得独占 data 锁后恢复 tmp；符号链接只删除链接，不跟随至外部文件。正常停服先停止 HTTP 和 worker，再等待已准入 CPU 闭包、清理临时/持久文件、关闭 SQLite。
- ZIP 闭包同时持有原件变更锁和临时 lease，取消不能让删除流程提前移除正在读取的原件。CPU panic 不伪装成功，容量仍会释放。

回归覆盖取消后容量/文件仍受保护、等待闭包的停服、运行中文件保留、无活跃 lease 清理、中断目录恢复、路径越界、符号链接以及失败上传在服务退出时清理。F01 已整改。

## F02：大载荷有界执行和背压

Database 共享 CpuExecutor，使用 Tokio 既有 blocking 执行器，最多两个重 CPU 任务；HTTP 满时返回 429，固定的持久 worker 可等待准入，不建立无限请求 CPU 队列。闭包持有许可直到实际结束，即使 HTTP 调用方已经取消。上传保持两个入口槽；Health 同步在缓冲正文前取得两个共享入口槽，明细读取/聚合也使用该准入域。密码运算沿用独立槽。

Health JSON 解析、整个批次校验、摘要、单项限额和原始 envelope 编码先在 CPU 域完成，再进入 SQL 事务。摘要采用流式编码，避免额外分配完整 40 MiB 摘要输入。晚到的非法项不会部分提交或改变 coverage；批次、来源版本、删除抑制和原始字段语义保留。上传改为先流式写文件，再在 CPU 域读取摘要/检查内容。大 Health 明细/聚合/页面证据/OCR envelope 的解析和输出编码、PDF bbox 解析、推理请求编码、导出 JSONL 和 ZIP 同样进入共享执行域。

测试以两个永不完成的输入流验证正文读取前准入、第三请求 429、状态接口响应和取消后重新准入。临界值测试接受 32 MiB payload 与 40 MiB 批次，超出一字节返回 413，SQLite、原件索引和 coverage 不发生部分写入；原 envelope 和同批次重放保持。F02 已整改。

## 追加的结果语义修复

真实混合 PDF 推理中，模型正确返回了四个结果，但附带“文字层只有页脚”和“采样日期来自页级”的提醒。此前 API 因 warnings 把有效提取判成任务失败。本轮将执行失败和复核需求分别表达：有效结构可以使 job 成功，相关 page 标 needs_review，候选仍 pending、metric_id=null；完整 warnings 和原生回复保留。网络、结构、字段校验失败继续失败，不会通过隐藏错误完成任务。

health-daily-v2 的 sample_count 统计实际参与计算的记录，source_record_count 与 excluded_sample_count 分别给出来源候选和排除数。缺失/错误单位/不适用类别不计入有效样本；数值溢出返回 value=null、value_status=numeric_overflow。稳定均值和 i128 时间差避免大有限数/极端时间造成中间溢出。聚合 SQL 只投影可计算字段，不把无关巨大字符串带入派生内存，原始 JSON 不改。EXPLAIN QUERY PLAN 验证继续使用 health_records_range(user_id, record_type, start_at, end_at)，来源/记录排序使用 SQLite 临时排序。

## 实际验证

| 检查 | 结果 |
| --- | --- |
| Rust | 71 项测试通过，fmt 和严格 Clippy 通过；含本轮生命周期、限额/原子性、背压、提醒语义、聚合异常值与分析版本回归。 |
| OCR Python | 10 项测试通过，isort/Black 通过；本轮 OCR 组件协议和模型未变。 |
| 开发脚本 | 构建入口运行 8 项测试通过；部署脚本 Black/isort 通过。 |
| 双 Docker 镜像 | backend_api/backend_ocr 本地构建成功，Rust 镜像关口重新执行 71 项测试及 fmt/Clippy；OCR 检查层对应未变源码。 |
| RTX 5090 + Qwen3.8/NInfer | 合成 PNG：LDL 120 mg/dL；文字 PDF：LDL 2.586 mmol/L→100 mg/dL；两页扫描/混合 PDF：每页 Glucose 5.551 mmol/L 和 ApoB 0.9 g/L，四项全部返回，人工映射后为 100 与 90 mg/dL，参考边界也转换正确。 |
| PDF 证据与原件 | 扫描页 text_layer=empty，混合页 available 且只有页脚文字，仍识别完整图片两行。原 PDF 字节、text/bbox XML、完整模型回复、警告和所有修订随导出保存。 |
| 部署闭环 | 可信合成 CA HTTPS、人工复核/趋势、JSONL/原件 ZIP、重启、停服 data 复制到独立卷恢复、报告及账户删除通过。验证惰性加载、空闲卸载、停止 OCR 后 API 仍响应；测试只清理自己的随机配置/容器/卷。 |
| Health 并发负载 | 两并发合成请求，正文分别 33,554,754 和 41,943,040 字节；原始归档、重放、临界值和超限原子拒绝通过。当前测量见下。 |

负载实测：API RSS 从 36.92 MiB 到采样峰值 234.02 MiB；11 次状态请求，p95/最大延迟均 2.54 ms。该测试是当前机器两个并发请求期间的短时观测，不是长时间吞吐、稳定内存上界或生产 SLA。输入均为合成数据，没有上传个人健康报告。

日志及合成结果位于 build/backend_api/2026-10-02-tests.log、2026-10-02-clippy.log、2026-10-02-health-query-plan.json，build/backend_ocr/2026-10-02-tests.log、2026-10-02-server-build.log、2026-10-02-server-deployment.log 和 build/deployment-check/{result,load-result}.json，不提交仓库。复现命令见[后端运维](../runbooks/backend.md)和[OCR 运维](../runbooks/ocr.md)。

## Skills 复查与验收范围

本轮继续按[全仓库适用性审查](2026-10-01-document-unit-completion.md)复核，落实其中两个尚未整改的服务端发现。没有触发条件的动画、品牌、C++、Office/PDF 文档制作、Pages/Sites 等技能不机械应用。通用技能中的 Receipt Master 特例不适用本仓库；保留 helpyourself 的显式 TOML、持久会话、durable worker 和无迁移约定。

| Skill | 本轮证据 |
| --- | --- |
| [add-modify-codebase](/home/zyin/dev/skills/add-modify-codebase/SKILL.md) / [general-coding](/home/zyin/dev/skills/general-coding/SKILL.md) | shared CPU/temporary 模块，所有新必填参数更新调用者和夹具，移除同步 Drop，实际取消/错误回归；错误不伪装成功。 |
| [axum-server](/home/zyin/dev/skills/axum-server/SKILL.md) | 应用状态/生命周期拥有执行与资源，身份提取先于 Health body，流式上传、准入背压、重 CPU 隔离；F01/F02 已关闭。 |
| [sql-coding](/home/zyin/dev/skills/sql-coding/SKILL.md) | 健康聚合投影为中央命名查询，参数绑定、来源隔离、事务原子性保持；SQLite 测试和索引计划已核对。 |
| [project-structure](/home/zyin/dev/skills/project-structure/SKILL.md) / [naming-conventions](/home/zyin/dev/skills/naming-conventions/SKILL.md) | execution/temporary/health_batch/transport 源码及镜像测试，审查在 docs/reviews、日志在 build；协议/版本更新贯穿分析和文档。 |
| [docker-build](/home/zyin/.agents/skills/docker-build/SKILL.md) / [code-formatting](/home/zyin/.agents/skills/code-formatting/SKILL.md) | 双镜像关口和真实隔离 GPU/负载闭环，脚本格式检查，无发布或用户数据操作。 |
| FastAPI/OOP/pyproject、RESTful API、Swift | 沿用上轮已核对规则；本轮未改变 OCR 配置/接口/依赖或 Swift 实现。API 保持 POST action 和 GET download，Apple SDK/真机验收不声称已完成。 |

本轮可在当前环境实施验证的服务端缺口已完成。Apple SDK 构建、HealthKit 全历史/权限/Watch/断线及原生写回由用户在 Mac/iOS 验收。真实个人报告/HEIC 识别准确率仍需授权样本逐行基准；合成 GPU 成功不能代替该准确率。超过当前限额的平台序列分块、条件化参考范围、额外指标、Android App 和公众部署属于[支持矩阵](../validation/support-matrix.md)列明的后续能力，不混入本轮通过项。
