//! Read-only, source-explicit views of the health archive.
pub mod algorithms;
pub mod associations;
pub mod clinical;
pub mod coach;
pub mod diet;
pub mod entries;
pub mod fit;
pub mod food;
pub mod food_lookup;
pub mod heart_rate;
pub mod hrv;
pub mod import;
pub mod materialized;
pub mod meal_glucose;
pub mod meals;
pub mod planning;
pub mod preferences;
pub mod records;
pub mod regularity;
pub mod reminders;
pub mod report;
pub mod review;
pub mod series;
pub mod sleep;
pub mod statistics;
pub mod timeline;
pub mod training;

use crate::{
    authentication::now,
    database::{Database, queries},
    error::AppError,
    health::AggregateRequest,
};
use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet};

pub const VERSION: &str = "wellness-day-v1";
pub const KINDS: &[&str] = &[
    "sleep",
    "workout",
    "steps",
    "resting_heart_rate",
    "heart_rate",
    "hrv_sdnn",
];

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DayRequest {
    pub date: String,
    pub timezone: String,
    pub source_priority: BTreeMap<String, Vec<String>>,
}

impl Database {
    pub async fn compute_wellness_day(
        &self,
        user: &str,
        request: DayRequest,
    ) -> Result<Value, AppError> {
        let date = NaiveDate::parse_from_str(&request.date, "%Y-%m-%d")
            .map_err(|_| AppError::Invalid("Invalid date"))?;
        if request.date != date.to_string() {
            return Err(AppError::Invalid("Use a date in YYYY-MM-DD format"));
        }
        validate_priorities(&request.source_priority)?;
        let lower = date
            .checked_sub_days(Days::new(42))
            .ok_or(AppError::Invalid("Invalid date range"))?;
        let revision: i64 = sqlx::query_scalar(queries::USER_DATA_REVISION)
            .bind(user)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)?;
        let mut metrics = Vec::new();
        for kind in KINDS {
            let aggregate = self
                .aggregate_health(
                    user,
                    AggregateRequest {
                        record_type: (*kind).into(),
                        start_date: lower.to_string(),
                        end_date: date.to_string(),
                        timezone: request.timezone.clone(),
                    },
                )
                .await;
            let aggregate = match aggregate {
                Ok(value) => value,
                Err(AppError::TooLarge) => {
                    metrics.push(json!({"record_type":kind,"current":{"date":request.date,"state":"query_limit_exceeded","selected":null,"alternatives":[]},"history":[],"baseline":null,"baseline_state":"insufficient_data","baseline_valid_days":0,"algorithm_version":crate::health::ALGORITHM_VERSION,"unit":null}));
                    continue;
                }
                Err(error) => return Err(error),
            };
            let priorities = request
                .source_priority
                .get(*kind)
                .cloned()
                .unwrap_or_default();
            metrics.push(select_metric(kind, &request.date, &priorities, &aggregate)?);
        }
        let coverage = self.health_coverage(user).await?;
        let current: i64 = sqlx::query_scalar(queries::USER_DATA_REVISION)
            .bind(user)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)?;
        if current != revision {
            return Err(AppError::Conflict("Archive changed; refresh the day view"));
        }
        Ok(
            json!({"date":request.date,"timezone":request.timezone,"data_revision":revision,
            "algorithm_version":VERSION,"computed_at":now()?,"metrics":metrics,"coverage":coverage["coverage"],
            "source_priority":request.source_priority,"source_policy":"explicit_priority_or_single_source_per_day","evidence_status":"descriptive",
            "notes":["Calendar-day sleep is not a main sleep session.","Missing records are not zero.","Different HRV protocols are not combined."]}),
        )
    }

    pub async fn wellness_sources(&self, user: &str) -> Result<Value, AppError> {
        let rows = sqlx::query(queries::WELLNESS_SOURCES)
            .bind(user)
            .fetch_all(&self.pool)
            .await?;
        let sources: Vec<Value> = rows.into_iter().map(|r| json!({
            "platform":r.get::<String,_>("platform"),"source_id":r.get::<String,_>("source_id"),
            "record_type":r.get::<String,_>("record_type"),"record_count":r.get::<i64,_>("record_count"),
            "first_at":r.get::<i64,_>("first_at"),"last_at":r.get::<i64,_>("last_at")
        })).collect();
        Ok(
            json!({"sources":sources,"supported_daily_types":KINDS,"coverage_scope":"visible_records_not_read_permission"}),
        )
    }
}

