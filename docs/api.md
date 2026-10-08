# API v1 实现契约

前缀 `/api/v1/`；除下载 GET 外均 POST。JSON 为 snake_case。仅 `server/status` 和 `session/login` 无需会话，其余需 `Authorization: Bearer <token>`。未列出的字段不接受，不能传 user_id。ISO 日期支持 `YYYY-MM-DD` 或带时区 RFC3339，健康明细时间为 Unix 整数秒；iPhone payload 同时保存原始小数秒。

API 同时通过 `GET /` 和固定 `/assets/{app.css,app.mjs,client.mjs,presentation.mjs}` 提供嵌入式 Web GUI；这些页面资源公开，数据读取继续使用上述认证。未知路由仍返回 JSON 404，不用网页覆盖 API 错误。[网页范围](runbooks/webgui.md)限定为浏览与现有血脂 AI 预设，导入和健康平台读写由移动端承担。

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

OCR 候选始终 pending、metric_id=null，不能自行确认。有效结构携带 warnings 时，任务成功且相关页为 needs_review；完整提醒随回复保留。网络、结构或字段校验失败仍是任务失败；成功完成提取不代表已经人工审核。原单位和文本结果都保留；仅已确认、已映射、可比较且有采样日期的结果进入数值趋势。当前字典 17 项见 metrics/list（含 accepted_units、conversion_version）。API 使用 lab-units-v3 的指标专属注册表，支持常见质量浓度、血脂/血糖/肌酐摩尔浓度及 HbA1c IFCC→NGSP 的明确规则；不会让 OCR 换算。受控空格/大小写/微符号别名不改变原单位，未知单位不做模糊纠正。

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

导出 manifest.version=1：表 JSONL（含完整健康历史、OCR 回复及人工修订）、辅助 CSV（含标准化结果/参考区间/状态/规则版本），extraction_inputs.jsonl 收录每页证据，以及按数据库 relative_path/raw_path 收录的有效原件；ZIP 使用 raw/apple_health、raw/photos、raw/google_health、raw/documents、raw/manual、raw/file_import 路径。删除后的内容不再导出。

列表按 ID 游标，不保证多请求期间新增数据的快照；趋势最多读取 10,000 条已确认结果，超限明确报错。iPhone 可比较两项指标，图表使用各自尺度；API 保持独立 metric_id/unit。

上传校验/摘要、大载荷编解码、PDF 证据处理与 ZIP 使用共享两个 CPU 槽；HTTP 请求无法准入返回 429，不累积无限重 CPU 队列。取消后正在运行的阻塞闭包继续持有槽位和临时文件使用权；服务停服等待它们结束，再清理临时文件。

领域错误 `error.code/message`：400、401、404、409、413、429、503；内部细节隐藏。框架 JSON/方法拒绝可能返回框架正文，客户端以状态码兜底。模型、文件路径、凭据不允许出现在错误或访问日志中。

## 独立 OCR 内部接口

手机仅调用本文件中的 backend_api 接口。backend_api 使用独立服务密钥调用 backend_ocr 的 POST /api/v1/documents/extract；该接口不开放给手机用户会话。请求、完整原生回复、错误码和推理限额见 [OCR 运维](runbooks/ocr.md)。

## Wellness and open file import

All routes below require the current user's Bearer session. Source data remains in the health archive, with revision history and export coverage.

`POST /api/v1/wellness/day` accepts `date` (`YYYY-MM-DD`), `timezone` (IANA name), and `source_priority` (metric names mapped to ordered source IDs). The response includes six metrics, their source alternatives, history, coverage, and archive revision. A source ID combines its platform and producer. Multiple available sources require a choice. Missing data remains null. Personal comparisons require 28 earlier valid days from the same source within 42 days. These comparisons describe data. They do not estimate disease risk.

`POST /api/v1/wellness/sources` accepts `{}`. The response lists visible source and type combinations, record counts, and timestamp ranges. An empty range does not establish denied permission.

