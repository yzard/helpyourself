//! Deterministic, analyte-specific derivation. Printed values are never rewritten.
use crate::reports::ObservationPayload;
use rust_decimal::Decimal;
use serde::Serialize;
use std::str::FromStr;

pub const CONVERSION_VERSION: &str = "lab-units-v3";
pub const CANONICAL_UNITS: &[&str] = &[
    "mg/dL", "mg/L", "g/L", "g/dL", "mmol/L", "µmol/L", "ng/mL", "µg/L", "µg/mL", "ng/L", "%",
    "mmol/mol", "fL", "U/L", "10^9/L",
];

#[derive(Serialize)]
pub struct MetricDefinition {
    pub metric_id: &'static str,
    pub name: &'static str,
    pub standard_unit: &'static str,
    pub accepted_units: Vec<&'static str>,
    pub conversion_version: &'static str,
}
pub fn metrics() -> Vec<MetricDefinition> {
    [
        ("total_cholesterol", "Total cholesterol", "mg/dL"),
        ("ldl_cholesterol", "LDL cholesterol", "mg/dL"),
        ("hdl_cholesterol", "HDL cholesterol", "mg/dL"),
        ("triglycerides", "Triglycerides", "mg/dL"),
        ("apob", "Apolipoprotein B", "mg/dL"),
        ("hba1c", "HbA1c (NGSP)", "%"),
        ("glucose", "Glucose", "mg/dL"),
        ("hemoglobin", "Hemoglobin", "g/dL"),
        ("creatinine", "Creatinine", "mg/dL"),
        ("ferritin", "Ferritin", "ng/mL"),
        ("albumin", "Albumin", "g/dL"),
        ("crp", "C-reactive protein", "mg/dL"),
        ("lymphocyte_percent", "Lymphocytes", "%"),
        ("mcv", "Mean corpuscular volume", "fL"),
        ("rdw", "Red cell distribution width (CV)", "%"),
        ("alp", "Alkaline phosphatase", "U/L"),
        ("wbc", "White blood cell count", "10^9/L"),
    ]
    .into_iter()
    .map(|(metric_id, name, standard_unit)| MetricDefinition {
        metric_id,
        name,
        standard_unit,
        accepted_units: CANONICAL_UNITS
            .iter()
            .copied()
            .filter(|unit| conversion(metric_id, standard_unit, unit).is_some())
            .collect(),
        conversion_version: CONVERSION_VERSION,
    })
    .collect()
}

pub fn canonical_unit(raw: &str) -> Option<&'static str> {
    let compact: String = raw
        .chars()
        .filter(|character| !character.is_whitespace())
        .map(|character| {
            if character == 'μ' || character == 'µ' {
                'u'
            } else {
                character
            }
        })
        .collect();
    // Closed spelling aliases. Do not globally lowercase SI prefixes (M is not m).
    match compact.as_str() {
        "mg/dL" | "mg/dl" | "MG/DL" => Some("mg/dL"),
        "mg/L" | "mg/l" | "MG/L" => Some("mg/L"),
        "g/L" | "g/l" | "G/L" => Some("g/L"),
        "g/dL" | "g/dl" | "G/DL" => Some("g/dL"),
        "mmol/L" | "mmol/l" | "MMOL/L" => Some("mmol/L"),
        "umol/L" | "umol/l" | "UMOL/L" => Some("µmol/L"),
        "ng/mL" | "ng/ml" | "NG/ML" => Some("ng/mL"),
        "ug/L" | "ug/l" | "UG/L" => Some("µg/L"),
        "ug/mL" | "ug/ml" | "UG/ML" => Some("µg/mL"),
        "ng/L" | "ng/l" | "NG/L" => Some("ng/L"),
        "%" | "%NGSP" | "NGSP%" => Some("%"),
        "mmol/mol" | "MMOL/MOL" => Some("mmol/mol"),
        "fL" | "fl" => Some("fL"),
        "U/L" | "IU/L" => Some("U/L"),
        "10^9/L" | "10⁹/L" | "10^3/uL" | "10³/uL" => Some("10^9/L"),
        _ => None,
    }
}

