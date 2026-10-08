//! Bounded source-separated views of existing device measurements.
use crate::{
    authentication::now,
    database::{Database, queries},
    error::AppError,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeriesRequest {
    pub record_type: String,
    pub start_at: i64,
    pub end_at: i64,
    pub maximum_gap_seconds: i64,
    pub declared_maximum: Option<super::heart_rate::DeclaredMaximum>,
}
impl Database {
    pub async fn wellness_series(
        &self,
        user: &str,
        request: SeriesRequest,
    ) -> Result<Value, AppError> {
        if let Some(maximum) = &request.declared_maximum {
            maximum.validate()?;
        }
        let unit = match request.record_type.as_str() {
            "heart_rate" => "count/min",
            "blood_glucose" => "mg/dL",
            "vo2_max" => "ml/kg*min",
            "body_mass" => "kg",
            "body_fat" => "%",
            "oxygen_saturation" => "%",
            "respiratory_rate" => "count/min",
            _ => return Err(AppError::Invalid("Unsupported series type")),
        };
        if request.end_at <= request.start_at
            || i128::from(request.end_at) - i128::from(request.start_at) > 90 * 86400
            || !(1..=1800).contains(&request.maximum_gap_seconds)
            || chrono::DateTime::from_timestamp(request.start_at, 0).is_none()
            || chrono::DateTime::from_timestamp(request.end_at, 0).is_none()
        {
            return Err(AppError::Invalid(
                "Select a valid window of at most 90 days and a gap from 1 to 1800 seconds",
            ));
        }
        let revision = self.data_revision(user).await?;
        let rows = sqlx::query(queries::WELLNESS_SERIES)
            .bind(user)
            .bind(&request.record_type)
            .bind(request.start_at)
            .bind(request.end_at)
            .fetch_all(&self.pool)
            .await?;
        if rows.len() > 50000 {
            return Err(AppError::TooLarge);
        }
        let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for row in rows {
            let raw_unit: Option<String> = row.try_get("unit")?;
            let raw_value: Option<f64> = row.try_get("value")?;
            let value = convert(
                &row.get::<String, _>("platform"),
                &request.record_type,
                raw_unit.as_deref(),
                raw_value,
            );
            groups.entry(format!("{}:{}",row.get::<String,_>("platform"),row.get::<String,_>("source_id"))).or_default().push(json!({"at":row.get::<i64,_>("start_at"),"record_id":row.get::<String,_>("record_id"),"version":row.get::<i64,_>("version"),"value":value,"raw_value":raw_value,"raw_unit":raw_unit}));
        }
        let mut sources = Vec::new();
        for (source, points) in groups {
            let invalid = points.iter().filter(|p| p["value"].is_null()).count();
            let mut duplicate = false;
            let mut samples = Vec::new();
            for point in &points {
                if let (Some(at), Some(value)) = (point["at"].as_i64(), point["value"].as_f64()) {
                    if samples.last().is_some_and(|(last, _)| *last == at) {
                        duplicate = true;
                    }
                    samples.push((at, value));
                }
            }
            let state = if invalid > 0 {
                "unsupported_measurements"
            } else if duplicate {
                "duplicate_timestamps"
            } else {
                "observed"
            };
            let glucose = if request.record_type == "blood_glucose" && invalid == 0 && !duplicate {
                Some(super::algorithms::glucose(
                    &samples,
                    request.start_at,
                    request.end_at,
                    request.maximum_gap_seconds,
                )?)
            } else {
                None
            };
            let heart_rate = if request.record_type == "heart_rate" && invalid == 0 && !duplicate {
                Some(super::heart_rate::summarize(
                    &samples,
                    request.start_at,
                    request.end_at,
                    request.maximum_gap_seconds,
                    request.declared_maximum.as_ref(),
                )?)
            } else {
                None
            };
            sources.push(json!({"source":source,"points":points,"state":state,"invalid_count":invalid,"glucose_summary":glucose,"heart_rate_summary":heart_rate}));
        }
        if revision != self.data_revision(user).await? {
            return Err(AppError::Conflict("Archive changed; refresh the series"));
        }
        Ok(
            json!({"record_type":request.record_type,"unit":unit,"start_at":request.start_at,"end_at":request.end_at,"sources":sources,"data_revision":revision,"computed_at":now()?,"algorithm_version":"device-series-v1","evidence_status":"descriptive","parameters":{"maximum_gap_seconds":request.maximum_gap_seconds,"interpolation":"left_hold_between_samples_within_gap","glucose_range_mg_dl":[70,180]},"notes":["Sources are not combined. Invalid units and duplicate timestamps stop glucose summaries.","Coverage describes recorded intervals. It does not establish that the source is a continuous glucose monitor.","The last sample has no inferred duration. Sparse laboratory and finger-stick values do not describe a full day.","Glucose ranges describe these records and are not personal treatment targets."]}),
        )
    }
}
fn convert(platform: &str, kind: &str, unit: Option<&str>, value: Option<f64>) -> Option<f64> {
    let value = value.filter(|v| v.is_finite())?;
    let (value, valid) = match (kind, unit?) {
        ("heart_rate", "count/min" | "bpm") => (value, value > 0.0 && value <= 300.0),
        ("blood_glucose", "mg/dL") => (value, value > 0.0 && value <= 2000.0),
        ("blood_glucose", "mmol/L") => {
            (value * 18.01559, value > 0.0 && value * 18.01559 <= 2000.0)
        }
        ("vo2_max", "ml/kg*min" | "mL/kg/min") => (value, value > 0.0 && value <= 200.0),
        ("body_mass", "kg") => (value, value > 0.0 && value <= 1000.0),
        ("body_fat" | "oxygen_saturation", "%") if platform == "apple_health" => {
            (value * 100.0, (0.0..=1.0).contains(&value))
        }
        ("body_fat" | "oxygen_saturation", "%") => (value, (0.0..=100.0).contains(&value)),
        ("respiratory_rate", "count/min") => (value, value > 0.0 && value <= 200.0),
        _ => return None,
    };
    valid.then_some(value)
}