pub fn select_metric(
    kind: &str,
    date: &str,
    priority: &[String],
    aggregate: &Value,
) -> Result<Value, AppError> {
    let days = aggregate["days"].as_array().ok_or(AppError::Internal)?;
    let mut grouped: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for day in days {
        grouped
            .entry(day["date"].as_str().ok_or(AppError::Internal)?.into())
            .or_default()
            .push(day.clone());
    }
    let mut history = Vec::new();
    for (date, alternatives) in grouped {
        let valid: Vec<&Value> = alternatives
            .iter()
            .filter(|v| v["value"].as_f64().is_some())
            .collect();
        let preferred = priority.iter().find_map(|p| {
            valid
                .iter()
                .find(|v| v["source"].as_str() == Some(p))
                .copied()
        });
        // Conflicting sources need a user decision; no alphabetical winner or cross-source sum.
        let selected = preferred.or_else(|| {
            if priority.is_empty() && valid.len() == 1 {
                Some(valid[0])
            } else {
                None
            }
        });
        let state = if selected.is_some() {
            "ready"
        } else if valid.is_empty() {
            "insufficient_data"
        } else {
            "source_selection_required"
        };
        history.push(
            json!({"date":date,"state":state,"selected":selected,"alternatives":alternatives}),
        );
    }
    let current = history
        .iter()
        .find(|d| d["date"] == date)
        .cloned()
        .unwrap_or_else(
            || json!({"date":date,"state":"insufficient_data","selected":null,"alternatives":[]}),
        );
    // A personal comparison is valid only within the selected source and metric method.
    let source = current["selected"]["source"].as_str();
    let mut baseline: Vec<f64> = history
        .iter()
        .rev()
        .filter(|d| d["date"].as_str().is_some_and(|d| d < date))
        .filter(|d| source.is_some() && d["selected"]["source"].as_str() == source)
        .filter_map(|d| d["selected"]["value"].as_f64())
        .take(28)
        .collect();
    let valid_days = baseline.len();
    let baseline_result = if valid_days == 28 {
        let median = algorithms::median(&mut baseline)?;
        let mut deviations: Vec<f64> = baseline.iter().map(|v| (v - median).abs()).collect();
        let mad = algorithms::median(&mut deviations)?;
        Some(
            json!({"median":median,"mad":mad,"valid_days":valid_days,"window_days":42,"evidence_status":"descriptive"}),
        )
    } else {
        None
    };
    Ok(
        json!({"record_type":kind,"current":current,"history":history,"baseline":baseline_result,
        "baseline_state":if baseline_result.is_some() {"ready"} else {"calibrating"},"baseline_valid_days":valid_days,
        "algorithm_version":aggregate["algorithm_version"],"unit":current["selected"]["unit"]}),
    )
}

pub fn validate_priorities(priorities: &BTreeMap<String, Vec<String>>) -> Result<(), AppError> {
    if priorities.len() > KINDS.len()
        || priorities.iter().any(|(kind, sources)| {
            !KINDS.contains(&kind.as_str())
                || sources.len() > 32
                || sources.iter().any(|s| s.is_empty() || s.len() > 1024)
                || sources.iter().collect::<BTreeSet<_>>().len() != sources.len()
        })
    {
        return Err(AppError::Invalid("Invalid source priority"));
    }
    Ok(())
}
