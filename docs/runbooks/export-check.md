# 独立检查导出档案

该工具使用 Python 3 标准库直接读取 helpyourself v1 导出 ZIP，不需要服务、数据库、AI 或第三方依赖。它不解压、不修改档案，不把健康记录输出到终端。

从仓库根目录运行，显式指定可接受的总解压后字节数。下面示例限制为 1 GiB：

```bash
python3 src/archive/check_export.py /absolute/path/export.zip --max-bytes 1073741824
```

成功时返回 JSON 摘要和退出码 0。摘要包含各表记录数、原件数、已校验摘要数及能力限制。失败时向标准错误输出简短 JSON，退出码为 1。命令行参数错误使用 argparse 的退出码 2。单个 JSON 对象或 JSONL 行上限为 96 MiB。

## 检查范围

工具要求相同的 18 张表、manifest 和 observations.csv 完整存在。检查 ZIP 重名、路径、链接成员、未声明文件及大小预算，并拒绝未知版本。

每条索引记录必须属于 manifest 用户。报告关联原件，观察记录关联报告及当前修订，健康修订关联健康记录。CSV 中的观察修订集合必须与 JSONL 相同。

报告原件按 byte_count 和 SHA-256 校验。健康原始 JSON 与索引 payload_json 按 JSON 内容比较。所有读取的 ZIP 成员同时经过 ZIP 库的 CRC 检查。摘要不包含健康数值或原始文件名。

## 限制

检查通过表示上述结构与内容一致，不证明数据真实性、医学准确性或来源服务器没有漏导出记录。v1 校验全成员摘要、字节数、表计数和列集合。清单未签名，不能发现内容和清单被一起修改后重新计算摘要的情况。

v1 对健康原件单独校验摘要。导出不包含处理副本，处理副本字段只保留历史元数据。校验工具只检查上述关联，不验证全部数据库约束、单位或医学日期含义。独立恢复工具另执行当前 schema 的完整外键与 SQLite 完整性检查。

v1 包含表计数、每文件摘要和 SQLite 列字典。`field_dictionary` 解释所有导出表字段、时间与缺失规则，以及手动领域单位。未知供应商载荷仍保留原字段，不声称全部已验证。同版本空实例恢复见下文。版本变化必须同时更新生产者、独立读取器和契约测试。

## 回归验证

```bash
python3 -m unittest discover -s tests/archive -p 'test_*.py' -v
cd src/backend_api
cargo test --locked export_contains_raw_and_revisions_without_credentials_and_delete_cleans_them
```

Rust 导出测试调用 Python 3 校验真实生产者生成的合成档案。它不使用个人健康数据。Python 测试覆盖正常读取、原件篡改、缺失、跨账户、关联断裂、版本漂移、大小预算、重名成员及错误信息不泄露健康内容。

2026-10-06 本地验证：9 项 Python 导出/恢复回归及 Rust 真实生产者导出/恢复集成测试通过。所有数据均为合成样例，没有使用真实健康档案。


## 第一版空实例恢复

恢复工具使用仓库中的当前 schema，不执行历史迁移。目标必须是不存在的绝对目录，其父目录必须存在。工具先校验 ZIP，再创建目标目录。失败时只移除本次新建的目录，不覆盖已有目录。

```bash
python3 src/archive/restore_export.py /absolute/path/export.zip --destination /absolute/path/new-data --username alice --max-bytes 1073741824
```

恢复保留用户 ID、数据修订、原件和记录历史。账户保持禁用，不恢复密码、会话、任务队列、模型凭据或设备权限。未完成的分析变为 `failed/restore_interrupted`。处理副本未导出，因此清除处理副本路径及摘要；原文件保留。`restore.json` 记录恢复结果和这些调整。

使用 API 程序生成配置，填写必需的 OCR 服务配置，再用交互密码提示启用账户。以下命令中的程序路径需要对应实际构建产物。

```bash
/path/to/helpyourself --data-dir /absolute/path/new-data init-config
/path/to/helpyourself --data-dir /absolute/path/new-data enable-user --username alice
/path/to/helpyourself --data-dir /absolute/path/new-data serve
```

`enable-user` 要求新密码，撤销已有会话。它是显式管理员操作。普通 `reset-password` 不自动启用已禁用账户。恢复不会自动发起 OCR 或模型请求。已恢复分析保留原输出；未完成分析需要用户重新提交。
