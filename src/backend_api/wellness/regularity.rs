//! SRI uses only explicit sleep or wake states on a fixed one-minute UTC grid.
use super::sleep::SleepRequest;
use crate::{
    authentication::now,
    database::{Database, queries},
    error::AppError,
    health::StoredHealthRecord,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
impl Database {
    pub async fn sleep_regularity_view(
        &self,
        user: &str,
        request: SleepRequest,
    ) -> Result<Value, AppError> {
        if i128::from(request.end_at) - i128::from(request.start_at) != 7 * 86400
            || request.start_at % 60 != 0
            || chrono::DateTime::from_timestamp(request.start_at, 0).is_none()
            || chrono::DateTime::from_timestamp(request.end_at, 0).is_none()
            || request.timezone.parse::<chrono_tz::Tz>().is_err()
        {
            return Err(AppError::Invalid(
                "SRI needs exactly seven 24-hour days starting on a minute boundary and a valid timezone",
            ));
        }
        let revision = self.data_revision(user).await?;
        let rows: Vec<StoredHealthRecord> = sqlx::query_as(queries::HEALTH_RANGE)
            .bind(user)
            .bind("sleep")
            .bind(request.start_at)
            .bind(request.end_at)
            .fetch_all(&self.pool)
            .await?;
        if rows.len() > 50000 {
            return Err(AppError::TooLarge);
        }
        let mut result = self
            .cpu
            .run(move || summarize_regularity(rows, request.start_at, request.end_at))
            .await?;
        if self.data_revision(user).await? != revision {
            return Err(AppError::Conflict(
                "Archive changed; refresh sleep regularity",
            ));
        }
        result["data_revision"] = json!(revision);
        result["computed_at"] = json!(now()?);
        Ok(result)
    }
}
pub fn summarize_regularity(
    records: Vec<StoredHealthRecord>,
    start: i64,
    end: i64,
) -> Result<Value, AppError> {
    if i128::from(end) - i128::from(start) != 604800 || start % 60 != 0 {
        return Err(AppError::Invalid("SRI requires a seven-day minute grid"));
    }
    let mut groups: BTreeMap<String, Vec<StoredHealthRecord>> = BTreeMap::new();
    for record in records {
        if record.end_at < record.start_at {
            return Err(AppError::Invalid("Invalid sleep interval"));
        }
        groups
            .entry(format!("{}:{}", record.platform, record.source_id))
            .or_default()
            .push(record);
    }
    if groups.len() > 64 {
        return Err(AppError::TooLarge);
    }
    let mut sources = Vec::new();
    for (source, records) in groups {
        let (mut asleep, mut awake, mut inputs) = (Vec::new(), Vec::new(), Vec::new());
        for record in records {
            let raw: Value = serde_json::from_str(&record.payload_json)?;
            let category = raw["payload"]["category"].as_i64();
            let span = (record.start_at.max(start), record.end_at.min(end));
            if matches!(category, Some(1 | 3 | 4 | 5)) {
                asleep.push(span);
            } else if category == Some(2) {
                awake.push(span);
            }
            inputs.push(json!({"record_id":record.record_id,"version":record.version,"start_at":record.start_at,"end_at":record.end_at,"category":category}));
        }
        let asleep = super::sleep::merge(asleep);
        let awake = super::sleep::merge(awake);
        let (mut sleep_index, mut wake_index, mut conflicts, mut sleep_minutes, mut wake_minutes) =
            (0, 0, 0, 0, 0);
        let mut states = Vec::with_capacity(10080);
        for minute in 0..10080 {
            let at = start + minute * 60;
            let sleeping = contains(&asleep, at, &mut sleep_index);
            let waking = contains(&awake, at, &mut wake_index);
            states.push(match (sleeping, waking) {
                (true, false) => {
                    sleep_minutes += 1;
                    Some(true)
                }
                (false, true) => {
                    wake_minutes += 1;
                    Some(false)
                }
                (true, true) => {
                    conflicts += 1;
                    None
                }
                _ => None,
            });
        }
        let known = sleep_minutes + wake_minutes;
        let state = if conflicts > 0 {
            "conflicting_states"
        } else if known < 10080 {
            "insufficient_coverage"
        } else if sleep_minutes == 0 || wake_minutes == 0 {
            "missing_sleep_or_wake_variation"
        } else {
            "ready"
        };
        let value = if state == "ready" {
            Some(super::algorithms::sleep_regularity(&states)?)
        } else {
            None
        };
        sources.push(json!({"source":source,"state":state,"value":value,"unit":"SRI","known_minutes":known,"total_minutes":10080,"conflict_minutes":conflicts,"sleep_minutes":sleep_minutes,"wake_minutes":wake_minutes,"inputs":inputs}));
    }
    Ok(
        json!({"sources":sources,"start_at":start,"end_at":end,"algorithm_version":"sri-minute-grid-v1","evidence_status":"conditional","parameters":{"sample_interval_seconds":60,"comparison_lag_seconds":86400,"state_sampling":"explicit_interval_at_each_minute_start","require_known_minutes":10080},"notes":["Missing intervals are unknown, not awake. In-bed records do not establish either state.","States are sampled on a fixed UTC minute grid, with a 24-hour comparison. This is not a local-clock-day calculation across daylight-saving changes.","An index describes regularity, not sleep sufficiency or recovery. It does not predict an individual health outcome."]}),
    )
}
fn contains(spans: &[(i64, i64)], at: i64, index: &mut usize) -> bool {
    while *index < spans.len() && spans[*index].1 <= at {
        *index += 1;
    }
    spans
        .get(*index)
        .is_some_and(|(start, end)| *start <= at && at < *end)
}
