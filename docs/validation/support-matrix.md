# 数据支持矩阵

更新：2026-10-02。“代码已实现”不代表 Apple SDK 编译、HealthKit 真机或真实 OCR 准确性通过；Apple SDK/真机由用户在 Mac/iOS 验收，真实个人报告与 HEIC 的准确率仍缺授权样本。

| 来源/类型 | 原始归档粒度 | 当前证据与限制 |
| --- | --- | --- |
| PDF、PNG、JPEG | 原字节、摘要、文件元数据、完整 OCR/解析回复、候选与出处、人工修订 | Rust 回归及真实 GPU 合成 PNG、文字 PDF、扫描/混合两页 PDF 多单位→HTTPS 数据闭环通过；真实报告待测 |
| HEIC/HEIF | 原字节在 raw/photos；JPEG 识别副本在 derived | 合成容器头及双 part 原字节/重放/删除测试通过；真实 HEIC 与原生照片导入待测，后端只校验容器头 |
| 扫描纸质报告 | VisionKit 输出的每页 PNG 单独归档 | 每页分别创建报告，不声称保留相机传感器 RAW 或自动合并为多页 PDF |
| backend_ocr / Qwen3.8 + NInfer | 各 run/page 的完整服务 HTTP 正文、原生 raw_response_body、reasoning、content、结构化结果、model/engine/prompt_version | 原生正文上限 3 MiB，服务回复上限 8 MiB；结构失败已接收回复保留，网络失败无回复；真实 GPU 合成识别及卸载通过；有效 warnings 记录 needs_review，不当成执行失败 |
| PDF 文本层 | 按 run/page 的文字、归一化词坐标、原始 bbox XML、状态/错误与处理版本；原 PDF 仍保留 | Poppler 和真实 GPU 文字/扫描/混合 PDF、归属/导出/删除测试通过；全部页面始终保留视觉，真实文档准确性待测 |
| 单位与参考区间 | 原值/单位/范围，加可重算派生结果、规则/来源/版本 | 现有 10 项常见单位；比较符/范围不伪装精确值，复杂条件/歧义/未知保持不可比较；无临床阈值推断 |
| 所有人工报告项目 | 原名、原值/单位/范围、日期精度、出处和不可变修订 | 不受 HealthKit 类型范围限制；文本、比较符或未映射项仍归档 |
| 血检数值趋势 | 当前 10 项注册指标与显式单位转换 | 总胆固醇/LDL/HDL/甘油三酯换算有对应因子；血糖/肌酐摩尔浓度、常见质量浓度与 HbA1c IFCC/NGSP 有明确规则；未知项不伪造连续数值 |
| API Health 大载荷 | 32 MiB 单 payload、40 MiB 批次/500 条；SQLite 与 raw 同步保留完整 envelope | 两并发准入、CPU 隔离、取消/停服回收、临界值/超限原子拒绝和重放已验证；测量见[服务端审查](../reviews/2026-10-02-server-lifecycle-and-load.md) |
| HealthKit 数量/分类 | 目录含 121 数量、69 分类标识；每个可读 HKSample secure archive、原始数量表示/分类、精确时间、metadata | 按 OS 和实际 HKObjectType 动态可用，原有 29 个常用种类保留既有字段；目录计数不是设备实际授权或查询成功数 |
| HealthKit correlation / clinical | 2 组合、9 临床标识；样本 secure archive、关联 sample UUID、可用 FHIR 原始 bytes/base64 | 组合通过子类型授权，临床需 Health Records 支持与 entitlement；全部待 SDK/真机验证 |
| Workout、audiogram、vision prescription | 样本 secure archive；workout 另保留活动、duration、可用总量、事件 | App 代码已实现，Linux 仅语法解析 |
| ECG、heartbeat、workout route、quantity series | 专门查询逐个波形点、时间/间隙、CLLocation 和数量子样本；地点/数量也保留 secure archive | 单次序列累计上限 24 MiB；超限显式失败，不用总量替代或推进锚点；真机待测 |
| State of Mind / medication dose | 样本 secure archive | 按 iOS 18/26 可用性保护；需对应 SDK 与设备 |
| Characteristics | 六项公开特征：生物性别、血型、出生日期、皮肤类型、轮椅使用、活动目标模式 | 快照版本记录；DOB 保存原 NSDateComponents；缺失/隐藏不推断删除 |
| Activity Summary / CDA / annotated medication | 活动环逐日 secure archive、包含 documentData 的 CDA、iOS 26 标注药物 secure archive | 快照查询、独立待发送检查点；CDA 单次累计上限 24 MiB，完整历史规模与删除语义待测 |
| Apple Health 写回 | 用户显式保存已确认、精确数值、mg/dL 或 mmol/L 的 glucose | 用户核对完整采样时间，先保存 API 修订；HealthKit sync identifier/version 去重；SDK/真机未验证 |
| Health Connect | 服务器 envelope、来源、修订、删除、原件目录 raw/google_health | 合成归档/导出/删除测试通过；没有 Android App，未来客户端待实现 |
| Excel、WHOOP/Function 账户直连、其他写回映射 | 无当前实现 | 后续范围 |

完整原件路径和 SQLite 关系见 [架构](../architecture.md)；原件下载、完整回复及导出 v3 见 [API](../api.md)。所有原始载荷进入 API 主档案；“所有数据”的目标受 HealthKit 公开可读对象、OS、设备、权限和服务限额约束，不能声明已验证 Apple 内部数据库的完整镜像。

健康记录身份是 user + platform + source_id + record_id；同版本不同内容报冲突。Apple 删除未知来源用通配墓碑，清除对应内容/历史/原件，防重扫复活。快照缺失、授权撤销或无可见样本不作为全量删除依据。

服务器索引时间为 Unix 秒；样本 payload 保留原小数秒。平台原始对象通过 NSKeyedArchiver secure archive 保留，不只依赖 metadata 的显示文本；服务器将二进制视作不透明数据。原始 JSON 是客户端 envelope 的完整序列化，不是 Apple 官方 export.xml 或原始 HTTP 请求字节。

客户端锚点查询每页上限 10 个对象；拆为不超过 500 条/40 MiB 的请求，单条 payload 不超过 32 MiB。全部批次及候选锚点先落盘，全部回执后才推进；超限或错误保留重试状态。序列或整个快照超限可能持续阻塞该类型，需要后续流式附件/分页方案，不假装已全量成功。

日聚合 health-daily-v2 返回参与数、来源候选数、排除数及缺失/溢出状态，只支持 heart_rate/resting_heart_rate/hrv_sdnn/steps/sleep/workout，分别按来源显示，不将不同设备总量相加。其他原始类型仅归档。覆盖状态 observed/error/unsupported/no_visible_samples 与权限 unknown 分开；观察到的最早/最晚时间不证明中间完整。

软件验证包含原件/载荷重放、跨用户、通配删除、导出、失败 OCR 回复、数值换算和夏令时。真实全历史、权限撤销、手机断线重启、Watch 来源、极长序列、原生写回及真实报告漏项仍需外部验收。
