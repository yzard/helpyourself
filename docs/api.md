# API v1 实现契约

前缀 `/api/v1/`；除下载 GET 外均 POST。JSON 为 snake_case。仅 `server/status` 和 `session/login` 无需会话，其余需 `Authorization: Bearer <token>`。未列出的字段不接受，不能传 user_id。ISO 日期支持 `YYYY-MM-DD` 或带时区 RFC3339，健康明细时间为 Unix 整数秒；iPhone payload 同时保存原始小数秒。

| 组件/操作 | 请求 | 返回 |
| --- | --- | --- |
| server/status | {} | status、api_version、capabilities |
| session/login | username、password | token、expires_at、user |
| session/logout | {} | revoked |
| user/get | {} | user_id、username |
| user/delete | confirmation：当前用户名 | cleanup_queued、sessions_revoked |
| files/upload | multipart file，HEIC/HEIF 可另附 original；X-Upload-Id UUID | file、job、replayed |
| files/list / reports/list / jobs/list | limit 1–100、after_id 可空 | files / reports / jobs |
| files/get / jobs/get | file_id / job_id | 对应对象 |
| files/{file_id}/download | GET | 原件流 |
| jobs/retry | job_id | 更新后的任务；仅 failed 可重试 |
| reports/get | report_id | report、context、observations、relation、pages、extraction_outputs 元数据、duplicate_candidates |
| reports/extraction/get | report_id、run_id、page、stage（ocr/document_parser） | response_body、status_code、content 可空 |
| reports/review | 见下方 | 新报告快照；可能附 analysis_trigger |
| reports/relate | report_id、expected_revision、preferred_report_id 可空、kind | updated |
| reports/delete | report_id、expected_revision | cleanup_queued |
| observations/history | observation_id | history 修订数组 |
| metrics/list | {} | metrics：metric_id/name/standard_unit |
| trends/get | metric_ids 1–20、start_at/end_at 可空 | points、incomparable_count、算法/转换版本 |
| health/connect | platform、installation_id UUID | connection_id、platform、installation_id |
| health/sync | 见下方 | batch_id、replayed |
| health/coverage | {} | coverage、read_permission=unknown |
| health/list | limit 1–500、offset 非负 | records，平台和完整 envelope |
| health/raw/list | limit 1–100、after_id 可空 | files：raw_id、platform、source_id、record_id、relative_path、received_at |
| health/raw/{raw_id}/download | GET | 该修订的完整原始 JSON |
| health/aggregate | record_type、start_date、end_date、timezone IANA | days、timezone、算法版本、来源策略、computed_at |
| exports/create | {} | export_id |
| exports/list | {} | exports，最近 100 项 |
| exports/delete | export_id | cleanup_queued |
| exports/{export_id}/download | GET | 当前 ready 的 ZIP；旧快照拒绝 |
| analysis/create | start_date、end_date、timezone | run_id；血脂全历史，健康上下文按所选日期 |
| analysis/list | {} | runs，最近 100 项；topic=lipid_risk |
| analysis/get | run_id | status、input、output、feedback、verification_status |
| analysis/retry | run_id | queued；仅 failed 可重试 |
| analysis/feedback | run_id、note | saved、verification_status |

## 复核

```json
{
  "report_id": "UUID",
  "expected_revision": 1,
  "context": {"fasting": true, "recent_exercise": null, "illness": null, "medications": null, "notes": null},
  "observations": [{
    "observation_id": null,
    "expected_revision": null,
    "status": "confirmed",
    "payload": {
      "raw_name": "LDL Cholesterol", "raw_result": "120", "raw_unit": "mg/dL",
      "reference_range": "<100", "report_flag": "H", "sampled_at": "2026-08-01",
      "metric_id": "ldl_cholesterol",
      "source": {"page": 1, "quote": "LDL Cholesterol 120 mg/dL", "bounding_box": null},
      "notes": null
    }
  }]
}
```

新项 ID/修订为空；修改项必须带 observation_id 和 expected_revision。状态为 pending/confirmed/rejected。context=null 表示不改背景；对象中的空值可以清除背景。一个请求事务执行，可只提交部分行；报告或行版本过期返回 409。bounding_box 是归一化 left/top/right/bottom；无法定位时为空，不伪造位置。

OCR 候选始终 pending、metric_id=null，不能自行确认。原单位和文本结果都保留；仅已确认、已映射、可比较且有采样日期的结果进入数值趋势。首版字典 10 项见 metrics/list；胆固醇和甘油三酯 mmol/L→mg/dL 用独立因子，其余目前只接受标准单位，不做猜测。