`POST /api/v1/wellness/entries/save` accepts `record_id` (UUID), `version` (positive integer), `batch_id` (UUID), `at` (Unix seconds), `timezone`, and `entry`. The entry contains `kind` and `content`. Supported kinds are `journal`, `training`, `nutrition`, `cycle`, `body`, `blood_pressure`, and `breathing`. Field definitions and units are in `src/backend_api/wellness/entries.rs`. Unknown fields and invalid units fail the request. Optional numeric fields accept null. Training calculations use CR10 effort and minutes. External training volume remains separate for each exercise.

`POST /api/v1/wellness/entries/list` accepts `start_at`, `end_at`, and nullable `kind`. The start is inclusive and the end is exclusive. The range cannot exceed 366 days or 1,000 entries. A larger result requires a shorter range. Each result includes the original entry, revision, source, and applicable calculation.

`POST /api/v1/wellness/entries/delete` accepts `record_id`, `version`, `batch_id`, and `kind`. Deletion removes the entry's archived revisions and prevents later uploads from restoring it. Independent backups remain outside the deletion scope.

`POST /api/v1/wellness/import/gpx` accepts `filename` and `xml`. The parser accepts UTF-8 GPX 1.1 with its standard namespace, timed track points, and increasing timestamps. The file cannot exceed 4 MiB or 50,000 points. The HTTP JSON body cannot exceed 5 MiB. DTDs are rejected. The archive retains the XML text and SHA-256 digest. Distance uses consecutive points within each segment. Elapsed duration includes pauses and gaps. It is not active exercise duration. FIT is not supported yet. TCX has a separate endpoint below.

Manual entries use `manual:helpyourself`. GPX records use `file_import:gpx-1.1`. TCX records use `file_import:tcx-2`. Their raw envelopes use `raw/manual` and `raw/file_import`. Public Health sync endpoints cannot write these reserved connections. The initial schema supports these platforms. All database and export schemas use version 1. No historical migration or compatibility path exists.

`POST /api/v1/wellness/import/list` accepts nullable `after_id` and returns up to 100 imports with `next_after_id`. The list omits XML bodies. `POST /api/v1/wellness/import/delete` accepts `record_id`, `source_id`, and `expected_version`. Deletion invalidates affected analysis and exports, removes archived revisions, and schedules original-file cleanup. Reimporting the same deleted content fails. Uploading identical content under another filename retains the first original.

`POST /api/v1/wellness/sleep` accepts `start_at`, `end_at`, and `timezone`. The interval cannot exceed 90 days. Results keep each source separate and include original stage intervals with record IDs and revisions. A 90-minute gap separates groups. This engineering threshold does not classify main sleep or naps. In-bed evidence is required for efficiency. Missing, conflicting, and truncated sessions do not produce a complete sleep duration. This endpoint does not infer wakefulness from gaps.

`POST /api/v1/wellness/clinical-age` accepts `report_id`, `age_at_collection_years`, and `research_acknowledged`. The caller must acknowledge research use. Age refers to the collection date and must be between 20 and 120. The model requires nine exact, reviewed values from the same report and collection date. Missing inputs produce `insufficient_data`. Duplicate inputs, unsupported units, and mixed collection dates fail the request. The response includes model inputs, observation revisions, archive revision, conversion version, and the published formula reference. The result does not establish measured biological age or disease risk. These read-time results are not persisted as new observations.

Manual edits require the next version. A stale version or a change of entry type returns a conflict. A repeated identical version remains safe to retry. An entry list is also limited to 4 MiB of stored payloads. A daily metric that exceeds the existing query budget reports `query_limit_exceeded`; other daily metrics remain available.

### 来源趋势、偏好和回顾

所有接口使用 POST 和当前账户鉴权。

`wellness/preferences/get` 返回 `version` 与 `preferences`。`wellness/preferences/save` 接收 `expected_version`、UUID `batch_id` 与完整偏好。偏好包含 `source_priority`、`favorite_metrics` 和可空 `sleep_target_minutes`。收藏为空时显示全部六类指标。睡眠目标是用户指定值，不是生理需求估计。偏好进入手动来源的修订、导出和删除流程。过期修改返回冲突。

