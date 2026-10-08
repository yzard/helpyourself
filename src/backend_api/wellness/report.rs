//! Source-separated calendar reports. These summaries do not estimate health outcomes.
use crate::{authentication::now, database::Database, error::AppError, health::AggregateRequest};
use chrono::{Days, NaiveDate, TimeZone};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub start_date: String,
    pub end_date: String,
    pub timezone: String,
}

pub fn summarize_days(days: &[Value], calendar_days: usize) -> Result<Vec<Value>, AppError> {
    let mut groups: BTreeMap<String, BTreeMap<String, &Value>> = BTreeMap::new();
    for day in days {
        let source = day["source"].as_str().ok_or(AppError::Internal)?;
        let date = day["date"].as_str().ok_or(AppError::Internal)?;
        if groups
            .entry(source.into())
            .or_default()
            .insert(date.into(), day)
            .is_some()
        {
            return Err(AppError::Conflict("Duplicate daily source results"));
        }
    }
    let mut result = Vec::new();
    for (source, days) in groups {
        let records: Vec<_> = days.into_values().collect();
        let mut values: Vec<f64> = records
            .iter()
            .filter_map(|v| v["value"].as_f64())
            .filter(|v| v.is_finite())
            .collect();
        values.sort_by(f64::total_cmp);
        let middle = values.len() / 2;
        let median = if values.is_empty() {
            None
        } else if values.len().is_multiple_of(2) {
            Some((values[middle - 1] + values[middle]) / 2.0)
        } else {
            Some(values[middle])
        };
        let valid = values.len();
        result.push(json!({"source":source,"observed_days":valid,"missing_days":calendar_days.saturating_sub(valid),"daily_median":median,"daily_minimum":values.first(),"daily_maximum":values.last(),"days":records}));
    }
    Ok(result)
}
impl Database {
    pub async fn wellness_report(&self, user: &str, request: Request) -> Result<Value, AppError> {
        let parse = |s: &str| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()
                .filter(|d| d.to_string() == s)
                .ok_or(AppError::Invalid("Use dates in YYYY-MM-DD format"))
        };
        let start = parse(&request.start_date)?;
        let end = parse(&request.end_date)?;
        if end < start || (end - start).num_days() > 365 {
            return Err(AppError::Invalid("Select up to 366 calendar days"));
        }
        let tz: chrono_tz::Tz = request
            .timezone
            .parse()
            .map_err(|_| AppError::Invalid("Unknown timezone"))?;
        let revision = self.data_revision(user).await?;
        let calendar_days = (end - start).num_days() as usize + 1;
        let mut metrics = Vec::new();
        for kind in super::KINDS {
            let data = self
                .aggregate_health(
                    user,
                    AggregateRequest {
                        record_type: (*kind).into(),
                        start_date: request.start_date.clone(),
                        end_date: request.end_date.clone(),
                        timezone: request.timezone.clone(),
                    },
                )
                .await;
            match data {
                Ok(data) => metrics.push(json!({"record_type":kind,"state":"available","algorithm_version":data["algorithm_version"],"sources":summarize_days(data["days"].as_array().ok_or(AppError::Internal)?,calendar_days)?})),
                Err(AppError::TooLarge) => metrics.push(json!({"record_type":kind,"state":"query_limit_exceeded","sources":[]})),
                Err(error) => return Err(error),
            }
        }
        let midnight = |date: NaiveDate| {
            tz.from_local_datetime(&date.and_hms_opt(0, 0, 0).ok_or(AppError::Internal)?)
                .earliest()
                .map(|v| v.timestamp())
                .ok_or(AppError::Invalid("No midnight on this date"))
        };
        let mut entries = Vec::new();
        let mut cursor = start;
        let exclusive = end
            .succ_opt()
            .ok_or(AppError::Invalid("Invalid report end"))?;
        while cursor < exclusive {
            let next = cursor
                .checked_add_days(Days::new(89))
                .ok_or(AppError::Invalid("Invalid report date"))?
                .min(exclusive);
            let from = midnight(cursor)?;
            let to = midnight(next)?;
            let data = self
                .list_wellness_entries(
                    user,
                    super::entries::ListRequest {
                        start_at: from,
                        end_at: to,
                        kind: None,
                    },
                )
                .await?;
            entries.extend(
                data["entries"]
                    .as_array()
                    .ok_or(AppError::Internal)?
                    .iter()
                    .filter(|v| {
                        matches!(
                            v["entry"]["kind"].as_str(),
                            Some(
                                "body"
                                    | "blood_pressure"
                                    | "cycle"
                                    | "training"
                                    | "nutrition"
                                    | "journal"
                            )
                        )
                    })
                    .cloned(),
            );
            cursor = next;
        }
        if self.data_revision(user).await? != revision {
            return Err(AppError::Conflict("Archive changed; refresh the report"));
        }
        Ok(
            json!({"start_date":request.start_date,"end_date":request.end_date,"timezone":request.timezone,"calendar_days":calendar_days,"metrics":metrics,"manual_records":entries,"data_revision":revision,"computed_at":now()?,"algorithm_version":"calendar-report-v1","evidence_status":"descriptive","notes":["Daily summaries remain separated by source. Missing days do not count as zero.","A recorded day does not establish complete sensor coverage. Compare the input days and units.","Manual records retain exact values and revisions. This report does not predict disease, fertility, injury, or lifespan."]}),
        )
    }
}
