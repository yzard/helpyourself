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
| reports/input/get | report_id、run_id、page | 完整页面证据，text_layer、词坐标、raw_bbox_xml、error_code；按用户归属鉴权 |
| reports/extraction/get | report_id、run_id、page、stage（当前为 ocr） | response_body、status_code、content 可空 |
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

OCR 候选始终 pending、metric_id=null，不能自行确认。有效结构携带 warnings 时，任务成功且相关页为 needs_review；完整提醒随回复保留。网络、结构或字段校验失败仍是任务失败；成功完成提取不代表已经人工审核。原单位和文本结果都保留；仅已确认、已映射、可比较且有采样日期的结果进入数值趋势。当前字典 10 项见 metrics/list（含 accepted_units、conversion_version）。API 使用 lab-units-v2 的指标专属注册表，支持常见质量浓度、血脂/血糖/肌酐摩尔浓度及 HbA1c IFCC→NGSP 的明确规则；不会让 OCR 换算。受控空格/大小写/微符号别名不改变原单位，未知单位不做模糊纠正。

reports/get 的 observations 含 interpretation（result/reference/version）：状态、明确原因、原单位及单位来源、标准单位、value 或上下界/包含性、显示文本和规则/来源。简单比较符/范围可换算边界但不会成为精确趋势点。参考范围显式单位优先，否则使用报告结果列单位并标记 unit_origin=observation；条件化/复杂范围保留原文，不生成假阈值。

trends/get 返回标准 value/unit、original、标准化 reference、excluded（observation_id/reason）与计数、转换版本。只有已确认/映射、有日期且精确可转换的值进入 points。报告查询只返回 extraction_inputs 元数据；完整文本/XML 经 reports/input/get 按需读取，避免列表加载整个证据正文。

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

platform 为 apple_health/health_connect；第二项只提供服务器协议，尚无 Android 客户端。批次最大 500 条、40 MiB；单条 payload 最大 32 MiB，不接受截断数据。Health 同步在读取正文前取得最多两个共享准入槽，health/list、health/aggregate 共用这两个槽；满时返回 429，客户端保留原批次重试。JSON 类型支持 application/json 及 application/*+json；非法 JSON 为 400、超限为 413。批次解析、校验、摘要和原始 envelope 编码在有界 CPU 执行域准备，事务内不重复编码。health/list 累积 envelope 超过 40 MiB 时明确失败，请减少 limit。查询原件索引不返回大载荷，单个原件下载为流式响应。

同批次不同内容冲突；同记录同版本不同内容冲突；旧版本忽略；删除标记不能被更高版本旧上传覆盖。Apple 删除事件不知道 source 时用 `*`，仅删除事件可用。删除清除内容、历史和对应原件，只留抑制标记。

有效修订的完整 envelope 同时保存 SQLite 和 raw/apple_health 或 raw/google_health。iOS payload.raw_archive 是 HealthKit 可读样本的 secure archive/base64，FHIR 与专门查询的序列另保留。服务端允许其他原始字段，不要求每种类型都能聚合。

coverage_status：observed/no_visible_samples/error/unsupported；它描述该批查询，不代表 Apple 已授予读取权限或整个历史完整。服务器保存最近批次状态、首次/最近成功时间；visible_start_at/visible_end_at 是当前平台/类型的归档范围，不证明中间完整。iOS 每次锚点查询上限 10 个对象，按字节及记录数拆请求；先持久化全部批次及候选锚点，全部得到回执才推进。断线或响应丢失重放相同批次；特征/活动环/CDA/药物快照独立检查点，不从隐藏或空结果推断删除。

日聚合版本 health-daily-v2，分析快照引用同一版本。支持 heart_rate/resting_heart_rate/hrv_sdnn/steps/sleep/workout。数量类型严格检查单位；前 3 项为样本均值，步数按区间重叠比例分配，睡眠/运动为同来源区间并集。每个来源分别输出；没有数据时无条目或 value=null，不能当作零。每个 day/source 的 sample_count 是实际参与计算的记录数；source_record_count 是该日来源候选数，excluded_sample_count 是未参与数。value_status 为 available/missing/numeric_overflow；后两种 value=null。非数值、错误单位、不适用睡眠类别及无重叠区间不进入计算，原始载荷仍完整归档。最多 367 天、50,000 条候选明细，超限明确失败而非截断。

## 限制和错误

上传每个 part 默认最大 20 MiB、100 页、4000 万像素，最多两次并发上传。通常仅 file，PDF/PNG/JPEG 按内容解析。HEIC/HEIF 使用 file=JPEG 识别副本、original=原文件（该顺序）；后端校验 original 的 MIME/ftyp 容器头，尚不解码整个 HEIC。其他附加字段拒绝。两份字节分别有摘要；相同上传 ID 携带不同原件、处理摘要或文件名返回 409。

file.relative_path 指向 raw/photos 或 raw/documents 原件；原件名称、content_type、sha256、byte_count 均描述原件。HEIC 的 processing_path/processing_sha256 描述 derived 下 JPEG；其他文件这两个字段为空。download 始终返回原件。相机扫描每页 PNG 独立上传，当前每页对应一份报告。

extraction_outputs 保存每个阶段最终接收的 UTF-8 HTTP 回复（上限 8 MiB），成功或解析失败均可取回；不包括网络失败时不存在的正文、超限正文和中间自动重试回复。已解析 content 与完整 response_body 分开保存。报告元数据列表不加载完整回复正文；读取单个回复及导出均检查用户归属。

导出 manifest.version=3：表 JSONL（含完整健康历史、OCR 回复及人工修订）、辅助 CSV（含标准化结果/参考区间/状态/规则版本），extraction_inputs.jsonl 收录每页证据，以及按数据库 relative_path/raw_path 收录的有效原件；ZIP 使用 raw/apple_health、raw/photos、raw/google_health、raw/documents 路径。删除后的内容不再导出。

列表按 ID 游标，不保证多请求期间新增数据的快照；趋势最多读取 10,000 条已确认结果，超限明确报错。iPhone 可比较两项指标，图表使用各自尺度；API 保持独立 metric_id/unit。

上传校验/摘要、大载荷编解码、PDF 证据处理与 ZIP 使用共享两个 CPU 槽；HTTP 请求无法准入返回 429，不累积无限重 CPU 队列。取消后正在运行的阻塞闭包继续持有槽位和临时文件使用权；服务停服等待它们结束，再清理临时文件。

领域错误 `error.code/message`：400、401、404、409、413、429、503；内部细节隐藏。框架 JSON/方法拒绝可能返回框架正文，客户端以状态码兜底。模型、文件路径、凭据不允许出现在错误或访问日志中。

## 独立 OCR 内部接口

手机仅调用本文件中的 backend_api 接口。backend_api 使用独立服务密钥调用 backend_ocr 的 POST /api/v1/documents/extract；该接口不开放给手机用户会话。请求、完整原生回复、错误码和推理限额见 [OCR 运维](runbooks/ocr.md)。