`wellness/series` 接收 `record_type`、`start_at`、`end_at` 和 `maximum_gap_seconds`。窗口为左闭右开，最多 90 天、50000 条记录。类型为 `blood_glucose`、`vo2_max`、`body_mass`、`body_fat`、`oxygen_saturation`、`respiratory_rate`。每来源返回原值、换算值、单位、记录 ID 与版本。血糖摘要只在相邻样本间隔不超过指定值时左侧保持，允许间隔为 1–1800 秒。最后一点不推断持续时间。重复时间或未知单位停止该来源摘要。稀疏血糖样本不被认定为 CGM。Apple Health 的百分比输入是 0–1，显示时转换为 0–100。其他平台的 `%` 契约是 0–100，不猜测量级。

`wellness/timeline` 接收 `start_at`、`end_at` 与可空 `cursor`，最多 31 天，每页 200 项。范围包括报告上传、原睡眠区间、运动与手动领域记录。报告时间是上传时间。将返回的 `next_cursor` 原样用于下一页。账户数据修订变化时必须从第一页重新读取。

`wellness/review` 接收 `start_at`、`end_at` 与 `timezone`。它使用手动日志的 366 天、1000 项与 4 MiB 限制，返回每日已记录合计、缺项数量、配对测量与窗口力量纪录。没有日志的日期不补零。宏量完整只表示已有日志中的字段齐全。力量纪录按动作名称与重复次数分开，不预测 1RM。结果包含输入 ID/版本、算法版本与账户修订。

导出 v1 的 `members` 对除 `manifest.json` 外的每个成员记录 SHA-256 和字节数。`table_counts` 记录各表行数，`table_columns` 记录 SQLite 列结构，`database_schema_version` 为 1。清单未签名。修改清单和全部内容后重算摘要，仍不能据此证明真实性。独立读取器仅支持第一版格式，不保留历史格式兼容。

