# 后端启动与运维

本版本提供报告原件归档、模型提取、人工复核、趋势、健康原始数据同步、完整导出与删除，以及默认关闭的个人血脂分析。具体字段见 [API 契约](../api.md)，验证边界见 [最新审查](../reviews/2026-10-01-raw-archive-and-skills-audit.md)。

## Playground 启动

从仓库根目录执行 `./run_playground.sh`，需要 Docker/Compose 和 Python 3。脚本不接收参数，每次先运行构建检查和镜像构建，再以前台 Compose 启动；持续显示日志，Ctrl+C 停止容器并保留数据。构建失败不会启动或替换服务。

Playground 使用独立 Compose 项目 `helpyourself-playground`，加载 `playground/config.toml`，将 `playground/data/` 挂载到容器 `/data`。该配置的监听地址是容器内的 `0.0.0.0:8080`；API 仍经 Caddy HTTPS 对外，不额外暴露明文后端端口。脚本从解析后的 Compose 配置读取端口并显示地址；LAN IP 不等于 TLS 可用域名。

首次启动后，在另一终端建号：

```bash
HELPYOURSELF_CONFIG="$PWD/playground/config.toml" HELPYOURSELF_DATA_DIR="$PWD/playground/data" \
  docker compose -p helpyourself-playground -f docker/docker-compose.yaml exec --user 1000:1000 backend \
  /app/helpyourself --config /config/config.toml create-user --username yourname
```

建号时隐藏输入密码，长度 12–1024 字节。自动化可用 `--password-stdin` 从安全输入通道传入，不要把密码写进命令行参数。没有默认账户或密码。Playground 直接初始化当前 SQLite schema；脚本不会清空宿主机 data 目录，也不升级旧 schema。

需要宿主机调试时，另外生成自己的 TOML，按其位置配置 data_dir，不要直接使用写着 `/data` 的容器配置：

```bash
cd src/backend
cargo run --locked -- init-config /absolute/path/config.toml
cargo run --locked -- --config /absolute/path/config.toml check-config
cargo run --locked -- --config /absolute/path/config.toml serve
```

原配置文件存在时不会覆盖。相对路径按 TOML 所在目录解析；修改后重启。后端 Cargo.toml、Cargo.lock 和 .cargo/config.toml 均位于 src/backend/，直接使用 Cargo 时从该目录执行；根目录不建立 Rust workspace。Rust 中间产物位于 `build/backend/target/`，镜像构建将最终二进制暂存于构建阶段的 `dist/backend/`。iOS 产物规则见其构建指南。

## 模型服务

三种职责在同一 Rust 进程内分模块运行：数据管理、文档处理、分析。OCR/结构化解析和分析通过独立 OpenAI 兼容接口连接自建或云端模型；没有官方服务器作为必经路径。

- `providers.ocr.adapter = "openai_chat"`：支持视觉输入及结构化 JSON 的模型直接处理逐页图像。
- `providers.ocr.adapter = "unlimited_ocr"`：OCR 返回 Markdown，随后交给启用的 `providers.document_parser` 生成候选。两步模型均由部署者选择。
- `providers.analysis`：个人血脂场景。先完成真实报告数据验收，再由服务器管理员显式启用。
- `base_url` 指向 `/v1`，服务调用 `/chat/completions`，使用非流式 messages/choices 协议。没有自动切换到其他服务。
- `api_key_file` 可选，路径按 TOML 解析；密钥从文件读取，仅用于该服务请求。Docker 下须另行只读挂载密钥文件。
- `extra_body = { temperature = 0, max_tokens = 8192 }` 可按模型需要设置；禁止覆盖 model/messages/stream。
- 网络中断、429、5xx 最多三次调用，间隔 1、2 秒；鉴权与输出结构错误不自动重试。任务租约防止过期结果提交。失败后可手动重试，已有审核不被覆盖。

在 `src/backend/` 中执行 `cargo run --locked -- --config /absolute/path/config.toml probe-providers` 只发送合成文字检查启用端点能否返回内容，不证明视觉支持或临床报告准确率。默认全部模型关闭，上传仍可人工录入和审核；任务显示 `blocked / provider_disabled`。启用 OCR 后阻塞任务自动排队。

