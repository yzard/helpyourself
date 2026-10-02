# 独立 OCR 服务与 skills 复查

后续：F01/F02 已于 2026-10-02 整改并通过负载与生命周期验证，当前结论见[服务端补齐验收](2026-10-02-server-lifecycle-and-load.md)；下文保留发现当日的证据与状态。

历史记录：文档输入与单位差距已按 [补齐计划](../implementation-document-units.md)实施，当前结果及仍未解决的 skills 事项见 [补齐验收](2026-10-01-document-unit-completion.md)；以下保留发现时的证据。

日期：2026-10-01。范围为整个 helpyourself 仓库；当前变更是用户明确指定的 backend_api / backend_ocr 拆分及 Qwen3.8 + NInfer 推理，不审查或修改其他项目。

结论：两个独立服务和真实 GPU 合成数据闭环已实现。主档案、来源 raw 目录和手机 API 契约保留；旧通用视觉 OCR 与 document_parser 调用/config 已移除。全部适用 skills 尚不能宣称完全符合：先前发现的 API 阻塞工作隔离 F01/F02 仍存在，iOS 原生 SDK/设备证据仍缺。

## 服务与数据边界

- `src/backend_api` 是唯一 Rust API 源树，Cargo 清单/锁文件/.cargo 随组件；对应 `tests/backend_api`。API 负责手机用户会话、SQLite、原件、持久任务、复核、计算、导出和删除。
- `src/backend_ocr` 是实际可运行的 Python FastAPI 服务，组件 pyproject.toml/uv.lock 及镜像 `tests/backend_ocr`。工厂/lifespan 持有 HTTP client、队列和 NInfer 进程。无数据库或业务 data 挂载。
- 手机上传照片/PDF到 API；API 原件路径仍为 `data/raw/photos` / `data/raw/documents`，HealthKit 为 `data/raw/apple_health`，未来 Health Connect 为 `data/raw/google_health`。OCR 只处理逐页内联图像，推理副本的旋转/缩小不修改原件。
- API 调用 POST `/api/v1/documents/extract`，只提交 page/image_url。独立只读服务密钥与手机用户凭据分离。模型、提示和生成配置归 OCR，手机无覆盖接口。
- Qwen3.8 27B NVFP4 模型和 NInfer 固定 revision/commit、SHA256；模型构建进镜像，原生引擎监听容器 loopback。版本、硬件及协议见 [OCR 运维](../runbooks/ocr.md)。无运行时隐式下载/更换模型。
- 结构化候选保持 pending/metric_id=null；完整服务 envelope、NInfer 正文/reasoning/usage、提示版本、HTTP 状态先进入 extraction_outputs，再做 API 校验。422 结构失败同样保留完整收到的回复；网络未收到或超限的正文不伪造完整。
- 惰性加载、单推理、最多 4 个 pending 请求、600 秒包含排队/图像处理/冷加载/推理的截止时间，默认 300 秒空闲卸载。取消/超时停止并等待进程组，健康检查不加载模型或延长空闲时钟。
- Compose 仅 Caddy 对外 HTTPS；API 和 OCR 共享 internal inference 网络，OCR 无 host port。两镜像独立 Dockerfile/ignore，统一构建入口，前台 playground 初始化独立密钥且不覆盖配置。仅运行隔离合成测试，没有启动用户 playground 或发布镜像。

## 适用 skills

复核了这次变更的调用者、配置、构建入口、文档和测试；未变更的 iOS/健康数据规则沿用 [原始档案审查](2026-10-01-raw-archive-and-skills-audit.md) 的证据及未决发现。同名 skill 的项目专属 Receipt Master 条款不应用于 helpyourself，用户的显式 TOML、当前 schema、不做 migration、Caddy HTTPS 和持久会话契约优先。

