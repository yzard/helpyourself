# Web GUI

Web GUI 嵌入 `backend_api` 二进制，随 `helpyourself-backend-api` 镜像交付。保持现有 Caddy 配置，打开 `https://你的服务器/` 即可；无需前端容器或网页文件挂载。本地直接运行 API 时，也可打开其 HTTP 监听地址。

## 登录和功能

使用服务器管理员创建的同一账户。页面只连接当前服务器，不要求输入另一地址。会话保存在页面内存中；刷新或关闭页面需要重新登录。Settings 显示服务器、账户、过期时间、时区及分析开关。

- Reports：浏览已归档报告、状态、原始与标准化数值/参考范围、原件、引用页和修订历史。PNG/JPEG 直接预览，PDF 使用浏览器内置查看器，HEIC/HEIF 提供原件下载。
- Trends：查看一项或对比两项指标的全部年份。不同指标独立绘制以保留单位；图表点和列表来源可返回报告，再打开引用页。不可比较结果保留在报告内。
- Health：读取移动端已归档的数据，展示最近 30 个日历日的按来源汇总、查询覆盖、原始结构化明细。日界线采用浏览器时区；缺失值不会显示成零。
- Insights：运行服务器现有的血脂历史与最近 90 天健康预设，浏览状态、源数据快照、研究假设、证据和就医问题；失败时可重试。`[providers.analysis].enabled = false` 时按钮禁用，历史结果仍可浏览。配置与服务密钥归服务器。

上传、复核修改、读取 Apple Health/Health Connect、同步到后端或写入平台属于移动端能力。Android 客户端尚未实现。

浏览器请求不经过官方服务器、CDN 或第三方网页服务。AI 请求由 API 使用管理员配置的服务处理。浏览器页面范围不构成新的只读账户权限。

## 构建和验证

`./build_docker.sh` 需要 Docker、Python 及 Node.js 22+。网页为原生 ES modules，无 npm 运行依赖或打包步骤。脚本执行网页客户端测试与语法检查，Docker builder 执行 Rust 格式、Clippy、全部测试和 release 构建。

```bash
node --test tests/frontend/*.mjs
./build_docker.sh
python3 tests/docker/deployment.py
```

浏览器回归测试使用 Playwright 和已有 Chromium。可将工具安装到忽略的构建目录，然后运行独立合成部署测试；测试不会使用现有 playground 数据或用户账户。

```bash
npm install --prefix build/webgui/tooling --no-save --package-lock=false playwright@1.58.2
node build/webgui/tooling/node_modules/playwright/cli.js install chromium
python3 tests/docker/deployment.py --webgui build/webgui/tooling/node_modules/playwright/index.mjs
```

浏览器测试的报告、原件、认证和健康读取通过真实独立 API/Caddy。AI 排队、成功、失败和失效显示使用合成 HTTP 回复；真实后端分析的快照、模型调用与引用校验由 Rust 测试覆盖。截图保存到 `build/webgui/screenshots/`，部署证据保存到 `build/deployment-check/result.json`。