struct Conversion {
    scale: Decimal,
    divisor: Decimal,
    offset: Decimal,
    rule: &'static str,
    source: &'static str,
}
fn conversion(metric: &str, standard: &str, unit: &str) -> Option<Conversion> {
    let mut rule = Conversion {
        scale: Decimal::ONE,
        divisor: Decimal::ONE,
        offset: Decimal::ZERO,
        rule: "identity",
        source: "https://www.nist.gov/pml/owm/metric-si-prefixes",
    };
    if standard == unit {
        return Some(rule);
    }
    if metric == "hba1c" {
        if unit != "mmol/mol" {
            return None;
        }
        rule.scale = Decimal::new(9148, 5);
        rule.offset = Decimal::new(2152, 3);
        rule.rule = "hba1c-ifcc-ngsp";
        rule.source = "https://ngsp.org/ifccngsp.asp";
        return Some(rule);
    }
    if standard == "mg/dL" {
        rule.scale = match unit {
            "mg/L" | "µg/mL" => Decimal::new(1, 1),
            "µg/L" => Decimal::new(1, 4),
            "g/L" => Decimal::new(100, 0),
            "g/dL" => Decimal::new(1000, 0),
            "mmol/L" | "µmol/L" => {
                rule.divisor = match metric {
                    "total_cholesterol" | "ldl_cholesterol" | "hdl_cholesterol" => {
                        rule.source = "https://wwwn.cdc.gov/nchs/data/nhanes/public/2017/datafiles/p_trigly.htm";
                        rule.rule = "cholesterol-molar";
                        Decimal::new(2586, 5)
                    }
                    "triglycerides" => {
                        rule.source = "https://wwwn.cdc.gov/nchs/data/nhanes/public/2017/datafiles/p_trigly.htm";
                        rule.rule = "triglycerides-molar";
                        Decimal::new(1129, 5)
                    }
                    "glucose" => {
                        rule.source =
                            "https://wwwn.cdc.gov/Nchs/Data/Nhanes/Public/2009/DataFiles/GLU_F.htm";
                        rule.rule = "glucose-molar";
                        Decimal::new(5551, 5)
                    }
                    "creatinine" => {
                        rule.source = "https://iris.who.int/bitstream/handle/10665/333647/TLS-NTP-manual-eng.pdf";
                        rule.rule = "creatinine-molar";
                        Decimal::new(884, 4)
                    }
                    _ => return None,
                };
                if unit == "µmol/L" {
                    rule.divisor = rule.divisor.checked_mul(Decimal::new(1000, 0))?;
                }
                Decimal::ONE
            }
            _ => return None,
        };
    } else if standard == "g/dL" {
        rule.scale = match unit {
            "g/L" => Decimal::new(1, 1),
            "mg/dL" => Decimal::new(1, 3),
            "mg/L" => Decimal::new(1, 4),
            _ => return None,
        };
    } else if standard == "ng/mL" {
        rule.scale = match unit {
            "µg/L" => Decimal::ONE,
            "µg/mL" | "mg/L" => Decimal::new(1000, 0),
            "ng/L" => Decimal::new(1, 3),
            _ => return None,
        };
    } else {
        return None;
    }
    if rule.rule == "identity" {
        rule.rule = "mass-volume-si";
    }
    Some(rule)
}
impl Conversion {
    fn apply(&self, value: Decimal) -> Option<String> {
        let value = value
            .checked_mul(self.scale)?
            .checked_div(self.divisor)?
            .checked_add(self.offset)?;
        let rounded = value.round_dp(6);
        let retained = if (self.scale == Decimal::ONE
            && self.divisor == Decimal::ONE
            && self.offset == Decimal::ZERO)
            || (value != Decimal::ZERO && rounded == Decimal::ZERO)
        {
            value
        } else {
            rounded
        };
        Some(retained.normalize().to_string())
    }
}

