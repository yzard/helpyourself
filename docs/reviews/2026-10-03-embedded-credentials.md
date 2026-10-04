# 后台服务凭据内嵌 TOML

后续：本日进一步移除 OCR enabled 开关，URL 与内嵌密钥始终必填，上传直接排队，见[必要 OCR 服务审查](2026-10-03-required-ocr.md)；下文保留凭据调整当时的记录。

日期：2026-10-03。用户要求所有后台服务配置中的 keys、passwords、tokens 和 secrets 直接内嵌到 config.toml，同时将约束写入 skills。本轮统一 helpyourself API、OCR 和分析适配器的配置，保留现有 `--data-dir ABSOLUTE_DIRECTORY`、各服务根目录与 SQLite 业务数据，不做 schema migration。

## 配置与运行契约

| 服务/用途 | 当前字段 | 行为 |
| --- | --- | --- |
| backend_api → OCR | `[ocr].api_key` | 启用 OCR 时必须有 24–8192 字符的有效凭据；不读取文件。 |
| backend_ocr 请求鉴权 | `[server].api_key` | 必填 24–8192 字符，与 API OCR 凭据一致；配置对象使用 SecretStr 隐藏表示。 |
| backend_api → 分析模型 | `[providers.analysis].api_key` | 非空时直接用于 Bearer header；空串表示该本地兼容接口无需鉴权。 |

密钥验证拒绝空白、换行、非可见 ASCII 和超限值，不能产生 header 注入。非法 TOML/字段/凭据错误不输出原值。两个运行时都拒绝旧 api_key_file 字段，不保留文件回退或秘密环境/CLI 覆盖；模型等非凭据路径继续按现有根目录解析。配置变更重启生效。

源模板只有空占位；生成后的私有服务 config.toml 使用 0600 且由现有 playground 忽略规则排除，不进入 Git、Docker 构建输入、手机/Web 响应或业务导出。API init-config 同样使用 0600 且不覆盖已有配置。用户账户密码哈希、已签发会话以及任务租约仍属于 SQLite 业务数据；没有将这些变成明文配置密码。Caddy 管理的证书是 TLS 运行产物，不是应用配置凭据。

## 初始化与一次性升级

src/development/data_roots.py 复用原根目录准备职责。新安装一次生成随机 OCR key，并分别嵌入两份服务 TOML；再次运行保持密钥和其他设置。

旧配置/文件的一次性转换在停服和 API 根独占锁下执行。先读取旧值、校验所有计划配置和 OCR key 一致性，再用私有暂存文件、fsync 和原子 replace 持久化。确认两份 TOML 成功后才删除项目内旧文件。配置写入中断时旧文件保留，重新运行不会轮换凭据；冲突、错误配置或活跃数据锁会停止而不覆盖。引用到项目外的旧凭据会嵌入配置，脚本不删除其他项目可能使用的外部文件。

本地 playground/backend_api/config.toml 与 playground/backend_ocr/config.toml 已实际转换，保持原 OCR key 一致、原有分析配置及其余设置，权限为 0600；两个原 ocr-key 已删除，实际 API check-config 和 OCR typed loader 检查通过。检查了原有数据文件 inode、大小与修改时间未变，没有操作真实健康内容。转换前本项目 playground 容器未运行；没有自动部署或重启用户服务。

## Skills 持久约束

两个技能根 `/home/zyin/dev/skills` 和 `/home/zyin/.agents/skills` 同步更新：

- [general-coding](/home/zyin/dev/skills/general-coding/SKILL.md#background-service-credentials)：统一后台服务 TOML 内嵌凭据、无文件/环境/CLI 密钥来源、保护模板与私有配置、保留旧值的原子升级。
- [axum-server](/home/zyin/dev/skills/axum-server/SKILL.md)、[fastapi-server](/home/zyin/.agents/skills/fastapi-server/SKILL.md)、[docker-build](/home/zyin/dev/skills/docker-build/SKILL.md)：直接引用共用约束，框架和容器不能另建凭据来源。
- Axum auth-config 与 Receipt Master config 参考同步清除密钥环境覆盖说明；非凭据 URL 覆盖仍可沿用其项目契约。r0 FastAPI 的旧 YAML 示例明确不得用于后台服务配置。

按 [skill-creator](/home/zyin/dev/skills/.system/skill-creator/SKILL.md)修改既有技能，八个 SKILL.md 均通过 quick_validate。保留这些技能目录中原有已暂存/未暂存的其他改动，没有 Git 提交或重置。

## 验证

- Rust 74 项测试、fmt、严格 Clippy 通过：内嵌 OCR/分析 header 鉴权，无密钥文件启动、旧字段拒绝、错误脱敏、私有配置权限及既有业务回归。
- OCR Python 10 项测试通过；开发脚本 13 项、前端 11 项测试由构建入口通过。Black/isort 检查通过。
- 根目录 build_docker.sh 成功构建 API 与 OCR 本地镜像，全部相应检查关口通过。
- 配置准备回归涵盖旧目录/数据库 sidecar/原件保留，所有密钥内嵌及转义，冲突/活跃锁拒绝，中断恢复不轮换和已有配置保留。

真实 GPU 部署验证通过：没有任何独立 key 文件的隔离服务根目录，API 用 TOML 内嵌密钥调用 Qwen3.8/NInfer；合成图片、文字 PDF、扫描/混合 PDF 识别，多单位换算、可信测试 CA HTTPS、非 root UID/GID、归档/复核/趋势/导出、重启、停服完整根复制恢复和删除均通过。Health 32/40 MiB 并发、原始载荷重放、超限原子拒绝也通过。网页静态资源检查及 11 项前端单元测试通过；本轮未请求 browser 验收，因此未运行 --webgui。临时测试容器和目录清理完成，不启动用户 playground。

日志：build/backend_api/2026-10-03-embedded-credentials-{tests,clippy}.log，build/backend_ocr/2026-10-03-embedded-credentials-{tests,bootstrap,build,deployment}.log。运行方式见[后端运维](../runbooks/backend.md)和[OCR 运维](../runbooks/ocr.md)。本轮不改变模型、iOS 或已记录的准确性验收边界。
