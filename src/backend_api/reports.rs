use crate::{
    authentication::now,
    database::{Database, queries},
    error::AppError,
    models::ListRequest,
};
use chrono::{DateTime, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::HashSet;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub page: i64,
    pub quote: String,
    pub bounding_box: Option<[f64; 4]>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservationPayload {
    pub raw_name: String,
    pub raw_result: String,
    pub raw_unit: Option<String>,
    pub reference_range: Option<String>,
    pub report_flag: Option<String>,
    pub sampled_at: Option<String>,
    pub metric_id: Option<String>,
    pub source: Source,
    pub notes: Option<String>,
}

impl ObservationPayload {
    pub fn validate(&self, pages: i64) -> Result<(), AppError> {
        if self.raw_name.trim().is_empty()
            || self.raw_name.len() > 256
            || self.raw_result.trim().is_empty()
            || self.raw_result.len() > 4096
            || self.source.page < 1
            || self.source.page > pages
            || self.source.quote.len() > 8192
        {
            return Err(AppError::Invalid(
                "Invalid observation name, result or source page",
            ));
        }
        for field in [
            &self.raw_unit,
            &self.reference_range,
            &self.report_flag,
            &self.notes,
        ] {
            if field.as_ref().is_some_and(|text| text.len() > 4096) {
                return Err(AppError::Invalid("Observation field too long"));
            }
        }
        if let Some(date) = &self.sampled_at {
            if date.len() > 64 {
                return Err(AppError::Invalid("Collection date exceeds limit"));
            }
            parse_time(date)?;
        }
        if let Some(metric) = &self.metric_id
            && !crate::laboratory::metrics()
                .iter()
                .any(|definition| definition.metric_id == metric)
        {
            return Err(AppError::Invalid(
                "Unknown metric_id; leave unmapped instead",
            ));
        }
        if let Some(bounds) = self.source.bounding_box
            && (bounds
                .iter()
                .any(|coordinate| !coordinate.is_finite() || !(0.0..=1.0).contains(coordinate))
                || bounds[0] >= bounds[2]
                || bounds[1] >= bounds[3])
        {
            return Err(AppError::Invalid(
                "Bounding box must use normalized left, top, right, bottom",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportContext {
    pub fasting: Option<bool>,
    pub recent_exercise: Option<String>,
    pub illness: Option<String>,
    pub medications: Option<String>,
    pub notes: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ReportSummary {
    pub report_id: String,
    pub revision: i64,
    #[serde(skip_serializing)]
    pub context_json: String,
    pub original_name: String,
    pub page_count: i64,
    pub created_at: i64,
}

#[derive(Serialize)]
pub struct Observation {
    pub observation_id: String,
    pub report_id: String,
    pub revision: i64,
    pub status: String,
    pub payload: ObservationPayload,
    pub interpretation: crate::laboratory::Interpretation,
}

#[derive(sqlx::FromRow)]
struct StoredObservation {
    observation_id: String,
    report_id: String,
    revision: i64,
    status: String,
    payload_json: String,
}
impl StoredObservation {
    fn decode(self) -> Result<Observation, AppError> {
        let payload: ObservationPayload = serde_json::from_str(&self.payload_json)?;
        let interpretation = crate::laboratory::interpret(&payload);
        Ok(Observation {
            observation_id: self.observation_id,
            report_id: self.report_id,
            revision: self.revision,
            status: self.status,
            payload,
            interpretation,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewItem {
    pub observation_id: Option<String>,
    pub expected_revision: Option<i64>,
    pub status: String,
    pub payload: ObservationPayload,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRequest {
    pub report_id: String,
    pub expected_revision: i64,
    pub context: Option<ReportContext>,
    pub observations: Vec<ReviewItem>,
}

pub fn parse_time(input: &str) -> Result<i64, AppError> {
    if input.len() == 10 {
        return NaiveDate::parse_from_str(input, "%Y-%m-%d")
            .ok()
            .and_then(|date| date.and_hms_opt(0, 0, 0))
            .map(|date| date.and_utc().timestamp())
            .ok_or(AppError::Invalid("Date must be YYYY-MM-DD or RFC3339"));
    }
    DateTime::parse_from_rfc3339(input)
        .map(|date| date.timestamp())
        .map_err(|_| AppError::Invalid("Date must be YYYY-MM-DD or RFC3339"))
}

impl Database {
    pub async fn report_summary(
        &self,
        user_id: &str,
        report_id: &str,
    ) -> Result<ReportSummary, AppError> {
        sqlx::query_as(queries::REPORT)
            .bind(user_id)
            .bind(report_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)
    }
    pub async fn reports(
        &self,
        user_id: &str,
        request: &ListRequest,
    ) -> Result<Vec<ReportSummary>, AppError> {
        request.validate()?;
        Ok(sqlx::query_as(queries::REPORTS)
            .bind(user_id)
            .bind(&request.after_id)
            .bind(&request.after_id)
            .bind(request.limit)
            .fetch_all(&self.pool)
            .await?)
    }
    pub async fn observations(
        &self,
        user_id: &str,
        report_id: &str,
    ) -> Result<Vec<Observation>, AppError> {
        self.report_summary(user_id, report_id).await?;
        let rows = sqlx::query_as::<_, StoredObservation>(queries::OBSERVATIONS)
            .bind(user_id)
            .bind(report_id)
            .fetch_all(&self.pool)
            .await?;
        decode_observations(&self.cpu, rows).await
    }
    pub async fn report(&self, user_id: &str, report_id: &str) -> Result<Value, AppError> {
        let mut transaction = self.pool.begin().await?;
        let report: ReportSummary = sqlx::query_as(queries::REPORT)
            .bind(user_id)
            .bind(report_id)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(AppError::NotFound)?;
        let context: Value = serde_json::from_str(&report.context_json)?;
        let observations = sqlx::query_as::<_, StoredObservation>(queries::OBSERVATIONS)
            .bind(user_id)
            .bind(report_id)
            .fetch_all(&mut *transaction)
            .await?;
        let observations = decode_observations(&self.cpu, observations).await?;
        let relation=sqlx::query(queries::REPORT_RELATION).bind(user_id).bind(report_id).fetch_optional(&mut *transaction).await?
            .map(|row| json!({"preferred_report_id":row.get::<String,_>("preferred_report_id"),"kind":row.get::<String,_>("kind")}));
        let pages=sqlx::query(queries::EXTRACTION_PAGES).bind(user_id).bind(report_id).fetch_all(&mut *transaction).await?.into_iter()
            .map(|row|json!({"run_id":row.get::<String,_>("run_id"),"page":row.get::<i64,_>("page_number"),"status":row.get::<String,_>("status"),
                "content":row.get::<String,_>("content"),"model":row.get::<String,_>("model"),"created_at":row.get::<i64,_>("created_at")})).collect::<Vec<_>>();
        let outputs = sqlx::query(queries::EXTRACTION_OUTPUTS).bind(user_id).bind(report_id)
            .fetch_all(&mut *transaction).await?.into_iter().map(|row| json!({
                "run_id":row.get::<String,_>("run_id"), "page":row.get::<i64,_>("page_number"),
                "stage":row.get::<String,_>("stage"), "model":row.get::<String,_>("model"),
                "adapter":row.get::<String,_>("adapter"), "created_at":row.get::<i64,_>("created_at")
            })).collect::<Vec<_>>();
        let inputs = sqlx::query(queries::EXTRACTION_INPUTS).bind(user_id).bind(report_id).fetch_all(&mut *transaction).await?
            .into_iter().map(|row| json!({"run_id":row.get::<String,_>("run_id"),"page":row.get::<i64,_>("page_number"),"created_at":row.get::<i64,_>("created_at"),"text_status":row.get::<Option<String>,_>("text_status"),"error_code":row.get::<Option<String>,_>("error_code")})).collect::<Vec<_>>();
        let duplicates = sqlx::query(queries::DUPLICATE_FILES).bind(user_id).bind(user_id).bind(report_id).bind(report_id).fetch_all(&mut *transaction).await?.into_iter().map(|row|json!({"report_id":row.get::<String,_>("file_id"),"original_name":row.get::<String,_>("original_name"),"reason":"identical_file_bytes"})).collect::<Vec<_>>();
        transaction.commit().await?;
        Ok(
            json!({"duplicate_candidates":duplicates,"report":report,"context":context,"observations":observations,"relation":relation,"pages":pages,"extraction_outputs":outputs,"extraction_inputs":inputs}),
        )
    }
    pub async fn extraction_input(
        &self,
        user_id: &str,
        request: ExtractionInputRequest,
    ) -> Result<Value, AppError> {
        let body: String = sqlx::query_scalar(queries::EXTRACTION_INPUT)
            .bind(user_id)
            .bind(request.report_id)
            .bind(request.run_id)
            .bind(request.page)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)?;
        self.cpu.run(move || Ok(serde_json::from_str(&body)?)).await
    }
    pub async fn extraction_output(
        &self,
        user_id: &str,
        request: ExtractionOutputRequest,
    ) -> Result<Value, AppError> {
        let row = sqlx::query(queries::EXTRACTION_OUTPUT)
            .bind(user_id)
            .bind(request.report_id)
            .bind(request.run_id)
            .bind(request.page)
            .bind(request.stage)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)?;
        Ok(
            json!({"response_body":row.get::<String,_>("response_body"), "status_code":row.get::<i64,_>("status_code"), "content":row.get::<Option<String>,_>("content")}),
        )
    }
    pub async fn review(&self, user_id: &str, request: ReviewRequest) -> Result<Value, AppError> {
        let report = self.report_summary(user_id, &request.report_id).await?;
        if request.observations.len() > 2000 {
            return Err(AppError::TooLarge);
        }
        let mut seen = HashSet::new();
        for observation in &request.observations {
            observation.payload.validate(report.page_count)?;
            if !["pending", "confirmed", "rejected"].contains(&observation.status.as_str()) {
                return Err(AppError::Invalid("Invalid review status"));
            }
            if let Some(id) = &observation.observation_id {
                if !seen.insert(id) {
                    return Err(AppError::Invalid("Duplicate observation in review"));
                }
                if observation.expected_revision.is_none() {
                    return Err(AppError::Invalid("Observation revision required"));
                }
            } else if observation.expected_revision.is_some() {
                return Err(AppError::Invalid("New observation cannot have a revision"));
            }
        }
        let context = request
            .context
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        if context.as_ref().is_some_and(|text| text.len() > 16384) {
            return Err(AppError::TooLarge);
        }
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let updated = sqlx::query(queries::UPDATE_REPORT)
            .bind(context)
            .bind(user_id)
            .bind(&request.report_id)
            .bind(request.expected_revision)
            .execute(&mut *transaction)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::Conflict(
                "Report changed; reload before reviewing",
            ));
        }
        for observation in request.observations {
            let (id, revision) = if let Some(id) = observation.observation_id {
                let current: Option<i64> = sqlx::query_scalar(queries::OBSERVATION)
                    .bind(user_id)
                    .bind(&request.report_id)
                    .bind(&id)
                    .fetch_optional(&mut *transaction)
                    .await?;
                let current = current.ok_or(AppError::NotFound)?;
                if Some(current) != observation.expected_revision {
                    return Err(AppError::Conflict("Observation changed"));
                }
                sqlx::query(queries::UPDATE_OBSERVATION)
                    .bind(current + 1)
                    .bind(user_id)
                    .bind(&id)
                    .execute(&mut *transaction)
                    .await?;
                (id, current + 1)
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                sqlx::query(queries::INSERT_OBSERVATION)
                    .bind(&id)
                    .bind(user_id)
                    .bind(&request.report_id)
                    .bind(Option::<String>::None)
                    .execute(&mut *transaction)
                    .await?;
                (id, 1)
            };
            sqlx::query(queries::INSERT_OBSERVATION_REVISION)
                .bind(user_id)
                .bind(id)
                .bind(revision)
                .bind(observation.status)
                .bind(serde_json::to_string(&observation.payload)?)
                .bind(now()?)
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
        transaction.commit().await?;
        self.report(user_id, &request.report_id).await
    }
    pub async fn observation_history(
        &self,
        user_id: &str,
        observation_id: &str,
    ) -> Result<Value, AppError> {
        let rows = sqlx::query(queries::OBSERVATION_HISTORY)
            .bind(user_id)
            .bind(observation_id)
            .fetch_all(&self.pool)
            .await?;
        if rows.is_empty() {
            return Err(AppError::NotFound);
        }
        let history=rows.into_iter().map(|row| Ok(json!({"revision":row.get::<i64,_>("revision"),"status":row.get::<String,_>("status"),
            "payload":serde_json::from_str::<Value>(&row.get::<String,_>("payload_json"))?,"created_at":row.get::<i64,_>("created_at")})))
            .collect::<Result<Vec<_>,AppError>>()?;
        Ok(json!({"history":history}))
    }
    pub async fn confirmed(&self, user_id: &str) -> Result<Vec<Observation>, AppError> {
        let rows = sqlx::query_as::<_, StoredObservation>(queries::CONFIRMED_OBSERVATIONS)
            .bind(user_id)
            .fetch_all(&self.pool)
            .await?;
        if rows.len() > 10000 {
            return Err(AppError::TooLarge);
        }
        decode_observations(&self.cpu, rows).await
    }
    pub async fn relate_reports(
        &self,
        user_id: &str,
        report_id: &str,
        preferred: Option<&str>,
        kind: &str,
        revision: i64,
    ) -> Result<(), AppError> {
        self.report_summary(user_id, report_id).await?;
        if let Some(preferred) = preferred {
            if preferred == report_id || !["duplicate", "superseded"].contains(&kind) {
                return Err(AppError::Invalid("Invalid report relationship"));
            }
            self.report_summary(user_id, preferred).await?;
        }
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let updated = sqlx::query(queries::UPDATE_REPORT)
            .bind(Option::<String>::None)
            .bind(user_id)
            .bind(report_id)
            .bind(revision)
            .execute(&mut *transaction)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::Conflict("Report changed"));
        }
        if let Some(preferred) = preferred {
            let depends: i64 = sqlx::query_scalar(queries::RELATION_DEPENDENTS)
                .bind(user_id)
                .bind(report_id)
                .fetch_one(&mut *transaction)
                .await?;
            let excluded = sqlx::query(queries::REPORT_RELATION)
                .bind(user_id)
                .bind(preferred)
                .fetch_optional(&mut *transaction)
                .await?;
            if depends > 0 || excluded.is_some() {
                return Err(AppError::Conflict(
                    "Choose a primary report; chained relations are not allowed",
                ));
            }
            sqlx::query(queries::SET_RELATION)
                .bind(user_id)
                .bind(report_id)
                .bind(preferred)
                .bind(kind)
                .execute(&mut *transaction)
                .await?;
        } else {
            sqlx::query(queries::CLEAR_RELATION)
                .bind(user_id)
                .bind(report_id)
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
        transaction.commit().await?;
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrendRequest {
    pub metric_ids: Vec<String>,
    pub start_at: Option<String>,
    pub end_at: Option<String>,
}
pub async fn trends(
    database: &Database,
    user_id: &str,
    request: TrendRequest,
) -> Result<Value, AppError> {
    if request.metric_ids.is_empty() || request.metric_ids.len() > 20 {
        return Err(AppError::Invalid("Select 1-20 metrics"));
    }
    let start = request
        .start_at
        .as_deref()
        .map(parse_time)
        .transpose()?
        .unwrap_or(i64::MIN);
    let end = request
        .end_at
        .as_deref()
        .map(parse_time)
        .transpose()?
        .unwrap_or(i64::MAX);
    if start > end {
        return Err(AppError::Invalid("Invalid time range"));
    }
    let mut points = Vec::new();
    let mut incomparable = 0;
    let mut excluded = Vec::new();
    for observation in database.confirmed(user_id).await? {
        if !observation
            .payload
            .metric_id
            .as_ref()
            .is_some_and(|id| request.metric_ids.contains(id))
        {
            continue;
        }
        let Some(date) = observation.payload.sampled_at.as_deref() else {
            excluded.push(json!({"observation_id":observation.observation_id,"reason":"missing_collection_date"}));
            incomparable += 1;
            continue;
        };
        let timestamp = parse_time(date)?;
        if timestamp < start || timestamp > end {
            continue;
        }
        let Some((number, unit)) = crate::laboratory::normalized(&observation.payload) else {
            excluded.push(json!({"observation_id":observation.observation_id,"reason":observation.interpretation.result.status}));
            incomparable += 1;
            continue;
        };
        points.push(json!({"observation_id":observation.observation_id,"report_id":observation.report_id,"revision":observation.revision,
            "metric_id":observation.payload.metric_id,"sampled_at":date,"timestamp":timestamp,"date_precision":if date.len()==10{"day"}else{"second"},
            "value":number,"unit":unit,"reference":observation.interpretation.reference,"original":observation.payload}));
    }
    points.sort_by_key(|point| point["timestamp"].as_i64());
    Ok(
        json!({"points":points,"incomparable_count":incomparable,"excluded":excluded,"algorithm_version":"lab-trend-v1","conversion_version":crate::laboratory::CONVERSION_VERSION}),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionOutputRequest {
    pub report_id: String,
    pub run_id: String,
    pub page: i64,
    pub stage: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionInputRequest {
    pub report_id: String,
    pub run_id: String,
    pub page: i64,
}

async fn decode_observations(
    cpu: &crate::execution::CpuExecutor,
    rows: Vec<StoredObservation>,
) -> Result<Vec<Observation>, AppError> {
    cpu.run(move || rows.into_iter().map(StoredObservation::decode).collect())
        .await
}