`wellness/import/tcx` 接收与 GPX 相同的 `filename` 和 UTF-8 `xml`。单文件支持一个 TCX v2 Activity，最多 4 MiB、1000 圈和 50000 点，覆盖不超过七天。不要求 GPS，因此可保留室内训练。使用来源圈次距离，来源计时与经过时间分开。保存心率、踏频、海拔、位置及完整原 XML。扩展字段仍保留在原 XML，不声称全部已标准化。依据 [Garmin TCX v2 schema](https://www8.garmin.com/xmlschemas/TrainingCenterDatabasev2.xsd)。导入列表包含 `source_id`。删除请求必须同时提交 `record_id`、`source_id` 和 `expected_version`。

呼吸日志的 `breaths_per_minute` 可空。网页和 iOS 的五分钟呼吸引导记录实际前台计时时长，用户点击保存后才写入日志。没有传感器测量时频率保持 null，计时不会生成压力或恢复评分。

`wellness/sleep/regularity` 使用 `start_at`、`end_at`、`timezone`。窗口必须是七个连续 24 小时，开始时间对齐到整分钟。按来源在 UTC 分钟起点读取明确睡眠/清醒区间，比较相隔 86400 秒的状态。每个来源必须有 10080 个已知状态且同时存在睡眠与清醒。任何未知状态或冲突都返回 null。卧床不是清醒。夏令时切换不会把 24 小时改为本地日长。最多读取 50000 条区间、64 个来源。返回全部输入 ID/修订、窗口、算法版本、覆盖和账户数据修订。


`training_day` 手动日志接收 `status`（`rest` 或 `all_sessions_logged`）与 `note`。日期由记录时间和记录时区决定。训练回顾的 `training` 返回最后 7／28 个已结束本地日历日。仅明确休息日计零；所有训练均填写努力分且确认记录完整，才视为完整训练日。不同记录时区的确认不套用，冲突确认保持未知。窗口缺完整日期时，`total_load_au` 与 `daily_mean_au` 为 null，`observed_sum_au` 单独展示已有记录。

`wellness/series` 另支持 `heart_rate`。可选 `declared_maximum` 包含 `bpm` 和必填 `source`。最大心率为 50–300 bpm；无声明时仅显示绝对分区。心率输入接受 `count/min` 与 `bpm`。相邻样本间隔在 `maximum_gap_seconds` 内才累计时间，最后一点无持续时间。Edwards 分区为最大值的 50–60、60–70、70–80、80–90、90–100%，左闭右开，100% 纳入末区。任何样本超过最大值，负荷保持 null。网页和 iOS 默认查看最近 24 小时，允许修改采样间隙和最大值来源。默认 15 秒是明确展示的工程假设，不是所有设备的统一协议。

`sleep_correction` 包含 `source`、`session_start`、`session_end`、`classification`、可空 `corrected_asleep_minutes`、`basis_revisions` 和必填 `note`。记录时间必须等于会话开始时间。分类为 `main_sleep`、`nap` 或 `unclassified_session`。会话最多七天，最多引用 1000 条记录。人工估计不能超过会话时长。睡眠页面将人工时长单列，保留设备分期、原时长和效率。来源修订或会话范围变化后显示 `stale_basis`，多个修正显示冲突。修正不生成分钟状态，不改变 SRI。

导出清单的 `field_dictionary` 包含第一版字段语义。校验器要求其表和列集合与 SQLite 列字典一致。字典解释归属、时间、修订、单位和缺失规则；供应商未标准化字段保持原始载荷。

### Behavior association research

Journal content requires `behaviors`, `measurements`, and `times` maps. An empty map is valid. Boolean behaviors use boolean values. Measurements use `{ "value": 120, "unit": "mg" }`. Times use integer minutes after midnight from 0 through 1439. Names must be distinct across all three maps.

`POST /api/v1/wellness/associations/run` accepts `end_date`, `timezone`, `outcome`, `source`, `behaviors`, `covariates`, and `lag_days`. The outcome uses a supported daily metric. The source uses the exact `platform:source_id` identifier. Specify 1–8 behavior names and 0–3 covariate names before the run. The lag is 0 or 1. The fixed window covers 90 completed local dates.

`POST /api/v1/wellness/associations/list` returns the current user's saved result metadata. `POST /api/v1/wellness/associations/get` accepts `result_id` and returns that user's result. Results include input snapshots, daily exclusion reasons, nominal statistical estimates, and calibration status. A source revision removes saved results. The server retains at most 100 results per user.

The current protocol fails synthetic error-rate calibration. `passes_by_threshold` is null, and `significance_decisions_enabled` is false. The client must not present nominal p/q values as confirmed discoveries.

### 扩展领域接口（schema v1）

以下路径均以 `/api/v1/` 开头，使用 POST、当前账户鉴权和修订一致性检查。未知字段被拒绝。

| 路径 | 请求和行为 |
| --- | --- |
| `wellness/library` | `kind` 为 exercise、workout_template、food、recipe、nutrition_goals、reminder 或 memory。返回最多 1000 条当前记录，受 4 MiB 限制，不受日期窗口限制。 |
| `wellness/hrv` | 起止时间窗口返回来源和协议分开的 NN/RMSSD/lnRMSSD 结果。最多 90 天、1000 条原始窗口和 16 MiB。未知心搏质量保持不可计算。 |
| `wellness/food/portion` | 接收 `basis`（record_id、version、amount、unit）和 `meal`。food 使用 g，recipe 使用 servings。返回营养素、缺项与来源快照，供人工确认后保存。 |
| `wellness/nutrition/day` | `date` 与 `timezone`。返回已有摄入、缺值计数及用户目标。重复同时间目标不自动选取。 |
| `wellness/reminders` | 指定时间窗口返回提醒的本地时间发生项，最多 31 天。静默时段抑制通知。不存在的夏令时时刻明确标记，重复时刻只取首次。 |
| `wellness/records/list` | `kind` 为 ecg、clinical、blood_pressure，另接可空 `after` 游标。每页 50 项。 |
| `wellness/records/get` | `platform`、`source_id`、`record_id`、`sample_offset`。每页最多 5000 个原始 ECG 或训练样本。返回 FHIR JSON、来源分类、同来源关联记录或训练圈段摘要。 |
| `wellness/coach` | `question`、`date`、`timezone`、`style`、可空 `prior_run_id`、`allow_drafts`。分析提供者必须启用。按来源证据生成问答，通过原分析任务接口读取。 |
| `wellness/import/fit` | `filename` 和 `data_base64`。最多 4 MiB 原始字节、50000 个定义/数据消息、一个 Activity Session、七天时长。强制 CRC 校验。原字节、摘要、全部解码消息及来源单位保留。 |
| `wellness/report` | `start_date`、`end_date`、`timezone`。首尾日期都包含，最多 366 天。返回六类按来源分开的日序列、中位数、缺失天数和手动记录。超量来源指标明确标记。 |
| `wellness/diet` | `date`、`timezone`。返回含当天的 1、7、28 日 HEI-2020。每个日期必须有一份完整、适龄的确认评估。重复评估、缺日或缺字段停止该窗口总分。 |
| `wellness/meal-glucose` | `start_at`、`end_at`、`maximum_gap_seconds`。最多七天。返回分来源血糖与同时段餐食/训练事件，不推断因果或生成餐食评分。 |
| `wellness/meals` | `start_at`、`end_at`。返回膳食计划及按食谱快照计算的购物克数。跳过的计划不计入购物。不同来源或营养快照不合并。 |

训练日志必须提供 `ended_at`、`paused_minutes`、`duration_basis` 和可空 `rpe_answered_at`。`duration_basis` 为 elapsed_including_pauses 或 active_excluding_pauses。日志时间表示训练开始。起止时差必须符合所选时长口径。提供 CR10 努力分时必须记录回答时间，不能早于训练结束。

`nutrition` 的 `micronutrients` 为必填对象，缺项保持未知。可空 `origin` 保留来源记录版本、分量及食品/食谱快照。当前不把外部食品数据库内容自动当成已确认摄入。

`diet_quality` 保存 `totals`、`source`、`complete_day`、`age_two_or_older` 与 `note`。14 个输入键和单位见 `wellness/diet.rs` 的 `KEYS`。杯、盎司和茶匙均为食物模式等价值，不能使用普通重量替代。`meal_plan` 保存 `recipe` 快照、`planned_servings` 和 planned/skipped `status`。实际摄入另存 nutrition。

Coach 输出只允许引用输入提供的证据 ID。可审阅草稿限制为训练计划、营养目标和提醒。系统拒绝未请求的草稿及其他写入类型。删除记忆会清除过期分析快照，不再用于后续对话。最多保留八轮先前对话，完整输入受 512 KiB 限制。

`wellness/food/lookup` 接收数字 `barcode`（8–14 位），只把条码发送给 Open Food Facts。它固定使用官方 HTTPS 地址，禁止重定向，响应最多 512 KiB，超时 15 秒。返回未保存的 food 草稿。用户必须核对标签为每 100 克后，才能保存 `external_source.mass_basis_confirmed=true` 的食品。每 100 毫升不能自动视为每 100 克。缺失或非数值营养项不填零。保存后的 `external_source` 包含提供者、条码、读取时间及完整返回 JSON，随原始归档导出或删除。来源保留 Open Food Facts 与 ODbL 标识。查看[官方接口说明](https://openfoodfacts.github.io/openfoodfacts-server/api/)。

训练日与 7／28 日窗口拒绝合计不同 `duration_basis`，返回 incompatible_duration_protocols。单个训练仍保留。NN 记录的起止时长须与窗口末尾偏移一致，容差一秒；七日 lnRMSSD 中位数至少需要五个有效日。

`health/aggregate` 按七个日历日分块读取。每块最多 50000 条记录，最终最多 10000 个来源日。返回结果前核对账户修订。该方式保留跨块睡眠和步数区间的日内截断语义。一个块超限仍返回明确限制错误。每日基线和长期报告复用此路径。

`cycle.context` 支持 cycle、pregnancy、postpartum、perimenopause 和 unknown。独立 `cycle_prediction` 条目保存 `start_date`、`end_date`、`generated_at`、`source`、`uncertainty`、`note`。日期使用 YYYY-MM-DD，区间最多 366 天。来源与不确定性说明必须填写，未知准确性也须明确写出。预测不转换为实际经期或排卵记录。