#[derive(Serialize)]
pub struct Quantity {
    pub status: &'static str,
    pub reason: &'static str,
    pub value: Option<String>,
    pub lower: Option<String>,
    pub upper: Option<String>,
    pub lower_inclusive: Option<bool>,
    pub upper_inclusive: Option<bool>,
    pub unit: Option<&'static str>,
    pub original_unit: Option<String>,
    pub unit_origin: &'static str,
    pub display: Option<String>,
    pub rule: Option<&'static str>,
    pub source: Option<&'static str>,
}
impl Quantity {
    fn unavailable(
        status: &'static str,
        original_unit: Option<&str>,
        origin: &'static str,
    ) -> Self {
        let reason = match status {
            "unmapped_metric" => {
                "Map the test to a known metric after checking the original report."
            }
            "missing_unit" => "No printed unit is available; do not infer one.",
            "unsupported_unit" => "This unit is not supported for the selected metric.",
            "unparsed_value" => {
                "Text, ambiguous numbers or complex ranges are retained without an exact numeric point."
            }
            "unparsed_reference" => {
                "The reference includes unsupported syntax or conditions; retain the original wording."
            }
            "missing_reference" => "No reference interval was printed.",
            "overflow" => "The numeric value exceeds the supported decimal range.",
            _ => "The original value cannot be compared.",
        };
        Self {
            status,
            reason,
            value: None,
            lower: None,
            upper: None,
            lower_inclusive: None,
            upper_inclusive: None,
            unit: None,
            original_unit: original_unit.map(str::to_owned),
            unit_origin: origin,
            display: None,
            rule: None,
            source: None,
        }
    }
}
#[derive(Serialize)]
pub struct Interpretation {
    pub version: &'static str,
    pub result: Quantity,
    pub reference: Quantity,
}