重复候选只表示文件字节相同；独立上传仍保留。用户决定 duplicate 或 superseded 关系，被替代报告退出趋势。禁止关系链/环，解绑需要新的版本。

## 健康批次

```json
{
  "connection_id": "UUID", "batch_id": "UUID", "record_type": "steps", "coverage_status": "observed",
  "records": [{"record_id": "platform-id", "source_id": "source.bundle", "record_type": "steps",
    "start_at": 1785542400, "end_at": 1785542460, "version": 1, "deleted": false,
    "payload": {"value": 100, "unit": "count", "metadata": {}}}]
}
```

platform 为 apple_health/health_connect；第二项只提供服务器协议，尚无 Android 客户端。批次最大 500 条、40 MiB；单条 payload 最大 32 MiB，不接受截断数据。health/list 累积 envelope 超过 40 MiB 时明确失败，请减少 limit。查询原件索引不返回大载荷，单个原件下载为流式响应。

同批次不同内容冲突；同记录同版本不同内容冲突；旧版本忽略；删除标记不能被更高版本旧上传覆盖。Apple 删除事件不知道 source 时用 `*`，仅删除事件可用。删除清除内容、历史和对应原件，只留抑制标记。

有效修订的完整 envelope 同时保存 SQLite 和 raw/apple_health 或 raw/google_health。iOS payload.raw_archive 是 HealthKit 可读样本的 secure archive/base64，FHIR 与专门查询的序列另保留。服务端允许其他原始字段，不要求每种类型都能聚合。

coverage_status：observed/no_visible_samples/error/unsupported；它描述该批查询，不代表 Apple 已授予读取权限或整个历史完整。服务器保存最近批次状态、首次/最近成功时间；visible_start_at/visible_end_at 是当前平台/类型的归档范围，不证明中间完整。iOS 每次锚点查询上限 10 个对象，按字节及记录数拆请求；先持久化全部批次及候选锚点，全部得到回执才推进。断线或响应丢失重放相同批次；特征/活动环/CDA/药物快照独立检查点，不从隐藏或空结果推断删除。

日聚合支持 heart_rate/resting_heart_rate/hrv_sdnn/steps/sleep/workout。数量类型严格检查单位；前 3 项为样本均值，步数按区间重叠比例分配，睡眠/运动为同来源区间并集。每个来源分别输出；没有数据时无条目或 value=null，不能当作零。最多 367 天、50,000 条候选明细，超限明确失败而非截断。

## 限制和错误

上传每个 part 默认最大 20 MiB、100 页、4000 万像素，最多两次并发上传。通常仅 file，PDF/PNG/JPEG 按内容解析。HEIC/HEIF 使用 file=JPEG 识别副本、original=原文件（该顺序）；后端校验 original 的 MIME/ftyp 容器头，尚不解码整个 HEIC。其他附加字段拒绝。两份字节分别有摘要；相同上传 ID 携带不同原件、处理摘要或文件名返回 409。

file.relative_path 指向 raw/photos 或 raw/documents 原件；原件名称、content_type、sha256、byte_count 均描述原件。HEIC 的 processing_path/processing_sha256 描述 derived 下 JPEG；其他文件这两个字段为空。download 始终返回原件。相机扫描每页 PNG 独立上传，当前每页对应一份报告。

extraction_outputs 保存每个阶段最终接收的 UTF-8 HTTP 回复（上限 8 MiB），成功或解析失败均可取回；不包括网络失败时不存在的正文、超限正文和中间自动重试回复。已解析 content 与完整 response_body 分开保存。报告元数据列表不加载完整回复正文；读取单个回复及导出均检查用户归属。

导出 manifest.version=2：表 JSONL（含完整健康历史、OCR 回复及人工修订）、辅助 CSV，以及按数据库 relative_path/raw_path 收录的有效原件；ZIP 使用 raw/apple_health、raw/photos、raw/google_health、raw/documents 路径。删除后的内容不再导出。

列表按 ID 游标，不保证多请求期间新增数据的快照；趋势最多读取 10,000 条已确认结果，超限明确报错。iPhone 可比较两项指标，图表使用各自尺度；API 保持独立 metric_id/unit。

领域错误 `error.code/message`：400、401、404、409、413、429、503；内部细节隐藏。框架 JSON/方法拒绝可能返回框架正文，客户端以状态码兜底。模型、文件路径、凭据不允许出现在错误或访问日志中。