| Skill | 证据与结论 |
| --- | --- |
| [project-structure](/home/zyin/dev/skills/project-structure/SKILL.md) | 两个真实组件、镜像 tests、组件清单/锁文件、Dockerfile-specific ignore、build/backend_api 输出；旧 src/backend 与 tests/backend 已移除，命令/文档同步。符合本轮目录迁移要求。 |
| [add-modify-codebase](/home/zyin/dev/skills/add-modify-codebase/SKILL.md) | 配置、worker、CLI probe、upload/retry 能力判断、模拟服务、取消/晚到及失败归档调用者全部更新；原生 GPU 闭环验证可观察行为。符合本轮契约变更要求。 |
| [general-coding](/home/zyin/.agents/skills/general-coding/SKILL.md) | 显式配置与依赖、不增加业务环境读取、错误边界与取消传播、执行槽/子进程归属、无模型失败后的默认成功。已接收失败回复可追溯。符合所查新契约，API 既有异步阻塞缺口见下。 |
| [axum-server](/home/zyin/.agents/skills/axum-server/SKILL.md) | 保留工厂/AppState/统一错误、会话身份、命名 SQL、持久任务及租约；OCR 新接口独立 client/key。部分符合；F01/F02 尚未修复。 |
| [fastapi-server](/home/zyin/.agents/skills/fastapi-server/SKILL.md) | app factory、typed TOML/Pydantic、app.state/lifespan、typed 请求/响应、统一脱敏错误、ASGI 入站鉴权/大小限制、方法/路由/状态/耗时日志；CPU 图像/输出校验用 asyncio.to_thread，运行时资源可靠关闭。8 项测试通过。 |
| [oop-code](/home/zyin/.agents/skills/oop-code/SKILL.md) | EngineRuntime 明确持有注入的 config/client，状态和生命周期集中；纯预处理/解析保持函数，无类层次或第三方全局连接。取消、进程回收、队列和空闲行为有实际子进程测试。 |
| [pyproject-toml](/home/zyin/.agents/skills/pyproject-toml/SKILL.md) | OCR 组件配置 Python 3.12、直接依赖及 dev tools，uv.lock 固定传递版本；源码作为应用复制到镜像（uv package=false），不假装构建发布 wheel。 |
| [code-formatting](/home/zyin/.agents/skills/code-formatting/SKILL.md) | OCR 组件 Black 120/skip-string-normalization/skip-magic-trailing-comma、isort Black profile；构建关口实际检查。 |
| [restful-api-design](/home/zyin/.agents/skills/restful-api-design/SKILL.md) | 内部 POST action extract，GET health 为协议健康检查；手机 action/GET stream 既有方法保持，内部服务 key 不代替用户身份。 |
| [docker-build](/home/zyin/.agents/skills/docker-build/SKILL.md) | 固定 native/model 下载 SHA、源构建 CUDA 依赖、runtime 真实 GPU 运行、双镜像检查、降权、只读密钥、内部网络和 foreground 入口；原件及凭据不进入镜像，未发布。 |
| [sql-coding](/home/zyin/.agents/skills/sql-coding/SKILL.md) | current schema v6、绑定/命名查询、归属外键/事务和晚到结果验证保持；当前 stage 只为 ocr。用户明确无需迁移，无兼容迁移或清库操作。 |
| [naming-conventions](/home/zyin/.agents/skills/naming-conventions/SKILL.md) | backend_api/backend_ocr 在目录、测试、Docker/Compose/命令/文档中对齐；健康来源协议与原件路径映射保持一致，无平行旧组件。 |
| [write-swift](/home/zyin/dev/skills/write-swift/SKILL.md) | 手机仍调用相同 backend_api 协议；本轮未改 Swift。此前 Core 5 项及 Linux 语法解析证据有效，不能取代 Apple SDK 编译/HealthKit 真机证据。 |

未适用技能：动画/视觉/品牌/图像生成、媒体播放器、Office/PDF/表格交付、Pages/Sites/wiki/宠物、Codex/OpenAI 产品、技能安装创建、plugin 管理、grilling 等没有本轮对应工作。NInfer 源码作为固定第三方构建输入，未改 C++，不机械套用 cpp-coding；没有针对现有 Python 业务类的行为保持重构，因此 refactoring 不单独触发。没有委派代理或操作其他仓库。

## 实际验证与限制

| 检查 | 结果 |
| --- | --- |
| Rust API | 50 项测试、fmt 和严格 Clippy 通过，含独立 OCR 请求、完整成功/失败回复归档、跨用户/晚到删除、PDF逐页提取、复核和导出。 |
| Python OCR | 8 项测试通过：内联图片与模型配置所有权、失败完整回复/截断/猜指标/错页、鉴权/请求限额/脱敏、配置、真实子进程取消/超时/回收、空闲卸载及队列上限。 |
| 开发脚本 | 8 项网络/配置密钥/前台入口/iOS 构建失败流程检查通过。已有密钥/用户配置不覆盖。 |
| 双 Docker 镜像 | `helpyourself-backend-api:local` 与 `helpyourself-backend-ocr:local` 构建通过，镜像构建内部执行对应检查。 |
| RTX 5090 / 真实模型 | 隔离合成 1400×600 PNG 的 LDL 行识别为 120 mg/dL；真实 NInfer finish_reason=stop、完整回复归档、候选 pending、无自动 metric 映射。 |
| 完整部署 | 可信合成 CA HTTPS、原件逐字节下载/ZIP、人工确认与趋势、API 重启、停服 data 复制恢复、报告/账户删除通过。OCR 启动未加载，测试空闲后卸载，OCR 停止后 API 仍响应。临时容器/卷已清理。 |
| iOS / 真实报告 | 本轮未改 Swift，也未运行 Xcode/iPhone/Watch 或用户真实报告；模型语义/漏项准确率仍待真实样本验收。 |

运行日志在 `build/backend_ocr/2026-10-01-build.log`、`2026-10-01-tests.log`、`2026-10-01-gpu-deployment.log` 和 `build/deployment-check/result.json`，不提交运行产物。此前 Core 与健康同步测试见原始档案审查。本轮没有提交 Git 或改动用户实际数据。

仍未解决：F01 是 API files/worker/maintenance 的同步 Drop 文件清理；F02 是 Health payload 编解码/摘要等 CPU 工作仍在异步调用域（本轮 API 大 OCR envelope 解析同样需要将来统一有界 CPU 隔离）。这些是实际技能缺口。iOS SDK/设备、极大健康序列、真实 HEIC 和真实报告准确性是外部证据/能力限制，不能用合成 GPU 成功代替。

## 后续输入与单位核实

[PDF、图片与单位处理审查](2026-10-01-document-input-and-units.md)补充核实：文字层路径尚未实现、单位转换仅覆盖四项血脂及标准单位、参考范围未统一换算，另发现非 null bounding_box 的 Python/Rust 类型不一致。上述待补齐，不能将原 GPU 合成闭环视作这些能力已完成。