enum Shape {
    Exact(Decimal),
    Below(Decimal, bool),
    Above(Decimal, bool),
    Range(Decimal, Decimal),
}
fn decimal(text: &str) -> Option<Decimal> {
    let text = text.trim();
    if text.is_empty()
        || text.len() > 64
        || !text
            .chars()
            .all(|c| c.is_ascii_digit() || ".+-eE".contains(c))
        || !text.chars().any(|c| c.is_ascii_digit())
    {
        return None;
    }
    let value = if text.contains(['e', 'E']) {
        Decimal::from_scientific(text).ok()?
    } else {
        Decimal::from_str(text).ok()?
    };
    (value >= Decimal::ZERO).then_some(value)
}
fn shape(text: &str) -> Option<Shape> {
    let text = text.trim();
    for (prefix, above, inclusive) in [
        ("<=", false, true),
        (">=", true, true),
        ("≤", false, true),
        ("≥", true, true),
        ("<", false, false),
        (">", true, false),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return decimal(rest).map(|value| {
                if above {
                    Shape::Above(value, inclusive)
                } else {
                    Shape::Below(value, inclusive)
                }
            });
        }
    }
    if let Some(value) = decimal(text) {
        return Some(Shape::Exact(value));
    }
    for separator in ["–", "—", " to ", "-"] {
        if let Some((left, right)) = text.split_once(separator)
            && let (Some(left), Some(right)) = (decimal(left), decimal(right))
            && left <= right
        {
            return Some(Shape::Range(left, right));
        }
    }
    None
}
fn derive(
    metric_id: Option<&str>,
    raw: &str,
    raw_unit: Option<&str>,
    origin: &'static str,
    reference: bool,
) -> Quantity {
    let Some(metric) = metrics()
        .into_iter()
        .find(|metric| Some(metric.metric_id) == metric_id)
    else {
        return Quantity::unavailable("unmapped_metric", raw_unit, origin);
    };
    let Some(raw_unit) = raw_unit.filter(|unit| !unit.trim().is_empty()) else {
        return Quantity::unavailable("missing_unit", raw_unit, origin);
    };
    let Some(unit) = canonical_unit(raw_unit) else {
        return Quantity::unavailable("unsupported_unit", Some(raw_unit), origin);
    };
    let Some(rule) = conversion(metric.metric_id, metric.standard_unit, unit) else {
        return Quantity::unavailable("unsupported_unit", Some(raw_unit), origin);
    };
    let Some(parsed) = shape(raw) else {
        return Quantity::unavailable(
            if reference {
                "unparsed_reference"
            } else {
                "unparsed_value"
            },
            Some(raw_unit),
            origin,
        );
    };
    let mut quantity = Quantity::unavailable("overflow", Some(raw_unit), origin);
    quantity.unit = Some(metric.standard_unit);
    quantity.rule = Some(rule.rule);
    quantity.source = Some(rule.source);
    let display = match parsed {
        Shape::Exact(value) => {
            let Some(value) = rule.apply(value) else {
                return quantity;
            };
            quantity.status = "exact";
            quantity.value = Some(value.clone());
            value
        }
        Shape::Below(value, inclusive) => {
            let Some(value) = rule.apply(value) else {
                return quantity;
            };
            quantity.status = "comparison";
            quantity.upper = Some(value.clone());
            quantity.upper_inclusive = Some(inclusive);
            format!("{} {value}", if inclusive { "≤" } else { "<" })
        }
        Shape::Above(value, inclusive) => {
            let Some(value) = rule.apply(value) else {
                return quantity;
            };
            quantity.status = "comparison";
            quantity.lower = Some(value.clone());
            quantity.lower_inclusive = Some(inclusive);
            format!("{} {value}", if inclusive { "≥" } else { ">" })
        }
        Shape::Range(left, right) => {
            let (Some(left), Some(right)) = (rule.apply(left), rule.apply(right)) else {
                return quantity;
            };
            quantity.status = "range";
            quantity.lower = Some(left.clone());
            quantity.upper = Some(right.clone());
            quantity.lower_inclusive = Some(true);
            quantity.upper_inclusive = Some(true);
            format!("{left} – {right}")
        }
    };
    quantity.display = Some(format!("{display} {}", metric.standard_unit));
    quantity.reason = if quantity.status == "exact" {
        "Deterministic conversion; original value retained."
    } else {
        "Bounded value retained; it is not an exact numeric trend point."
    };
    quantity
}

pub fn interpret(payload: &ObservationPayload) -> Interpretation {
    let result = derive(
        payload.metric_id.as_deref(),
        &payload.raw_result,
        payload.raw_unit.as_deref(),
        "observation",
        false,
    );
    let reference = match payload
        .reference_range
        .as_deref()
        .filter(|text| !text.trim().is_empty())
    {
        None => Quantity::unavailable("missing_reference", None, "none"),
        Some(raw) => {
            let explicit = raw.char_indices().find_map(|(index, _)| {
                let (value, unit) = raw.split_at(index);
                (canonical_unit(unit).is_some() && shape(value).is_some()).then_some((value, unit))
            });
            if let Some((value, unit)) = explicit {
                derive(
                    payload.metric_id.as_deref(),
                    value,
                    Some(unit),
                    "reference_explicit",
                    true,
                )
            } else {
                derive(
                    payload.metric_id.as_deref(),
                    raw,
                    payload.raw_unit.as_deref(),
                    "observation",
                    true,
                )
            }
        }
    };
    Interpretation {
        version: CONVERSION_VERSION,
        result,
        reference,
    }
}

pub fn normalized(payload: &ObservationPayload) -> Option<(String, String)> {
    let interpreted = interpret(payload).result;
    if interpreted.status != "exact" {
        return None;
    }
    Some((interpreted.value?, interpreted.unit?.into()))
}
