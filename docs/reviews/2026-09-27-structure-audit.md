# 仓库结构与技能审查

历史记录：服务目录、OCR 架构和当前验证证据已由 [独立 OCR 服务审查](2026-10-01-backend-ocr-service.md) 更新；以下保留当时的实现及结果。

历史记录：保留当时的实现和测试事实；当前数据库策略、原件布局、HealthKit 范围及验证结果以 [2026-10-01 审查](2026-10-01-raw-archive-and-skills-audit.md) 为准。

日期：2026-09-27。范围：project-structure、docker-build、add-modify-codebase、general-coding、axum-server、sql-coding、restful-api-design、naming-conventions、write-swift，以及变更的 Python 文件格式。依据是本次实际读取的技能版本；不是沿用 9 月 6 日的旧目录要求。

## 发现与逐项修复

| 编号 | 原问题 | 处理结果 |
| --- | --- | --- |
| S01 | 原生 iPhone 代码在 src/frontend/ios，测试同样嵌套 | 移到 src/ios 与 tests/ios；更新 Package.swift、XcodeGen 相对测试路径和全部文档。旧工作目录已移除，不留源码副本或兼容转发。 |
| S02 | 缺少根级 iOS 构建入口，命令只散落在说明中 | 新增 build_ios.sh：Core 测试、XcodeGen、模拟器测试编译、可选模拟器执行、unsigned 设备构建。build/ios 存中间文件，dist/ios 存 App。Linux 默认拒绝完整构建，--core-only 只验证可移植代码。 |
| S03 | run_playground.sh 直接运行宿主机程序，忽略多余参数，没有前台 Compose 构建流程 | 改成无参数入口，每次先 build_docker.sh，失败即停止，再 exec Compose up --no-build；使用独立 playground 项目和持久 data 挂载。显示从 Compose 解析的 HTTPS 端口及 LAN 地址/失败提示。 |
| S04 | Compose 路径与当前技能的 canonical 名称不一致 | 统一 docker/docker-compose.yaml；同步部署脚本、构建调用和文档。默认独立部署的命名卷保留；playground 显式选择自己的配置/目录。 |
| S05 | schema.sql 在 CREATE 后又追加历史 ALTER 和回填 SQL；v1 测试还从当前 schema 截取旧结构 | schema.sql 直接定义当前 v5，历史升级 SQL 原样保留；建立固定 v1 测试夹具，验证所有受支持版本升级后的列、外键、索引与新库一致。无需提高 schema 版本，因为最终结构没有变化。 |
| S06 | Docker 源码变更导致依赖重复编译，构建没有检查关口 | 使用 BuildKit registry/git/target 缓存，并在缓存外 dist/backend 暂存成品；构建中强制 fmt、Clippy、47 项测试及 release 编译。COPY/ignore 更新为包含测试和合成夹具；Poppler 也安装在测试构建阶段。 |

## 关联修复

- 将网络显示辅助实现放在 src/development/playground.py，对应测试在 tests/development/playground.py；根脚本流程测试对应 tests/run_playground.py、tests/build_ios.py。测试不启动正式服务。
- 新增 6 项脚本/地址测试：构建失败不启动、前台参数、无效参数、Linux 构建边界、来源过滤、Compose 端口及明确降级提示。
- 测试部署显式覆盖配置、数据卷及 loopback 绑定，防止继承用户环境后误用正式目录。
- Docker 的入口缓存不包含运行数据或密钥；最终镜像只复制最终二进制与入口。运行用户和目录权限机制保持既有实现。
- Python 改动按 isort/Black 统一格式；没有增加新的业务环境变量配置通道。

## 核对后保留的仓库契约

- src/backend 是一个实际 Rust 服务。OCR/解析和分析是它连接的外部 provider，不是仓库内独立推理后端；因此不创建空 backend_ocr，也不机械改成 Receipt Master 的 backend_api 布局。
- 没有已实现 Android App，所以不创建 build_android.sh 或空 Android 项目，不在 Docker build 中声称生成 APK。iOS 构建与 Docker 分开。
- Bearer token 对应持久化会话与摘要，属于已建立的认证契约。Axum 技能要求保留仓库鉴权，不因其标题出现 JWT 而改协议。
- TOML、类型化状态、应用工厂、命名查询、事务归属、POST action 路由与 GET 下载符合现有约定。单个集中 queries.rs 不违反“SQL 集中管理”；没有按行数强拆模块。
- 用户明确要求 Caddy 承担 TLS：公开入口是 Caddy HTTPS，后端 8080 仅在容器网络中。不会为套用一般 playground 示例而增加公网明文 API。
- Swift 当前使用显式 MainActor 和严格并发检查。本次迁移不同时更改并发语义；Core 属于通用库，不启用全局 MainActor。完整 Apple SDK 类型检查仍需 macOS。
- README、整项目的 Package 清单和根构建入口是根目录例外。按用户后续明确规则，Rust 组件的 Cargo.toml/Cargo.lock/.cargo 配置归属 src/backend；未来独立共享 Rust 库归属 src/shared，不创建根 Rust workspace。跨模块 lifecycle 和部署测试不需要捏造对应业务模块。

## 验证记录

- 47 项 Rust 测试通过，包括新增的 v1–v5 结构等价测试。
- 严格 Clippy、Rust 格式检查通过。
- build_ios.sh --core-only：全部 Swift 语法解析及 3 项 Swift Testing 通过。
- 6 项 Python 流程/辅助测试通过；isort/Black 和 shell 语法检查通过。
- build_docker.sh 完整构建通过：镜像内 fmt、严格 Clippy、47 项测试、release 编译全部成功；BuildKit 依赖缓存已使用。
- 新镜像通过隔离 Compose 测试：可信本地 CA 的 HTTPS、登录上传复核趋势、原件/ZIP、重启保留、停服复制恢复、报告和账户删除。测试项目容器/卷已清理，未触及正式 data。
- macOS/Xcode、签名、模拟器执行及真机验证未执行，不能把脚本存在或 Linux Core 检查算作 iOS 编译通过。

仓库原有的暂存内容没有提交、重置或重新暂存；本次文件迁移保留在工作树，提交前需要将新路径和旧路径删除一起暂存。

## Cargo 组件归属修正

按用户明确要求，后端 Cargo.toml/Cargo.lock/.cargo 配置移至 src/backend，lib/bin/test 路径、Docker WORKDIR/COPY 和运行文档同步调整。根目录不保留 Cargo 清单或锁文件，不创建占位共享库；后端测试继续放 tests/backend，构建产物仍在 build/backend/target。
