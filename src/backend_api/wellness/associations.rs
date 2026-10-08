//! Declared, source-separated behavior associations. Stored outputs are invalidated by source edits.
use super::statistics::{self, Row};
use crate::{
    authentication::now,
    database::{Database, queries},
    error::AppError,
    health::StoredHealthRecord,
};
use chrono::{Datelike, Days, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row as SqlRow;
use std::collections::{BTreeMap, BTreeSet};
pub const VERSION: &str = "behavior-hac-calendar-v1";
#[derive(Deserialize, Serialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub end_date: String,
    pub timezone: String,
    pub outcome: String,
    pub source: String,
    pub behaviors: Vec<String>,
    pub covariates: Vec<String>,
    pub lag_days: u8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetRequest {
    pub result_id: String,
}
impl Database {
    pub async fn behavior_associations(
        &self,
        user: &str,
        request: Request,
    ) -> Result<Value, AppError> {
        let zone: chrono_tz::Tz = request
            .timezone
            .parse()
            .map_err(|_| AppError::Invalid("Unknown timezone"))?;
        let end = NaiveDate::parse_from_str(&request.end_date, "%Y-%m-%d")
            .map_err(|_| AppError::Invalid("Invalid end date"))?;
        let unique: BTreeSet<_> = request
            .behaviors
            .iter()
            .chain(&request.covariates)
            .collect();
        if end.to_string() != request.end_date
            || request.lag_days > 1
            || request.behaviors.is_empty()
            || request.behaviors.len() > 8
            || request.covariates.len() > 3
            || unique.len() != request.behaviors.len() + request.covariates.len()
            || unique.iter().any(|s| s.trim().is_empty() || s.len() > 80)
            || !super::KINDS.contains(&request.outcome.as_str())
            || request.source.is_empty()
            || request.source.len() > 512
        {
            return Err(AppError::Invalid(
                "Declare one outcome and source, 1–8 behaviors, up to 3 distinct covariates, and a lag of 0 or 1 day",
            ));
        }
        let start = end
            .checked_sub_days(Days::new(89))
            .ok_or(AppError::Invalid("Date overflow"))?;
        let predictor_start = start
            .checked_sub_days(Days::new(u64::from(request.lag_days)))
            .ok_or(AppError::Invalid("Date overflow"))?;
        let boundary = |date: NaiveDate| {
            zone.from_local_datetime(&date.and_hms_opt(0, 0, 0).ok_or(AppError::Internal)?)
                .earliest()
                .map(|v| v.timestamp())
                .ok_or(AppError::Invalid(
                    "Selected timezone has no midnight on this date",
                ))
        };
        let start_at = boundary(start)?;
        let end_at = boundary(end.succ_opt().ok_or(AppError::Invalid("Date overflow"))?)?;
        if end_at > now()? {
            return Err(AppError::Invalid("Use only completed calendar days"));
        }
        let revision = self.data_revision(user).await?;
        let journals = self
            .list_wellness_entries(
                user,
                super::entries::ListRequest {
                    start_at: boundary(predictor_start)?,
                    end_at,
                    kind: Some("journal".into()),
                },
            )
            .await?;
        let records: Vec<StoredHealthRecord> = sqlx::query_as(queries::HEALTH_RANGE)
            .bind(user)
            .bind(&request.outcome)
            .bind(start_at)
            .bind(end_at)
            .fetch_all(&self.pool)
            .await?;
        if records.len() > 50000 {
            return Err(AppError::TooLarge);
        }
        let request_copy = request.clone();
        let mut output=self.cpu.run(move||{
            let records:Vec<_>=records.into_iter().filter(|r|format!("{}:{}",r.platform,r.source_id)==request_copy.source).collect();
            let evidence:Vec<_>=records.iter().map(|r|json!({"record_id":r.record_id,"version":r.version,"start_at":r.start_at,"end_at":r.end_at,"payload_projection":serde_json::from_str::<Value>(&r.payload_json).ok()})).collect();
            let aggregates=crate::health::aggregate_records(records,&request_copy.outcome,start,end,zone)?;
            let mut result=analyze(&request_copy,start,&journals["entries"],&aggregates["days"])?;
            result["input_records"]=json!({"health":evidence,"journals":journals["entries"]});
            Ok(result)
        }).await?;
        let id = uuid::Uuid::new_v4().to_string();
        let created = now()?;
        output["result_id"] = json!(id);
        output["data_revision"] = json!(revision);
        output["computed_at"] = json!(created);
        let serialized = serde_json::to_string(&output)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let current: i64 = sqlx::query_scalar(queries::USER_DATA_REVISION)
            .bind(user)
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or(AppError::NotFound)?;
        if current != revision {
            return Err(AppError::Conflict(
                "Archive changed; run the declared analysis again",
            ));
        }
        sqlx::query(queries::INSERT_DERIVED_RESULT)
            .bind(&id)
            .bind(user)
            .bind(VERSION)
            .bind(revision)
            .bind(serde_json::to_string(&request)?)
            .bind(serde_json::to_string(&output["input_records"])?)
            .bind(&serialized)
            .bind(created)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(queries::LIMIT_DERIVED_RESULTS)
            .bind(user)
            .bind(user)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(output)
    }
    pub async fn association_results(&self, user: &str) -> Result<Value, AppError> {
        let rows = sqlx::query(queries::LIST_DERIVED_RESULTS)
            .bind(user)
            .fetch_all(&self.pool)
            .await?;
        let mut results = Vec::new();
        for row in rows {
            results.push(json!({"result_id":row.get::<String,_>("result_id"),"created_at":row.get::<i64,_>("created_at"),"data_revision":row.get::<i64,_>("input_revision"),"parameters":serde_json::from_str::<Value>(&row.get::<String,_>("parameters_json"))?}));
        }
        Ok(json!({"results":results,"retention_limit":100}))
    }
    pub async fn association_result(&self, user: &str, id: &str) -> Result<Value, AppError> {
        let output: String = sqlx::query_scalar(queries::GET_DERIVED_RESULT)
            .bind(user)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)?;
        Ok(serde_json::from_str(&output)?)
    }
}
#[derive(Clone)]
struct Measurement {
    value: f64,
    kind: String,
    unit: String,
}
fn read(content: &Value, name: &str) -> Option<Measurement> {
    if let Some(value) = content["behaviors"][name].as_bool() {
        return Some(Measurement {
            value: if value { 1.0 } else { 0.0 },
            kind: "boolean".into(),
            unit: "yes versus no".into(),
        });
    }
    if let Some(value) = content["measurements"][name]["value"].as_f64() {
        return Some(Measurement {
            value,
            kind: "number".into(),
            unit: content["measurements"][name]["unit"].as_str()?.into(),
        });
    }
    if let Some(minute) = content["times"][name].as_u64() {
        return Some(Measurement {
            value: ((minute + 720) % 1440) as f64,
            kind: "clock_time".into(),
            unit: "minute after noon".into(),
        });
    }
    None
}
pub fn analyze(
    request: &Request,
    start: NaiveDate,
    journals: &Value,
    outcomes: &Value,
) -> Result<Value, AppError> {
    let zone: chrono_tz::Tz = request
        .timezone
        .parse()
        .map_err(|_| AppError::Invalid("Unknown timezone"))?;
    let mut daily: BTreeMap<NaiveDate, Vec<&Value>> = BTreeMap::new();
    for journal in journals.as_array().ok_or(AppError::Internal)? {
        if journal["timezone"].as_str() != Some(request.timezone.as_str()) {
            continue;
        }
        let date =
            chrono::DateTime::from_timestamp(journal["at"].as_i64().ok_or(AppError::Internal)?, 0)
                .ok_or(AppError::Internal)?
                .with_timezone(&zone)
                .date_naive();
        daily
            .entry(date)
            .or_default()
            .push(&journal["entry"]["content"]);
    }
    let outcomes = outcomes.as_array().ok_or(AppError::Internal)?;
    let mut results = Vec::new();
    for name in &request.behaviors {
        let mut rows = Vec::new();
        let mut day_inputs = Vec::new();
        let mut protocol = BTreeMap::new();
        let mut drift = false;
        let mut yes = 0;
        let mut no = 0;
        for offset in 0..90 {
            let date = start
                .checked_add_days(Days::new(offset))
                .ok_or(AppError::Internal)?;
            let predictor_date = date
                .checked_sub_days(Days::new(u64::from(request.lag_days)))
                .ok_or(AppError::Internal)?;
            let values = daily.get(&predictor_date);
            let outcome = outcomes
                .iter()
                .find(|r| r["date"] == date.to_string() && r["source"] == request.source);
            let value = outcome.and_then(|r| {
                if r["excluded_sample_count"].as_u64() == Some(0) {
                    r["value"].as_f64()
                } else {
                    None
                }
            });
            let mut predictors = Vec::new();
            let mut reason = "missing_journal_or_measurement";
            if let Some(contents) = values {
                reason = if contents.len() == 1 {
                    "missing_journal_or_measurement"
                } else {
                    "multiple_journals"
                };
                if contents.len() == 1 {
                    for key in std::iter::once(name).chain(&request.covariates) {
                        if let Some(measurement) = read(contents[0], key) {
                            let identity = (measurement.kind.clone(), measurement.unit.clone());
                            if protocol
                                .get(key)
                                .is_some_and(|existing| *existing != identity)
                            {
                                drift = true;
                            }
                            protocol.insert(key.clone(), identity);
                            predictors.push(measurement.value);
                        } else {
                            break;
                        }
                    }
                }
            }
            let complete = predictors.len() == 1 + request.covariates.len() && value.is_some();
            if complete {
                if predictors[0] == 1.0 {
                    yes += 1;
                } else if predictors[0] == 0.0 {
                    no += 1;
                }
                let mut x = vec![1.0, predictors[0], offset as f64 / 90.0];
                let weekday = date.weekday().num_days_from_monday();
                for day in 1..7 {
                    x.push(if weekday == day { 1.0 } else { 0.0 });
                }
                x.extend_from_slice(&predictors[1..]);
                rows.push(Row {
                    day: offset as usize,
                    x,
                    y: value.ok_or(AppError::Internal)?,
                });
            }
            day_inputs.push(json!({"date":date.to_string(),"predictor_date":predictor_date.to_string(),"outcome":value,"predictors":predictors,"state":if complete{"included"}else if value.is_none(){"missing_or_excluded_outcome"}else{reason}}));
        }
        let kind = protocol
            .get(name)
            .map(|v| v.0.as_str())
            .unwrap_or("unknown");
        let mut state = if drift {
            "behavior_type_or_unit_changed"
        } else if rows.len() < 60 {
            "insufficient_days"
        } else if kind == "boolean" && (yes < 20 || no < 20) {
            "insufficient_boolean_groups"
        } else {
            "estimated"
        };
        // Center and scale only measured predictors. Effects are converted back to their original units.
        let predictor_columns: Vec<_> = std::iter::once(1)
            .chain(9..9 + request.covariates.len())
            .collect();
        let mut scale = 1.0;
        for col in predictor_columns {
            if rows.is_empty() {
                break;
            }
            let mean = rows.iter().map(|r| r.x[col]).sum::<f64>() / rows.len() as f64;
            let sd = (rows.iter().map(|r| (r.x[col] - mean).powi(2)).sum::<f64>()
                / rows.len() as f64)
                .sqrt();
            let range = rows
                .iter()
                .map(|r| r.x[col])
                .fold(f64::NEG_INFINITY, f64::max)
                - rows.iter().map(|r| r.x[col]).fold(f64::INFINITY, f64::min);
            let key = if col == 1 {
                name
            } else {
                &request.covariates[col - 9]
            };
            if protocol.get(key).is_some_and(|v| v.0 == "clock_time") && range > 720.0 {
                state = "clock_time_crosses_anchor";
            }
            if sd <= 1e-8 {
                if state == "estimated" {
                    state = "no_predictor_variation";
                }
                continue;
            }
            if col == 1 {
                scale = sd;
            }
            for row in &mut rows {
                row.x[col] = (row.x[col] - mean) / sd;
            }
        }
        let mut estimate = None;
        if state == "estimated" {
            let encoded = serde_json::to_vec(&(request, name))?;
            let digest = crate::authentication::digest(&encoded);
            let seed = u64::from_str_radix(&digest[..16], 16).map_err(|_| AppError::Internal)?;
            estimate = statistics::regress(&rows, seed);
            if let Some(value) = &mut estimate {
                for field in ["effect", "standard_error"] {
                    value[field] = json!(value[field].as_f64().ok_or(AppError::Internal)? / scale);
                }
                for field in ["ci_95", "bootstrap_ci_95"] {
                    if let Some(bounds) = value[field].as_array_mut() {
                        for bound in bounds {
                            *bound = json!(bound.as_f64().ok_or(AppError::Internal)? / scale);
                        }
                    }
                }
            } else {
                state = "singular_or_degenerate_model";
            }
        }
        results.push(json!({"behavior":name,"state":state,"sample_days":rows.len(),"missing_days":90-rows.len(),"predictor_kind":kind,"predictor_unit":protocol.get(name).map(|v|&v.1),"outcome_unit":outcomes.iter().find_map(|r|r["unit"].as_str()),"estimate":estimate,"days":day_inputs}));
    }
    let p: Vec<_> = results
        .iter()
        .map(|r| r["estimate"]["p_value"].as_f64())
        .collect();
    let q = statistics::benjamini_yekutieli(&p);
    for (result, q) in results.iter_mut().zip(q) {
        result["q_value"] = json!(q);
        result["passes_by_threshold"] = Value::Null;
        result["inference_status"] = json!("calibration_failed_research_only");
    }
    Ok(
        json!({"algorithm_version":VERSION,"evidence_status":"calibration_failed_research_only","parameters":request,"protocol":{"window_days":90,"minimum_complete_days":60,"minimum_boolean_yes_and_no":20,"hac_lag_calendar_days":statistics::HAC_LAG,"kernel":"Bartlett","finite_sample_factor":"n/(n-p)","inference":"Student t with n-p degrees of freedom; approximate under dependent errors","bootstrap_block_calendar_days":statistics::BLOCK_DAYS,"bootstrap_replicates":statistics::BOOTSTRAPS,"bootstrap":"non-circular moving pairs blocks; missing calendar slots retained; linear quantiles","time_anchor":"noon; stop if observed range crosses more than 12 hours","multiple_testing":"Benjamini-Yekutieli across the complete declared family, including unestimable tests as p=1","design":"intercept, behavior, linear calendar trend, six weekday indicators, declared covariates","source_protocol":"one explicit producer; record coverage does not establish device protocol equivalence"},"results":results,"calibration":{"status":"failed_nominal_error_control","synthetic_trials_per_scenario":1000,"observed_null_rejection_range":[0.078,0.235],"nominal_alpha":0.05,"significance_decisions_enabled":false},"notes":["Nominal p values and BY q values are research diagnostics only. Fixed-protocol simulations did not control the claimed error rate. No significant-discovery decision is made.","The displayed 95% intervals are nominal intervals with unvalidated coverage for personal time series.","Associations are not causal effects. Unmeasured confounding and selective missingness can remain.","These fixed engineering thresholds do not establish power, clinical validity, or reliable inference for every personal time series.","Do not search across dates, lags or behavior subsets for significance. BY correction covers only this declared batch, not repeated analyses.","Multiple journals on one date are excluded rather than silently combined. Predictor units and kinds must remain unchanged.","Clock times use a noon anchor and are not analyzed when the observed range spans more than 12 hours.","Source edits or deletions remove saved results and their snapshots. A maximum of 100 current results is retained."]}),
    )
}
