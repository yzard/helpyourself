# 数据根目录约定与验证

用户指出原有 `[server].data_dir` 不符合希望采用的 skill 约定。本次按其要求将两个服务统一为显式绝对 `--data-dir` 和固定 `<data-dir>/config.toml`。所参考的规则来自 docker-build 的 `references/receipt-master-config.md`；仅采用数据根、初始化保留和进程身份规则，不引入其收据领域配置、JWT 或 APK 契约。

## 完成的调整

- Rust API 与 Python OCR 均移除配置文件路径参数，只接受绝对数据根；相对根、缺失配置、无效 TOML 和 TOML 中的 data_dir 均拒绝。密钥和模型的相对路径从根解析。
- API 的 SQLite、原件、派生文件、临时文件、导出、config.toml 与 ocr-key 位于同一根。OCR 的独立根仅含自己的配置与密钥，模型仍在镜像 `/models`。
- Compose 将 playground/backend_api 读写挂载到 API `/data`，将 playground/backend_ocr 只读挂载到 OCR `/data`；不再使用共享 data、独立 `/config` 或 `/secrets` 挂载。启动脚本导出调用用户 PUID/PGID。
- Bootstrap 仅创建缺失的配置和稳定密钥，创建配置/密钥权限为 0600。已有设置保留，两个根的 OCR 密钥需一致。
- 旧 Playground 布局按停服、独占数据锁、冲突检测迁移。SQLite、sidecar、原件及密钥值保留；自定义旧数据路径要求先明确搬迁。此操作不改变数据库 schema。
- CLI、配置模板、镜像启动参数、管理命令、备份恢复说明与测试调用均同步更新。

## 已执行验证

`./build_docker.sh` 成功：Rust fmt、严格 Clippy、73 项 Rust 测试及 release 构建；OCR isort、Black、10 项测试；10 项开发脚本测试；11 项前端测试及模块语法检查。两个 Docker 镜像完成构建。后续启动脚本改动再次通过开发/启动脚本测试、Shell 语法与 diff 空白检查。

配置回归覆盖绝对目录、固定文件名、旧 TOML 字段拒绝、相对密钥路径。布局回归覆盖旧数据库/sidecar/原件字节保留、配置修改保留、稳定密钥、冲突失败及活跃锁阻止搬迁。

`tests/docker/deployment.py --ocr --webgui build/webgui/tooling/node_modules/playwright/index.mjs` 通过：

- 检查解析后的两个独立 `/data` 挂载及读写标志；读取主进程 `/proc/1/status` 确认实际 UID/GID；检查 API 数据库与原件文件归属；OCR 根不存在 API 数据库。
- 非 root OCR 真实冷启动 Qwen3.8/NInfer，完成合成图片、文字 PDF、扫描/混合 PDF 识别及单位/参考范围转换，验证惰性加载、空闲卸载和 OCR 停止后 API 仍可用。
- 通过受信测试 CA 验证 HTTPS、登录、上传、复核、趋势、原件和导出；桌面/手机尺寸浏览器验证报告、健康数据、原件、认证、键盘与 XSS。AI 网页状态使用响应 fixture。
- 重启后档案保持；停服复制整个 API 根并仅挂载复制目录即可恢复账户及档案；报告/账户删除完成，测试独立资源清理。

集成结果位于忽略目录 `build/deployment-check/result.json`，本次日志为 `build/data-roots-docker.log` 和 `build/data-roots-deployment.log`。未重跑 `--load` 性能测量；已有生命周期/负载审查仍说明其测量边界。本次使用合成输入，不代表真实临床报告准确性或 iPhone 真机验收。
