# 化验数值、单位与参考区间契约

版本：lab-units-v2。实现唯一入口是 src/backend_api/laboratory.rs；手机、OCR、分析和导出不各自实现另一套换算。模型负责保留印刷内容，人工确认 metric_id 后 API 生成可重算解释，原字段及每次修订不覆盖。

## 注册规则与依据

| 指标 / 输入 | 标准单位与规则 | 原始依据 |
| --- | --- | --- |
| 总胆固醇、LDL、HDL mmol/L | mg/dL = 输入 ÷ 0.02586 | [CDC/NHANES 血脂说明](https://wwwn.cdc.gov/nchs/data/nhanes/public/2017/datafiles/p_trigly.htm)；同一胆固醇分子的单位转换，不涉及计算 LDL 的临床方程 |
| 甘油三酯 mmol/L | mg/dL = 输入 ÷ 0.01129 | [CDC/NHANES](https://wwwn.cdc.gov/nchs/data/nhanes/public/2017/datafiles/p_trigly.htm) |
| 血糖 mmol/L | mg/dL = 输入 ÷ 0.05551 | [CDC/NHANES 血糖单位](https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2009/DataFiles/GLU_F.htm) |
| 肌酐 µmol/L | mg/dL = 输入 ÷ 88.4 | [WHO 转换手册](https://iris.who.int/bitstream/handle/10665/333647/TLS-NTP-manual-eng.pdf)，另用 [NCI/SEER 单位记录](https://staging.seer.cancer.gov/cs/input/02.05.50/liver/ssf5/?version=/tnm/home/1.2/) 独立核对 |
| HbA1c IFCC mmol/mol | NGSP % = 输入 × 0.09148 + 2.152 | [NGSP 官方主方程](https://ngsp.org/ifccngsp.asp)；参考边界也使用同一仿射转换 |
| ApoB g/L | mg/dL = 输入 × 100 | [NIST SI 前缀](https://www.nist.gov/pml/owm/metric-si-prefixes)，结合 L/dL 的量纲换算 |
| 血红蛋白 g/L | g/dL = 输入 ÷ 10 | 同上 |
| 铁蛋白 µg/L | ng/mL = 输入 | 同上 |

同指标的常见质量浓度也按 SI 换算；mg/dL 为标准的指标接受 mg/L、g/L、g/dL、µg/L、µg/mL。血红蛋白另接受 mg/dL、mg/L，铁蛋白另接受 µg/mL、mg/L、ng/L。摩尔浓度只在有指标专属因子的血脂、血糖、肌酐接受；ApoB 不凭通用摩尔质量猜算。所有 10 项可接受单位以 API metrics/list 的 accepted_units 为准。

别名仅为封闭集合：去空格，µ/μ/u 的已定义写法，常见小写 l 与全大写写法。不会普遍大小写折叠 SI 前缀，也不把 m9/dL 等 OCR 错字猜成 mg/dL。百分比属于已映射 HbA1c NGSP 语义，不将其他比值混入该指标。

## 数字与边界

- 使用 Decimal 检查运算，明确溢出；输入保留完整字符串，不依赖二进制浮点计算。
- 可解析非负精确小数及支持范围内的科学计数法。数字逗号、下划线、文字、复杂组合和超出表示范围的数字不猜测。
- <、>、≤、≥、<=、>= 产生带包含性的上下界；简单闭区间支持连字符、长短横及 to。比较符和区间不会成为精确趋势点。
- 同单位不减少已有小数精度；转换默认舍入最多 6 位小数，非零极小结果不舍入成零。原数字、规则及来源都保留，展示精度不是原检验的测量精度承诺。
- 参考区间尾部有可识别显式单位时独立换算；否则使用结果列单位且 unit_origin=observation。年龄/性别/方法等条件化范围保留原文与 unparsed_reference，不计算统一阈值。

解释含 version、result、reference；每个量含 status/reason、value 或上下界/包含性、original_unit/unit/unit_origin、display、rule/source。未知、缺单位、未映射及不可解析均有明确原因。report_flag 保持报告原标记，不由标准范围重新计算。

趋势只使用已确认、已映射、具采样日期且 exact 可转换的有效行。返回原字段、派生参考范围、规则版本及 excluded 原因；导出 v3 JSONL 保留事实，辅助 CSV 附这些派生解释。iOS 分别标注原单位参考和标准化参考，防止混读。

本文件描述数据换算规则，不赋予跨方法、检验环境或参考人群的临床可比性。真实报告的漏项/单位错配准确性仍需授权样本验收。
