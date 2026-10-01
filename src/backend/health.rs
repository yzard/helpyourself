use crate::{
    authentication::{digest, now},
    database::{Database, queries},
    error::AppError,
};
use chrono::{NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::{BTreeMap, HashSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectionRequest {
    pub platform: String,
    pub installation_id: String,
}
#[derive(Serialize, sqlx::FromRow)]
pub struct Connection {
    pub connection_id: String,
    pub platform: String,
    pub installation_id: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HealthRecordInput {
    pub record_id: String,
    pub source_id: String,
    pub record_type: String,
    pub start_at: i64,
    pub end_at: i64,
    pub version: i64,
    pub deleted: bool,
    pub payload: Value,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncRequest {
    pub connection_id: String,
    pub batch_id: String,
    pub record_type: String,
    pub coverage_status: String,
    pub records: Vec<HealthRecordInput>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AggregateRequest {
    pub record_type: String,
    pub start_date: String,
    pub end_date: String,
    pub timezone: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct StoredHealthRecord {
    pub platform: String,
    pub source_id: String,
    pub record_id: String,
    pub record_type: String,
    pub start_at: i64,
    pub end_at: i64,
    pub version: i64,
    #[serde(skip_serializing)]
    pub payload_json: String,
}

impl Database {
    pub async fn create_connection(
        &self,
        user_id: &str,
        request: ConnectionRequest,
    ) -> Result<Connection, AppError> {
        if !["apple_health", "health_connect"].contains(&request.platform.as_str())
            || uuid::Uuid::parse_str(&request.installation_id).is_err()
        {
            return Err(AppError::Invalid("Invalid platform or installation ID"));
        }
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let inserted = sqlx::query(queries::CREATE_CONNECTION)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(user_id)
            .bind(&request.platform)
            .bind(&request.installation_id)
            .bind(now()?)
            .execute(&mut *transaction)
            .await?;
        if inserted.rows_affected() > 0 {
            sqlx::query(queries::INVALIDATE_EXPORTS)
                .bind(user_id)
                .execute(&mut *transaction)
                .await?;
        }
        let connection = sqlx::query_as(queries::CONNECTION_BY_INSTALLATION)
            .bind(user_id)
            .bind(request.platform)
            .bind(request.installation_id)
            .fetch_one(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(connection)
    }
    pub async fn sync_health(
        &self,
        user_id: &str,
        request: SyncRequest,
    ) -> Result<Value, AppError> {
        if uuid::Uuid::parse_str(&request.batch_id).is_err()
            || request.records.len() > 500
            || request.record_type.is_empty()
            || request.record_type.len() > 128
            || !["observed", "no_visible_samples", "error", "unsupported"]
                .contains(&request.coverage_status.as_str())
        {
            return Err(AppError::Invalid("Invalid sync batch"));
        }
        let encoded = serde_json::to_vec(&request)?;
        if encoded.len() > 40 * 1024 * 1024 {
            return Err(AppError::TooLarge);
        }
        let batch_digest = digest(&encoded);
        let mut seen = HashSet::new();
        for record in &request.records {
            if record.record_id.is_empty()
                || record.record_id.len() > 256
                || record.source_id.is_empty()
                || record.source_id.len() > 512
                || (!record.deleted && record.source_id == "*")
                || record.record_type != request.record_type
                || record.version < 1
                || record.end_at < record.start_at
                || record.payload.to_string().len() > 32 * 1024 * 1024
                || !record.payload.is_object()
                || !seen.insert((&record.source_id, &record.record_id))
            {
                return Err(AppError::Invalid("Invalid health record"));
            }
        }
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let platform: String = sqlx::query_scalar(queries::CONNECTION)
            .bind(user_id)
            .bind(&request.connection_id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(AppError::NotFound)?;
        let existing: Option<String> = sqlx::query_scalar(queries::SYNC_BATCH)
            .bind(user_id)
            .bind(&request.connection_id)
            .bind(&request.batch_id)
            .fetch_optional(&mut *transaction)
            .await?;
        if let Some(existing) = existing {
            if existing != batch_digest {
                return Err(AppError::Conflict("Batch ID reused with different content"));
            }
            return Ok(json!({"batch_id":request.batch_id,"replayed":true}));
        }
        let mut changed = false;
        for record in &request.records {
            if !record.deleted {
                let deleted: i64 = sqlx::query_scalar(queries::HEALTH_TOMBSTONE)
                    .bind(user_id)
                    .bind(&platform)
                    .bind(&record.record_id)
                    .bind(&record.source_id)
                    .fetch_one(&mut *transaction)
                    .await?;
                if deleted > 0 {
                    continue;
                }
            }
            if record.deleted {
                let paths: Vec<String> = sqlx::query_scalar(queries::HEALTH_RAW_TO_DELETE)
                    .bind(user_id)
                    .bind(&platform)
                    .bind(&record.record_id)
                    .bind(&record.source_id)
                    .bind(&record.source_id)
                    .fetch_all(&mut *transaction)
                    .await?;
                for path in paths {
                    sqlx::query(queries::INSERT_CLEANUP)
                        .bind(uuid::Uuid::new_v4().to_string())
                        .bind(path)
                        .bind(now()?)
                        .execute(&mut *transaction)
                        .await?;
                }
            }
            if record.deleted && record.source_id == "*" {
                sqlx::query(queries::DELETE_HEALTH_ORIGINS)
                    .bind(user_id)
                    .bind(&platform)
                    .bind(&record.record_id)
                    .execute(&mut *transaction)
                    .await?;
                sqlx::query(queries::PURGE_HEALTH_ORIGINS)
                    .bind(user_id)
                    .bind(&platform)
                    .bind(&record.record_id)
                    .execute(&mut *transaction)
                    .await?;
                changed = true;
            }
            let stored = sqlx::query(queries::HEALTH_RECORD)
                .bind(user_id)
                .bind(&platform)
                .bind(&record.source_id)
                .bind(&record.record_id)
                .fetch_optional(&mut *transaction)
                .await?;
            let payload = if record.deleted {
                "{}".to_owned()
            } else {
                serde_json::to_string(record)?
            };
            if let Some(stored) = stored {
                if stored.get::<i64, _>("deleted") == 1 {
                    continue;
                }
                let version = stored.get::<i64, _>("version");
                if !record.deleted && record.version < version {
                    continue;
                }
                if !record.deleted && record.version == version {
                    if stored.get::<String, _>("payload_json") != payload {
                        return Err(AppError::Conflict(
                            "Health record changed without a new source version",
                        ));
                    }
                    continue;
                }
            }
            sqlx::query(queries::UPSERT_HEALTH)
                .bind(user_id)
                .bind(&platform)
                .bind(&record.source_id)
                .bind(&record.record_id)
                .bind(&record.record_type)
                .bind(if record.deleted { 0 } else { record.start_at })
                .bind(if record.deleted { 0 } else { record.end_at })
                .bind(record.version)
                .bind(i64::from(record.deleted))
                .bind(&payload)
                .execute(&mut *transaction)
                .await?;
            if record.deleted {
                sqlx::query(queries::PURGE_HEALTH_REVISIONS)
                    .bind(user_id)
                    .bind(&platform)
                    .bind(&record.source_id)
                    .bind(&record.record_id)
                    .execute(&mut *transaction)
                    .await?;
            } else {
                let revision_id = uuid::Uuid::new_v4().to_string();
                let raw_path = crate::raw::health_path(user_id, &platform, &revision_id)?;
                crate::raw::archive(&self.data_dir, &raw_path, payload.as_bytes()).await?;
                sqlx::query(queries::INSERT_HEALTH_REVISION)
                    .bind(user_id)
                    .bind(&platform)
                    .bind(&record.source_id)
                    .bind(&record.record_id)
                    .bind(revision_id)
                    .bind(raw_path)
                    .bind(payload)
                    .bind(now()?)
                    .execute(&mut *transaction)
                    .await?;
            }
            changed = true;
        }
        sqlx::query(queries::INSERT_SYNC_BATCH)
            .bind(user_id)
            .bind(&request.connection_id)
            .bind(&request.batch_id)
            .bind(batch_digest)
            .bind(now()?)
            .execute(&mut *transaction)
            .await?;
        let successful = ["observed", "no_visible_samples"]
            .contains(&request.coverage_status.as_str())
            .then_some(now()?);
        sqlx::query(queries::UPSERT_COVERAGE)
            .bind(user_id)
            .bind(&request.connection_id)
            .bind(&request.record_type)
            .bind(request.coverage_status)
            .bind(now()?)
            .bind(successful)
            .bind(successful)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(queries::INVALIDATE_EXPORTS)
            .bind(user_id)
            .execute(&mut *transaction)
            .await?;
        if changed {
            if request.records.iter().any(|record| record.deleted) {
                sqlx::query(queries::PURGE_FEEDBACK)
                    .bind(user_id)
                    .execute(&mut *transaction)
                    .await?;
            }
            sqlx::query(queries::TOUCH_USER_DATA)
                .bind(user_id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query(queries::INVALIDATE_ANALYSES)
                .bind(user_id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query(queries::INVALIDATE_EXPORTS)
                .bind(user_id)
                .execute(&mut *transaction)
                .await?;
        }
        transaction.commit().await?;
        Ok(json!({"batch_id":request.batch_id,"replayed":false}))
    }
    pub async fn health_coverage(&self, user_id: &str) -> Result<Value, AppError> {
        let rows = sqlx::query(queries::COVERAGE)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        Ok(
            json!({"coverage":rows.into_iter().map(|row|json!({"connection_id":row.get::<String,_>("connection_id"),"platform":row.get::<String,_>("platform"),
            "installation_id":row.get::<String,_>("installation_id"),"record_type":row.get::<Option<String>,_>("record_type"),"status":row.get::<Option<String>,_>("status"),
            "last_sync_at":row.get::<Option<i64>,_>("last_sync_at"),"first_success_at":row.get::<Option<i64>,_>("first_success_at"),"last_success_at":row.get::<Option<i64>,_>("last_success_at"),"visible_start_at":row.get::<Option<i64>,_>("visible_start_at"),"visible_end_at":row.get::<Option<i64>,_>("visible_end_at"),"visible_range_scope":"platform_and_type"})).collect::<Vec<_>>(),"read_permission":"unknown"}),
        )
    }
    pub async fn health_records(
        &self,
        user_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Value, AppError> {
        if !(1..=500).contains(&limit) || offset < 0 {
            return Err(AppError::Invalid("Invalid health page"));
        }
        let mut rows = sqlx::query_as::<_, StoredHealthRecord>(queries::HEALTH_LIST)
            .bind(user_id)
            .bind(limit)
            .bind(offset)
            .fetch(&self.pool);
        let mut records = Vec::new();
        let mut byte_count = 0;
        while let Some(record) = rows.try_next().await? {
            byte_count += record.payload_json.len();
            if byte_count > 40 * 1024 * 1024 {
                return Err(AppError::TooLarge);
            }
            records.push(json!({"platform": record.platform,"record":serde_json::from_str::<Value>(&record.payload_json)?}));
        }
        Ok(json!({"records":records}))
    }
    pub async fn health_raw_files(
        &self,
        user_id: &str,
        request: &crate::models::ListRequest,
    ) -> Result<Value, AppError> {
        request.validate()?;
        let rows = sqlx::query(queries::HEALTH_RAW_LIST)
            .bind(user_id)
            .bind(&request.after_id)
            .bind(&request.after_id)
            .bind(request.limit)
            .fetch_all(&self.pool)
            .await?;
        let files = rows
            .into_iter()
            .map(|row| {
                json!({
                    "raw_id": row.get::<String, _>("revision_id"),
                    "platform": row.get::<String, _>("platform"),
                    "source_id": row.get::<String, _>("source_id"),
                    "record_id": row.get::<String, _>("record_id"),
                    "relative_path": row.get::<String, _>("raw_path"),
                    "received_at": row.get::<i64, _>("received_at")
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({"files": files}))
    }
    pub async fn health_raw_path(&self, user_id: &str, raw_id: &str) -> Result<String, AppError> {
        sqlx::query_scalar(queries::HEALTH_RAW_GET)
            .bind(user_id)
            .bind(raw_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)
    }
    pub async fn aggregate_health(
        &self,
        user_id: &str,
        request: AggregateRequest,
    ) -> Result<Value, AppError> {
        if ![
            "heart_rate",
            "resting_heart_rate",
            "hrv_sdnn",
            "steps",
            "sleep",
            "workout",
        ]
        .contains(&request.record_type.as_str())
        {
            return Err(AppError::Invalid(
                "This record type has no aggregation algorithm",
            ));
        }
        let timezone: Tz = request
            .timezone
            .parse()
            .map_err(|_| AppError::Invalid("Unknown IANA timezone"))?;
        let start = NaiveDate::parse_from_str(&request.start_date, "%Y-%m-%d")
            .map_err(|_| AppError::Invalid("Invalid start date"))?;
        let end = NaiveDate::parse_from_str(&request.end_date, "%Y-%m-%d")
            .map_err(|_| AppError::Invalid("Invalid end date"))?;
        if end < start || (end - start).num_days() > 366 {
            return Err(AppError::Invalid("Select at most 367 days"));
        }
        let exclusive = end.succ_opt().ok_or(AppError::Invalid("Date overflow"))?;
        let start_at = day_boundary(timezone, start)?;
        let end_at = day_boundary(timezone, exclusive)?;
        let records = sqlx::query_as::<_, StoredHealthRecord>(queries::HEALTH_RANGE)
            .bind(user_id)
            .bind(&request.record_type)
            .bind(start_at)
            .bind(end_at)
            .fetch_all(&self.pool)
            .await?;
        if records.len() > 50000 {
            return Err(AppError::TooLarge);
        }
        aggregate_records(records, &request.record_type, start, end, timezone)
    }
}

fn day_boundary(timezone: Tz, date: NaiveDate) -> Result<i64, AppError> {
    timezone
        .from_local_datetime(
            &date
                .and_hms_opt(0, 0, 0)
                .ok_or(AppError::Invalid("Invalid day"))?,
        )
        .earliest()
        .map(|date| date.timestamp())
        .ok_or(AppError::Invalid(
            "Midnight does not exist in selected timezone",
        ))
}

pub fn aggregate_records(
    records: Vec<StoredHealthRecord>,
    kind: &str,
    start: NaiveDate,
    end: NaiveDate,
    timezone: Tz,
) -> Result<Value, AppError> {
    if ![
        "heart_rate",
        "resting_heart_rate",
        "hrv_sdnn",
        "steps",
        "sleep",
        "workout",
    ]
    .contains(&kind)
    {
        return Err(AppError::Invalid(
            "No aggregate algorithm for this type; raw records remain available",
        ));
    }
    let mut days = Vec::new();
    let mut date = start;
    while date <= end {
        let next = date.succ_opt().ok_or(AppError::Invalid("Date overflow"))?;
        let lower = day_boundary(timezone, date)?;
        let upper = day_boundary(timezone, next)?;
        let mut sources: BTreeMap<String, Vec<(&StoredHealthRecord, Value)>> = BTreeMap::new();
        for record in &records {
            if record.end_at < lower || record.start_at >= upper {
                continue;
            }
            let envelope: Value = serde_json::from_str(&record.payload_json)?;
            sources
                .entry(format!("{}:{}", record.platform, record.source_id))
                .or_default()
                .push((
                    record,
                    envelope.get("payload").ok_or(AppError::Internal)?.clone(),
                ));
        }
        for (source, entries) in sources {
            let mut numbers = Vec::new();
            let mut intervals = Vec::new();
            let mut unit: Option<String> = None;
            for (record, payload) in &entries {
                if kind == "sleep" || kind == "workout" {
                    if kind == "sleep"
                        && !payload
                            .get("category")
                            .and_then(Value::as_i64)
                            .is_some_and(|category| [1, 3, 4, 5].contains(&category))
                    {
                        continue;
                    }
                    let from = record.start_at.max(lower);
                    let to = record.end_at.min(upper);
                    if to > from {
                        intervals.push((from, to));
                    }
                    continue;
                }
                let Some(number) = payload
                    .get("value")
                    .and_then(Value::as_f64)
                    .filter(|number| number.is_finite() && *number >= 0.0)
                else {
                    continue;
                };
                let Some(record_unit) = payload.get("unit").and_then(Value::as_str) else {
                    continue;
                };
                let expected = match kind {
                    "steps" => "count",
                    "hrv_sdnn" => "ms",
                    _ => "count/min",
                };
                if record_unit != expected {
                    continue;
                }
                unit = Some(record_unit.to_owned());
                if kind == "steps" && record.end_at > record.start_at {
                    let overlap = (record.end_at.min(upper) - record.start_at.max(lower)).max(0);
                    if overlap > 0 {
                        numbers.push(
                            number * overlap as f64 / (record.end_at - record.start_at) as f64,
                        );
                    }
                } else if record.start_at >= lower && record.start_at < upper {
                    numbers.push(number);
                }
            }
            let (result, algorithm) = if kind == "sleep" || kind == "workout" {
                intervals.sort_unstable();
                let mut merged: Vec<(i64, i64)> = Vec::new();
                for interval in intervals {
                    if let Some(last) = merged.last_mut().filter(|last| interval.0 <= last.1) {
                        last.1 = last.1.max(interval.1);
                    } else {
                        merged.push(interval);
                    }
                }
                unit = Some("seconds".into());
                (
                    if merged.is_empty() {
                        None
                    } else {
                        Some(
                            merged
                                .iter()
                                .map(|(from, to)| (to - from) as f64)
                                .sum::<f64>(),
                        )
                    },
                    "interval_union",
                )
            } else if numbers.is_empty() {
                (None, "missing")
            } else if kind == "steps" {
                (Some(numbers.iter().sum::<f64>()), "interval_prorated_sum")
            } else {
                (
                    Some(numbers.iter().sum::<f64>() / numbers.len() as f64),
                    "sample_mean",
                )
            };
            days.push(json!({"date":date.to_string(),"source":source,"value":result,"unit":unit,"sample_count":entries.len(),"algorithm":algorithm}));
        }
        date = next;
    }
    Ok(
        json!({"days":days,"timezone":timezone.name(),"algorithm_version":"health-daily-v1","source_policy":"separate_sources_no_cross_source_sum","computed_at":Utc::now().timestamp()}),
    )
}
