# 后端启动与运维

本版本提供报告原件归档、模型提取、人工复核、趋势、健康原始数据同步、完整导出与删除，以及默认关闭的个人血脂分析。具体字段见 [API 契约](../api.md)，验证边界见 [最新审查](../reviews/2026-10-02-server-lifecycle-and-load.md)。

## Playground 启动

从仓库根目录执行 `./run_playground.sh`，需要 Docker/Compose、Python 3、NVIDIA Container Toolkit 和 RTX 5090。OCR 镜像包含约 23.7 GB 的固定模型文件；首次构建需要对应下载和磁盘空间。脚本不接收参数，每次先运行构建检查和镜像构建，再以前台 Compose 启动；持续显示日志，Ctrl+C 停止容器并保留数据。构建失败不会启动或替换服务。

Playground 使用独立 Compose 项目 `helpyourself-playground`，加载 `playground/config.toml`，将 `playground/data/` 挂载到容器 `/data`。该配置的监听地址是容器内的 `0.0.0.0:8080`；API 仍经 Caddy HTTPS 对外，不额外暴露明文后端端口。脚本从解析后的 Compose 配置读取端口并显示地址；LAN IP 不等于 TLS 可用域名。

启动脚本仅在缺失时创建 `playground/secrets/ocr-key`（0600）和 `playground/backend_ocr.toml`，已有密钥和配置保留。两个服务只读共享该内部密钥文件。首次启动后，在另一终端建号：

```bash
HELPYOURSELF_CONFIG="$PWD/playground/config.toml" HELPYOURSELF_DATA_DIR="$PWD/playground/data" \
  HELPYOURSELF_OCR_CONFIG="$PWD/playground/backend_ocr.toml" \
  docker compose -p helpyourself-playground -f docker/docker-compose.yaml exec --user 1000:1000 backend_api \
  /app/helpyourself --config /config/config.toml create-user --username yourname
```

建号时隐藏输入密码，长度 12–1024 字节。自动化可用 `--password-stdin` 从安全输入通道传入，不要把密码写进命令行参数。没有默认账户或密码。Playground 直接初始化当前 SQLite schema；脚本不会清空宿主机 data 目录，也不升级旧 schema。

需要宿主机调试时，另外生成自己的 TOML，按其位置配置 data_dir，不要直接使用写着 `/data` 的容器配置：

```bash
cd src/backend_api
cargo run --locked -- init-config /absolute/path/config.toml
cargo run --locked -- --config /absolute/path/config.toml check-config
cargo run --locked -- --config /absolute/path/config.toml serve
```

原配置文件存在时不会覆盖。相对路径按 TOML 所在目录解析；修改后重启。后端 Cargo.toml、Cargo.lock 和 .cargo/config.toml 均位于 src/backend_api/，直接使用 Cargo 时从该目录执行；根目录不建立 Rust workspace。Rust 中间产物位于 `build/backend_api/target/`，镜像构建将最终二进制暂存于构建阶段的 `dist/backend_api/`。iOS 产物规则见其构建指南。

## 模型服务

backend_api 接收原件并持久化任务；backend_ocr 是独立 FastAPI 服务，管理 Qwen3.8 27B NVFP4 + NInfer 推理。PDF 由 API 渲染为逐页 PNG，HEIC 使用已归档的 JPEG 处理副本；原件均留在 API。服务通信、配置和运行限额见 [OCR 运维](ocr.md)。

- API `[ocr]` 配置 `enabled`、`url`、`api_key_file` 和 `timeout_seconds`。容器示例启用并连接 `http://backend_ocr:8000`，独立宿主机模板默认关闭。
- OCR 服务只接受内联 PNG/JPEG，不接受远程 URL、文件路径、任意 messages 或客户端模型配置。当前没有 document_parser 第二阶段。
- `providers.analysis` 使用独立 OpenAI 兼容 `/v1/chat/completions` 接口，个人血脂分析默认关闭。其模型/密钥/extra_body 仍由管理员明确配置。
- OCR 请求不进行 HTTP 层重复调用；重试由 API 持久任务与租约负责。已确认修订不会被重跑覆盖。分析接口的临时错误最多三次调用，间隔 1、2 秒。

在 `src/backend_api/` 执行 `cargo run --locked -- --config /absolute/path/config.toml probe-providers`：OCR 只检查服务存活，不加载模型；分析发送合成文字。该命令不证明视觉识别正确。OCR 关闭时上传与人工复核仍可用，提取任务显示 `blocked / provider_disabled`，启用后重新排队。

## 账户与隔离

```bash
cd src/backend_api
cargo run --locked -- --config /absolute/path/config.toml reset-password --username yourname
cargo run --locked -- --config /absolute/path/config.toml disable-user --username yourname
```

用户名规范化为小写；重置密码和禁用撤销全部旧会话，进行中的旧凭据登录也不能重新建立有效会话。会话默认一天。数据库只保存 token 摘要。禁用保留档案；删除账户由已认证用户显式确认用户名完成。

