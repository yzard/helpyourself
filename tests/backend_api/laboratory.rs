use crate::observation_payload;
use helpyourself::laboratory::{canonical_unit, interpret, metrics, normalized};

#[test]
fn analyte_specific_conversions_and_aliases_retain_printed_payload() {
    for (metric, value, unit, expected, standard) in [
        ("ldl_cholesterol", "2.586", "mmol/L", "100", "mg/dL"),
        ("triglycerides", "1.129", "mmol / L", "100", "mg/dL"),
        ("glucose", "5.551", "MMOL/L", "100", "mg/dL"),
        ("creatinine", "88.4", "μmol / l", "1", "mg/dL"),
        ("hemoglobin", "150", "g/L", "15", "g/dL"),
        ("apob", "0.9", "g/L", "90", "mg/dL"),
        ("ferritin", "35", "µg/L", "35", "ng/mL"),
        ("hba1c", "42", "mmol/mol", "5.99416", "%"),
        ("glucose", "100", "MG / DL", "100", "mg/dL"),
    ] {
        let mut input = observation_payload();
        input.metric_id = Some(metric.into());
        input.raw_result = value.into();
        input.raw_unit = Some(unit.into());
        let before = serde_json::to_value(&input).unwrap();
        assert_eq!(
            normalized(&input),
            Some((expected.into(), standard.into())),
            "{metric} {unit}"
        );
        assert_eq!(serde_json::to_value(input).unwrap(), before);
    }
    assert_eq!(canonical_unit("Mmol/L"), None);
    assert_eq!(canonical_unit("m9/dL"), None);
    for metric in metrics() {
        assert!(metric.accepted_units.contains(&metric.standard_unit));
    }
}

#[test]
fn bounds_are_converted_without_becoming_exact_trend_values() {
    let mut input = observation_payload();
    input.metric_id = Some("glucose".into());
    input.raw_unit = Some("mmol/L".into());
    for (printed, lower, upper, inclusive) in [
        ("<5.551", None, Some("100"), false),
        ("≤5.551", None, Some("100"), true),
        (">=5.551", Some("100"), None, true),
        ("5.551–11.102", Some("100"), Some("200"), true),
    ] {
        input.raw_result = printed.into();
        let derived = interpret(&input).result;
        assert_eq!(derived.lower.as_deref(), lower);
        assert_eq!(derived.upper.as_deref(), upper);
        if lower.is_some() {
            assert_eq!(derived.lower_inclusive, Some(inclusive));
        } else {
            assert_eq!(derived.upper_inclusive, Some(inclusive));
        }
        assert!(normalized(&input).is_none());
    }
}

#[test]
fn reference_units_and_hba1c_affine_conversion_do_not_invent_context() {
    let mut input = observation_payload();
    input.metric_id = Some("glucose".into());
    input.raw_unit = Some("mg/dL".into());
    input.reference_range = Some("5.551 - 11.102 mmol/L".into());
    let reference = interpret(&input).reference;
    assert_eq!(reference.lower.as_deref(), Some("100"));
    assert_eq!(reference.upper.as_deref(), Some("200"));
    assert_eq!(reference.unit_origin, "reference_explicit");
    input.reference_range = Some("Male: <100; Female: <90 mg/dL".into());
    assert_eq!(interpret(&input).reference.status, "unparsed_reference");
    input.metric_id = Some("hba1c".into());
    input.raw_unit = Some("mmol/mol".into());
    input.reference_range = Some("<42".into());
    assert_eq!(
        interpret(&input).reference.upper.as_deref(),
        Some("5.99416")
    );
    assert_eq!(interpret(&input).reference.rule, Some("hba1c-ifcc-ngsp"));
}

#[test]
fn unknown_ambiguous_overflow_and_unmapped_values_remain_explicit() {
    let mut input = observation_payload();
    for text in [
        "1,234",
        "positive",
        "-5",
        "1.2.3",
        "<5 or >10",
        "NaN",
        "1_000",
        "0.1%",
        "1e-99",
    ] {
        input.raw_result = text.into();
        assert!(normalized(&input).is_none(), "{text}");
    }
    input.raw_result = "79228162514264337593543950335".into();
    input.raw_unit = Some("g/L".into());
    assert_eq!(interpret(&input).result.status, "overflow");
    input.raw_result = "120".into();
    input.raw_unit = Some("bananas".into());
    assert_eq!(interpret(&input).result.status, "unsupported_unit");
    input.raw_unit = None;
    assert_eq!(interpret(&input).result.status, "missing_unit");
    input.metric_id = None;
    assert_eq!(interpret(&input).result.status, "unmapped_metric");
    input.metric_id = Some("apob".into());
    input.raw_unit = Some("mmol/L".into());
    assert_eq!(interpret(&input).result.status, "unsupported_unit");
}

#[test]
fn small_values_and_identity_precision_are_not_rounded_into_zero() {
    let mut input = observation_payload();
    input.raw_result = "0.0000001".into();
    assert_eq!(normalized(&input).unwrap().0, "0.0000001");
    input.raw_unit = Some("mmol/L".into());
    assert_ne!(normalized(&input).unwrap().0, "0");
}
