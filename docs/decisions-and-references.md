# 决定记录与参考资料

记录日期：2026-09-06；范围更新：2026-10-01。平台能力会变化，实施各适配器时需复核当前官方文档和真机行为。

## 已批准决定与演变

| 编号 | 最终决定 | 替代的早期想法 / 理由 |
| --- | --- | --- |
| D01 | helpyourself 保存完整档案 | 不再以 Apple Health 可写指标作为血检范围 |
| D02 | 服务器必需，手机缓存 | 替代最初设备为主档案；支持未来跨平台 |
| D03 | 第一版数据库多用户、管理员建号 | 替代单用户建议；不开放注册 |
| D04 | 服务器级 TOML 配置 | 不支持用户逐人覆盖 provider |
| D05 | Rust backend_api + 独立 backend_ocr（Qwen3.8 / NInfer） | 2026-10-01 用户明确要求 OCR 作为独立服务；分析保持独立配置接口 |
| D06 | 内部 HTTP，Caddy TLS，Compose | 替代私有网络/VPN 必需方案 |
| D07 | 原始明细优先、服务端派生 | 不以手机日汇总取代原始数据 |
| D08 | 暂不做备份功能，停服复制 data | 保留数据导出；在线一致性备份不在首版 |
| D09 | iPhone、PDF/图像、完整可读原始档案；已确认血糖可显式写回 | Android、Excel、其他写回映射后续实现 |
| D11 | 2026-10-01：schema 直接重构，无 migration | 用户确认 playground 为空，无需保留升级兼容 |
| D12 | API raw 按 apple_health/photos/google_health/documents 分类 | HealthKit 数据、OCR 和人工添加内容全部进入服务器档案 |
| D10 | 先数据基础，再个人风险发现 | AI 核心价值保留，首场景是血脂纵向变化 |

## 平台与模型事实

Apple 的 HKClinicalRecord 临床记录不能由第三方创建或保存，因此不能承诺把任意 OCR 化验结果写成原生临床记录。[Apple：Accessing Health Records](https://developer.apple.com/documentation/healthkit/accessing-health-records)

HealthKit 有血糖等量化类型；当前目录没有给出 LDL、HbA1c 等常见血检的独立量化类型。CDA 文档保存是另一条路径，不能推导文档内容会被自动拆成可画趋势的原生指标。当前只提供用户核对时间后保存已确认血糖的入口，其他映射仍待定义和验证。[量化类型](https://developer.apple.com/documentation/healthkit/hkquantitytypeidentifier)、[CDA 文档](https://developer.apple.com/documentation/healthkit/hkcdadocumentsample)

HealthKit 同步需按类型选择查询；增量查询与访问授权分别参考官方 API。读权限状态不能简单视作明确的可读/拒绝标志，设计保留未知与无样本的区别。背景调度和各类型删除行为需真机验证。[增量查询](https://developer.apple.com/documentation/healthkit/hkanchoredobjectquery)、[授权访问](https://developer.apple.com/documentation/healthkit/authorizing-access-to-health-data)

Android 对应 Health Connect，已有实验性 Medical Records 能力，不能沿用“只能交换少数数值”的旧假设。未来实施时核实设备、权限、FHIR 资源和发布限制。[Medical Records](https://developer.android.com/health-and-fitness/health-connect/medical-records)、[医学数据写入](https://developer.android.com/health-and-fitness/health-connect/medical-records/write-data)

当前用户指定 Qwen3.8 + NInfer。固定 NInfer 源码提交及 Qwen3.8-27B NVFP4 模型修订、SHA256，镜像在构建时下载模型。NInfer 的原生视觉服务运行在 OCR 容器回环地址；当前实现针对 RTX 5090。固定版本与验收记录见 [OCR 运维](runbooks/ocr.md)及[本轮审查](reviews/2026-10-02-server-lifecycle-and-load.md)。[NInfer 官方仓库](https://github.com/Neroued/ninfer)、[官方 NInfer 模型](https://huggingface.co/neroued/Qwen3.8-27B-nvfp4-NInfer)

SQLite 提供在线备份机制；本项目首版选择停止写入后复制目录，不能把随意复制活动数据库当作可靠在线备份。需要在线备份时另立工作包。[SQLite Backup API](https://www.sqlite.org/backup.html)

## momento 参考范围

已只读查看本机 `/home/zyin/dev/momento` 的配置加载约定。可复用的是分节 TOML、显式配置路径、强类型校验、data_dir。其模型运行配置不等于通用 provider 地址配置，不照搬模型路径和镜像管理。

本计划未复制 momento 的密钥或真实运行配置；不引入两个项目的代码依赖。helpyourself 的相对路径按配置文件目录解析，是本次独立设计。

## 待验证而非待重复批准

| 项目 | 在何时解决 | 失败时的处理 |
| --- | --- | --- |
| Xcode、签名、最低 iOS、用户真机 | P00 | 明确设备验证阻塞，基础后端可继续 |
| Qwen3.8/NInfer 的真实血检准确性与资源需求 | P00/P04 | 保留失败回复，调整显式部署与提示；不偷偷发往其他服务 |
| 真实样本数量与人工基准 | P00/P11 | 用合成夹具建设，真实准确性不标通过 |
| HealthKit 首轮类型与明细限制 | P00/P07 | 支持矩阵显式列出缺口，后续补齐 |
| 规模与查询延迟 | P08 | 测量后优化索引或可失效缓存 |
| 医学资料与评审人 | P12/P13 | 不发布缺依据假设，不跳过公众发布关口 |
| Android 的当前能力 | P14 | 按实际能力实现，不承诺与 iOS 对称 |

以上不阻止进入已批准范围内的设计与实现；遇到影响产品边界的新事实，再回到用户确认变更。

2026-10-01 文档补齐：PDF 文字/视觉双证据、指标专属单位注册、数值与参考边界语义及官方换算依据见 [化验单位契约](laboratory-units.md)，验收见 [最新审查](reviews/2026-10-02-server-lifecycle-and-load.md)。