每个受保护请求由会话确定 user_id，不能由请求覆盖。跨用户资源按 404 处理。日志仅记录服务器生成的请求 ID、方法、路由模板、状态和耗时，不记录原文、文件名、模型响应、密码或 token。响应 `Cache-Control: no-store`。

## 独立部署 Docker / Caddy

```bash
./build_docker.sh
python3 src/development/ocr_key.py --key playground/secrets/ocr-key --config playground/backend_ocr.toml --template src/backend_ocr/config.toml
docker compose -f docker/docker-compose.yaml up -d
docker compose -f docker/docker-compose.yaml exec --user 1000:1000 backend_api \
  /app/helpyourself --config /config/config.toml create-user --username yourname
```

此处直接启动的是独立部署配置，与 run_playground.sh 的项目/挂载分开。Compose 内部后端 HTTP，Caddy 对外 HTTPS。默认 OCR 模板可直接使用；修改模板时通过 HELPYOURSELF_OCR_CONFIG 指定独立文件。默认 localhost 使用本地 CA；真机需使用受信任的证书和可访问地址。配置 `HELPYOURSELF_DOMAIN` 为自己的域名并安排解析和入口端口。没有替用户发布公共服务。

`docker/config.toml` 只读挂载；模型 URL 在容器内解析，127.0.0.1 指容器自身，需改为真实模型服务地址或同网络服务名。PUID/PGID 默认 1000，入口验证 UID/GID/UMASK 并对专用 data 卷降权；管理员 exec 使用对应 UID。不要将无关目录挂载为 `/data`。

正式停止用 `docker compose -f docker/docker-compose.yaml stop`，不要 `down -v` 删除持久卷。测试脚本仅为自己创建的独立测试项目使用 `down -v`。

## 数据、复制与恢复

SQLite schema v7，启用外键、WAL 和 FULL synchronous。按用户“playground 为空、无需 migration”的要求，当前只支持空库初始化和 v7 重启；旧版本或其他未知版本明确拒绝启动，不自动修改或删除旧库。历史 migration SQL 已移除。同一 data 只允许一个服务进程；管理员命令可独立运行。

停服并停止管理员写入后，复制整个 data（包含仍存在的 SQLite sidecar）；另存 TOML 与密钥。恢复到独立目录/卷，用相同版本启动核对账户、原件、修订与导出。当前没有跨 schema 升级、在线备份或自动备份机制。

报告/账户删除先在事务内使内容不可访问并撤销有关任务或会话，再由持久清理队列删除文件。旧上传 ID 和健康删除事件保留最小抑制标记，避免迟到上传复活。用户自己导出的文件、手机分享目的地和独立备份不在服务器删除范围内。

导出 v3 在一致数据库快照中生成 JSONL、CSV、原件和版本清单，随后再次核对数据版本；数据变化使旧导出不可下载。失败/过期导出可删除后重新生成。CSV 防公式执行，JSONL 保留原字符串；包含健康有效修订原件、OCR/解析完整回复和人工修订。

原件实际路径是 `raw/photos/<user_id>/<file_id>`、`raw/documents/<user_id>/<file_id>`、`raw/apple_health/<user_id>/<revision_id>.json`，以及预留的 `raw/google_health/<user_id>/<revision_id>.json`。上传原件无扩展名，文件名和类型保留在 SQLite；health_connect 映射到 google_health 目录。HEIC 原件保留，derived 下 JPEG 仅供 OCR。ZIP 按这些索引路径收录原件。

运行期间临时文件由既有维护周期异步清理；HTTP 取消不会提前删除阻塞任务仍在使用的文件。正常停服等待 worker 和已准入 CPU 闭包，再清理并关闭 SQLite；启动在独占 data 锁下恢复 tmp 和没有数据库记录的原件。不要手工向 raw 添加健康报告。PDF 内容解析仍在受大小约束的进程内，公众版需要额外资源隔离验证。

## 验证

```bash
cd src/backend_api
cargo fmt --all --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cd ../..
docker compose -f docker/docker-compose.yaml config --quiet
src/backend_ocr/.venv/bin/python tests/docker/deployment.py --ocr --load
```

最后一项使用 OCR 组件虚拟环境（见 OCR 运维），启动独立合成测试容器，验证实际 GPU 图片/文字 PDF/扫描与混合 PDF 多单位识别、惰性加载与空闲卸载、HTTPS、归档、复核、趋势、导出、重启和删除；--load 追加并发 32 MiB 单 payload /40 MiB 批次、超限原子拒绝和完整重放，记录 API RSS 与状态延迟，再清理自己的临时卷。省略 --ocr 可只运行 API 部署闭环；两种方式均启动 OCR 容器但只有 --ocr 加载模型。

CPU 或 Health 入口槽满时返回 429，保留相同批次 ID 和内容重试；413 必须缩小请求，不能重试被截断的原始载荷。性能测量写入 build/deployment-check/load-result.json；它是当前机器的短时合成测试结果，不是生产 SLA。当前验证记录见[服务端验收](../reviews/2026-10-02-server-lifecycle-and-load.md)。
