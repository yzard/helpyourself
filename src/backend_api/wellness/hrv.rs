//! Five-minute normal-to-normal intervals, separated by declared acquisition protocol.
use crate::{
    database::{Database, queries},
    error::AppError,
    health::StoredHealthRecord,
};
use chrono::{Days, TimeZone};
use futures_util::TryStreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub start_at: i64,
    pub end_at: i64,
    pub timezone: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intervals {
    pub schema: String,
    pub protocol_id: String,
    pub context: String,
    pub device_model: String,
    pub firmware: String,
    pub posture: String,
    pub quality_reviewed: bool,
    pub intervals: Vec<Interval>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Interval {
    pub offset_seconds: f64,
    pub nn_ms: f64,
    pub quality: String,
    pub preceded_by_gap: bool,
}

pub fn calculate(input: &Intervals) -> Result<Value, AppError> {
    if input.schema != "nn-intervals-v1"
        || !input.quality_reviewed
        || input.intervals.len() < 2
        || input.intervals.len() > 3000
        || [
            &input.protocol_id,
            &input.device_model,
            &input.firmware,
            &input.posture,
        ]
        .iter()
        .any(|v| v.trim().is_empty() || v.len() > 128)
        || !["morning_rest", "night", "deep_sleep"].contains(&input.context.as_str())
    {
        return Err(AppError::Invalid(
            "A declared protocol and quality-reviewed normal intervals are required",
        ));
    }
    let mut previous = 0.0;
    let mut nn = Vec::new();
    for interval in &input.intervals {
        if !interval.offset_seconds.is_finite()
            || !interval.nn_ms.is_finite()
            || interval.nn_ms <= 0.0
            || interval.nn_ms > 10000.0
            || interval.quality != "normal"
            || interval.preceded_by_gap
            || (interval.offset_seconds - previous - interval.nn_ms / 1000.0).abs() > 0.002
        {
            return Err(AppError::Invalid(
                "Discontinuous or unreviewed intervals cannot form an RMSSD window",
            ));
        }
        previous = interval.offset_seconds;
        nn.push(interval.nn_ms);
    }
    if !(300.0..=310.0).contains(&previous)
        || previous - nn.last().copied().ok_or(AppError::Internal)? / 1000.0 >= 300.0
    {
        return Err(AppError::Invalid(
            "Supply exactly the intervals through the first beat at or after five minutes",
        ));
    }
    let rmssd = super::algorithms::rmssd(&nn)?;
    Ok(
        json!({"rmssd_ms":rmssd,"ln_rmssd":if rmssd>0.0 {Some(rmssd.ln())} else {None},"state":if rmssd>0.0 {"observed"} else {"zero_variability"},"interval_count":nn.len(),"duration_seconds":previous,"algorithm_version":"nn-five-minute-v1"}),
    )
}

impl Database {
    pub async fn hrv_windows(&self, user: &str, request: Request) -> Result<Value, AppError> {
        let zone: chrono_tz::Tz = request
            .timezone
            .parse()
            .map_err(|_| AppError::Invalid("Unknown timezone"))?;
        if request.end_at <= request.start_at
            || i128::from(request.end_at) - i128::from(request.start_at) > 90 * 86400
        {
            return Err(AppError::Invalid("Select at most 90 days"));
        }
        let revision = self.data_revision(user).await?;
        let mut rows = Vec::<StoredHealthRecord>::new();
        let mut bytes = 0usize;
        let mut stream = sqlx::query_as::<_, StoredHealthRecord>(queries::HRV_WINDOWS)
            .bind(user)
            .bind(request.start_at)
            .bind(request.end_at)
            .fetch(&self.pool);
        while let Some(row) = stream.try_next().await? {
            bytes = bytes.saturating_add(row.payload_json.len());
            if bytes > 16 * 1024 * 1024 || rows.len() >= 1000 {
                return Err(AppError::TooLarge);
            }
            rows.push(row);
        }
        drop(stream);
        let result = self.cpu.run(move || summarize(rows, zone)).await?;
        if self.data_revision(user).await? != revision {
            return Err(AppError::Conflict("Archive changed; refresh HRV"));
        }
        Ok(
            json!({"sources":result,"data_revision":revision,"algorithm_version":"nn-five-minute-v1","notes":["SDNN and unclassified heartbeat series are not converted to RMSSD.","Quality labels and acquisition metadata are provider declarations, not independent validation.","A protocol change starts a separate baseline. No recovery percentage is inferred.","Daily medians and 28-day baselines are descriptive engineering summaries."]}),
        )
    }
}

pub fn summarize(rows: Vec<StoredHealthRecord>, zone: chrono_tz::Tz) -> Result<Value, AppError> {
    let mut groups: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for row in rows {
        let envelope: Value = serde_json::from_str(&row.payload_json)?;
        let payload = &envelope["payload"];
        let parsed = serde_json::from_value::<Intervals>(payload.clone());
        let protocol = json!({"protocol_id":payload["protocol_id"],"context":payload["context"],"device_model":payload["device_model"],"firmware":payload["firmware"],"posture":payload["posture"]});
        let key = format!("{}:{}:{}", row.platform, row.source_id, protocol);
        let mut result = match parsed {
            Ok(ref input)
                if input.intervals.last().is_some_and(|v| {
                    ((i128::from(row.end_at) - i128::from(row.start_at)) as f64 - v.offset_seconds)
                        .abs()
                        > 1.0
                }) =>
            {
                json!({"state":"ineligible_window","reason":"The record duration does not match the NN window."})
            }
            Ok(ref input) => match calculate(input) {
                Ok(value) => value,
                Err(error) => json!({"state":"ineligible_window","reason":error.to_string()}),
            },
            Err(_) => {
                json!({"state":"quality_or_protocol_unknown","reason":"The archived sample does not declare reviewed NN intervals and a five-minute protocol."})
            }
        };
        let date = zone
            .timestamp_opt(row.start_at, 0)
            .single()
            .ok_or(AppError::Invalid("Invalid timestamp"))?
            .date_naive();
        result["record_id"] = json!(row.record_id);
        result["version"] = json!(row.version);
        result["at"] = json!(row.start_at);
        result["date"] = json!(date.to_string());
        result["protocol"] = protocol;
        result["source"] = json!(format!("{}:{}", row.platform, row.source_id));
        groups.entry(key).or_default().push(result);
    }
    let mut output = Vec::new();
    for (key, windows) in groups {
        let mut grouped: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for window in &windows {
            if let Some(v) = window["ln_rmssd"].as_f64() {
                grouped
                    .entry(window["date"].as_str().ok_or(AppError::Internal)?.into())
                    .or_default()
                    .push(v);
            }
        }
        let mut medians = BTreeMap::new();
        for (date, mut values) in grouped {
            medians.insert(date, super::algorithms::median(&mut values)?);
        }
        let mut days = Vec::new();
        for (date, value) in &medians {
            let current = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .map_err(|_| AppError::Internal)?;
            let lower = current
                .checked_sub_days(Days::new(42))
                .ok_or(AppError::Internal)?
                .to_string();
            let mut history: Vec<f64> = medians
                .range(lower..date.clone())
                .rev()
                .take(28)
                .map(|(_, v)| *v)
                .collect();
            let count = history.len();
            let baseline = if count == 28 {
                let median = super::algorithms::median(&mut history)?;
                let mut deviations: Vec<_> = history.iter().map(|v| (v - median).abs()).collect();
                let mad = super::algorithms::median(&mut deviations)?;
                Some(
                    json!({"median":median,"mad":mad,"z":if mad>0.0 {Some((value-median)/(1.4826*mad))}else{None}}),
                )
            } else {
                None
            };
            let week_start = current
                .checked_sub_days(Days::new(6))
                .ok_or(AppError::Internal)?
                .to_string();
            let mut week: Vec<f64> = medians
                .range(week_start..=date.clone())
                .map(|(_, v)| *v)
                .collect();
            let week_count = week.len();
            let week_median = if week_count >= 5 {
                Some(super::algorithms::median(&mut week)?)
            } else {
                None
            };
            days.push(
                json!({"date":date,"ln_rmssd":value,"seven_day_valid_days":week_count,"seven_day_median":week_median,"baseline":baseline,"baseline_days":count}),
            );
        }
        output.push(json!({"key":key,"source":windows[0]["source"],"protocol":windows[0]["protocol"],"windows":windows,"days":days}));
    }
    Ok(json!(output))
}
