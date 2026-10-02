# PDF、图片与单位处理核实

后续：F01/F02 已于 2026-10-02 整改并通过负载与生命周期验证，当前结论见[服务端补齐验收](2026-10-02-server-lifecycle-and-load.md)；下文保留发现当日的证据与状态。

历史记录：文档输入与单位差距已按 [补齐计划](../implementation-document-units.md)实施，当前结果及仍未解决的 skills 事项见 [补齐验收](2026-10-01-document-unit-completion.md)；以下保留发现时的证据。

日期：2026-10-01。基于当前代码和针对性测试核实实际行为，不将计划中的功能写成已实现；本轮未修改运行逻辑。

## 实际输入路径

| 输入 | 当前路径 | 缺口 |
| --- | --- | --- |
| PDF（文字、扫描、混合） | API 验证内容、加密状态和页数，原件进 raw/documents；每页 pdftoppm -scale-to 2400 -png，交给 OCR | 不检测/提取文本层，无文字与视觉交叉核对；小字/复杂版式受渲染分辨率及模型识别影响 |
| PNG/JPEG | 原件进 raw/photos，逐图交给 OCR | 推理副本做 EXIF 转正及像素预算缩小；不能保证细小文字不受影响 |
| HEIC/HEIF | 手机提供原件和 JPEG 识别副本，两者分别归档，识别 JPEG | 服务端尚不完整解码 HEIC；真实照片验收待完成 |

PDF 文字层仍在完整原件中，但没有可独立检索的文本层派生记录。docs/workflows.md 此前写“保留可用 PDF 文本作为证据”超出了实现，现已改为实际行为。

依据：src/backend_api/files.rs inspect_document；src/backend_api/worker.rs extract；src/backend_ocr/documents.py prepare_request。

## 原始值与转换的职责

OCR 提取 raw_name、raw_result、raw_unit、reference_range、report_flag、sampled_at、source，保留比较符/文字/原单位，禁止猜指标映射及医学解释。API 收到完整服务回复先存 extraction_outputs，候选为 pending/metric_id=null。用户复核并映射 metric_id 后，API 的 normalized 在趋势查询时产生派生 value/unit；不覆盖原始字段和审核修订。趋势同时返回 original 和 conversion_version=lipids-v1。

当前注册 10 项指标，标准单位为：总胆固醇、LDL、HDL、甘油三酯、ApoB、血糖、肌酐使用 mg/dL；HbA1c 使用 %；血红蛋白使用 g/dL；铁蛋白使用 ng/mL。

除标准单位原样接受，只有四项血脂支持 mmol/L 到 mg/dL：总胆固醇/LDL/HDL 除以 0.02586；甘油三酯除以 0.01129。Decimal 运算，最终保留至小数点后 6 位。针对性测试的 LDL 2.586 mmol/L 产生 100 mg/dL，raw_result/raw_unit 本身不改。

以下不会产生精确趋势点：未确认/未映射、缺采样日期、未知/缺失单位、带比较符或范围/文字结果、不能按 Decimal 解析的结果。已归档数据保留。incomparable_count 是已确认且匹配所选指标后缺日期/不可转换项的数量，不是所有未入趋势项目的总数。

## 已确认 gap

1. 文字型 PDF 路径缺失：没有逐页文本层提取、位置保存或与视觉结果核对；混合 PDF 也统一走图片。不能承诺文字型 PDF 的文字被无误利用。
2. 单位注册表不完整：血糖 mmol/L、肌酐 µmol/L、血红蛋白/ApoB g/L 等目前不会换算。单位只 trim 后精确匹配，MG/DL、mg / dL 等写法没有受控别名；原件/原文虽保留，趋势可能漏点。当前不做一般任意单位转换，不能将质量浓度与摩尔浓度统一乘同一常数。
3. 参考范围没有同步转换：只保留原始字符串，不生成标准化上下界及对应单位；不同报告的年龄/性别/方法条件也没有结构化。iOS 趋势显示标准化数值后，直接显示原 reference_range，没有在该行明确标注其原单位，可能被误当成标准单位范围。report_flag 是报告原标记，不是统一范围计算的异常判断。
4. 提取错误没有被单位转换兜底：OCR 把单位识别错/漏掉时，原值仍待人工核对；已被用户错误确认的“合法但错误”单位不能由当前规则自动识别。真实报告的数值/单位列错配准确性尚未测。
5. 另一个接口一致性问题：OCR Source.bounding_box 定义为字典，Rust Source.bounding_box 为 [left, top, right, bottom] 数组。当前测试和提示主要使用 null，非 null 坐标有协议失败风险，应与字段完整性整改一起修复。

## 需要补齐的行为

- API 在保留原件及视觉页的同时，逐页提取文本/位置/状态；有可用文字层时交给结构化识别作证据，扫描/混合页仍使用视觉，禁止仅凭有少量文本就跳过整页视觉。
- API 建立按指标和单位语义的受控转换注册表与别名，不依赖模型换算；原始值和派生值分存，明确转换版本、精度和不可比较原因。
- 参考区间保留原字符串，同时在可可靠解析时产生带原单位/标准单位的边界；UI 明确显示两者。范围无法解析时不能绘制虚构标准范围。
- 补充文字/扫描/混合 PDF、多种单位同指标、比较符/文本值、范围、别名和非 null 坐标的端到端覆盖；使用授权真实报告验证漏项与单位错配。

## 本轮证据

针对性 Rust 测试通过：reports::unit_conversion_never_rewrites_comparators_or_unknown_units；worker::pdf_rendering_and_independent_ocr_service_create_reviewable_rows。后者验证真实 PDF 渲染和模拟 OCR 协议，不证明模型能无误读取真实 PDF。此前真实 GPU 合成页面只覆盖 LDL 120 mg/dL，不能作为其他单位或文字层读取的证据。