实际 Unlimited-OCR 部署版本、视觉模板、资源需求和用户 Function Health 报告准确性尚未实测。遇到不兼容输出应调整明确配置的服务，不能把模板中的模型名当作已验证结果。

## 账户与隔离

```bash
cd src/backend
cargo run --locked -- --config /absolute/path/config.toml reset-password --username yourname
cargo run --locked -- --config /absolute/path/config.toml disable-user --username yourname
```

用户名规范化为小写；重置密码和禁用撤销全部旧会话，进行中的旧凭据登录也不能重新建立有效会话。会话默认一天。数据库只保存 token 摘要。禁用保留档案；删除账户由已认证用户显式确认用户名完成。

每个受保护请求由会话确定 user_id，不能由请求覆盖。跨用户资源按 404 处理。日志仅记录服务器生成的请求 ID、方法、路由模板、状态和耗时，不记录原文、文件名、模型响应、密码或 token。响应 `Cache-Control: no-store`。

## 独立部署 Docker / Caddy

```bash
./build_docker.sh
docker compose -f docker/docker-compose.yaml up -d
docker compose -f docker/docker-compose.yaml exec --user 1000:1000 backend \
  /app/helpyourself --config /config/config.toml create-user --username yourname
```

此处直接启动的是独立部署配置，与 run_playground.sh 的项目/挂载分开。Compose 内部后端 HTTP，Caddy 对外 HTTPS。默认 localhost 使用本地 CA；真机需使用受信任的证书和可访问地址。配置 `HELPYOURSELF_DOMAIN` 为自己的域名并安排解析和入口端口。没有替用户发布公共服务。

`docker/config.toml` 只读挂载；模型 URL 在容器内解析，127.0.0.1 指容器自身，需改为真实模型服务地址或同网络服务名。PUID/PGID 默认 1000，入口验证 UID/GID/UMASK 并对专用 data 卷降权；管理员 exec 使用对应 UID。不要将无关目录挂载为 `/data`。

正式停止用 `docker compose -f docker/docker-compose.yaml stop`，不要 `down -v` 删除持久卷。测试脚本仅为自己创建的独立测试项目使用 `down -v`。

## 数据、复制与恢复

SQLite schema v6，启用外键、WAL 和 FULL synchronous。按用户“playground 为空、无需 migration”的要求，当前只支持空库初始化和 v6 重启；旧版本或其他未知版本明确拒绝启动，不自动修改或删除旧库。历史 migration SQL 已移除。同一 data 只允许一个服务进程；管理员命令可独立运行。

停服并停止管理员写入后，复制整个 data（包含仍存在的 SQLite sidecar）；另存 TOML 与密钥。恢复到独立目录/卷，用相同版本启动核对账户、原件、修订与导出。当前没有跨 schema 升级、在线备份或自动备份机制。

报告/账户删除先在事务内使内容不可访问并撤销有关任务或会话，再由持久清理队列删除文件。旧上传 ID 和健康删除事件保留最小抑制标记，避免迟到上传复活。用户自己导出的文件、手机分享目的地和独立备份不在服务器删除范围内。

导出 v2 在一致数据库快照中生成 JSONL、CSV、原件和版本清单，随后再次核对数据版本；数据变化使旧导出不可下载。失败/过期导出可删除后重新生成。CSV 防公式执行，JSONL 保留原字符串；包含健康有效修订原件、OCR/解析完整回复和人工修订。

原件实际路径是 `raw/photos/<user_id>/<file_id>`、`raw/documents/<user_id>/<file_id>`、`raw/apple_health/<user_id>/<revision_id>.json`，以及预留的 `raw/google_health/<user_id>/<revision_id>.json`。上传原件无扩展名，文件名和类型保留在 SQLite；health_connect 映射到 google_health 目录。HEIC 原件保留，derived 下 JPEG 仅供 OCR。ZIP 按这些索引路径收录原件。

启动清理 tmp 和没有数据库记录的原件；不要手工向 raw 添加健康报告。PDF 内容解析仍在受大小约束的进程内，公众版需要额外资源隔离验证。

## 验证

```bash
cd src/backend
cargo fmt --all --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cd ../..
docker compose -f docker/docker-compose.yaml config --quiet
python3 tests/docker/deployment.py
```

最后一项启动独立合成测试容器，验证真实 HTTPS、归档、复核、趋势、导出、重启和删除，再清理自己的临时卷。不会触碰正式实例。
