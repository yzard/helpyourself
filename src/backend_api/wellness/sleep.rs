//! Session summaries preserve observed sleep, awake, and in-bed intervals separately.
use crate::{
    database::{Database, queries},
    error::AppError,
    health::StoredHealthRecord,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SleepRequest {
    pub start_at: i64,
    pub end_at: i64,
    pub timezone: String,
}
pub(crate) fn merge(mut spans: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    spans.sort_unstable();
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (start, end) in spans {
        if end <= start {
            continue;
        }
        if let Some(last) = merged.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
            continue;
        }
        merged.push((start, end));
    }
    merged
}
fn duration(spans: &[(i64, i64)]) -> i64 {
    spans.iter().map(|(a, b)| b - a).sum()
}
fn intersection(a: &[(i64, i64)], b: &[(i64, i64)]) -> i64 {
    let (mut i, mut j, mut total) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        total += (a[i].1.min(b[j].1) - a[i].0.max(b[j].0)).max(0);
        if a[i].1 < b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    total
}
pub fn summarize(
    records: Vec<StoredHealthRecord>,
    start: i64,
    end: i64,
    timezone: chrono_tz::Tz,
) -> Result<Value, AppError> {
    let mut sources: BTreeMap<String, Vec<(StoredHealthRecord, i64)>> = BTreeMap::new();
    for record in records {
        if chrono::DateTime::from_timestamp(record.start_at, 0).is_none()
            || chrono::DateTime::from_timestamp(record.end_at, 0).is_none()
            || i128::from(record.end_at) - i128::from(record.start_at) > 7 * 86400
        {
            return Err(AppError::Invalid(
                "Sleep interval is outside the supported timestamp or seven-day range",
            ));
        }
        let raw: Value = serde_json::from_str(&record.payload_json)?;
        let Some(category) = raw["payload"]["category"]
            .as_i64()
            .filter(|c| (0..=5).contains(c))
        else {
            continue;
        };
        if record.end_at <= record.start_at {
            continue;
        }
        sources
            .entry(format!("{}:{}", record.platform, record.source_id))
            .or_default()
            .push((record, category));
    }
    let mut results = Vec::new();
    for (source, mut records) in sources {
        records.sort_by_key(|(r, _)| r.start_at);
        let mut groups: Vec<Vec<(StoredHealthRecord, i64)>> = Vec::new();
        let mut group_end = None;
        for record in records {
            if group_end.is_none_or(|at: i64| record.0.start_at.saturating_sub(at) > 90 * 60) {
                groups.push(Vec::new());
            }
            group_end = Some(group_end.unwrap_or(record.0.end_at).max(record.0.end_at));
            groups.last_mut().ok_or(AppError::Internal)?.push(record);
        }
        let mut sessions = Vec::new();
        for group in groups {
            let from = group
                .iter()
                .map(|(r, _)| r.start_at)
                .min()
                .ok_or(AppError::Internal)?;
            let to = group
                .iter()
                .map(|(r, _)| r.end_at)
                .max()
                .ok_or(AppError::Internal)?;
            if to < start || from >= end {
                continue;
            }
            let spans = |categories: &[i64]| {
                merge(
                    group
                        .iter()
                        .filter(|(_, c)| categories.contains(c))
                        .map(|(r, _)| (r.start_at, r.end_at))
                        .collect(),
                )
            };
            let asleep = spans(&[1, 3, 4, 5]);
            let awake = spans(&[2]);
            let in_bed = spans(&[0]);
            let asleep_seconds = duration(&asleep);
            let in_bed_seconds = duration(&in_bed);
            let partial = from < start || to > end;
            let conflict = intersection(&asleep, &awake) > 0;
            let covered = intersection(&asleep, &in_bed) == asleep_seconds;
            let efficiency =
                if !asleep.is_empty() && !partial && !conflict && covered && in_bed_seconds > 0 {
                    Some(asleep_seconds as f64 / in_bed_seconds as f64)
                } else {
                    None
                };
            let wake_date = chrono::DateTime::from_timestamp(to, 0)
                .ok_or(AppError::Invalid("Sleep timestamp is out of range"))?
                .with_timezone(&timezone)
                .date_naive()
                .to_string();
            let timeline:Vec<Value>=group.iter().map(|(r,c)|json!({"record_id":r.record_id,"version":r.version,"start_at":r.start_at,"end_at":r.end_at,"category":c})).collect();
            sessions.push(json!({"start_at":from,"end_at":to,"wake_date":wake_date,"asleep_seconds":if asleep.is_empty() || conflict || partial {None} else {Some(asleep_seconds)},"observed_awake_seconds":duration(&awake),"in_bed_seconds":if in_bed_seconds>0 {Some(in_bed_seconds)} else {None},"efficiency":efficiency,"state":if partial {"partial_session"} else if conflict {"conflicting_states"} else if asleep.is_empty() {"insufficient_data"} else {"ready"},"classification":"unclassified_session","timeline":timeline,"touches_query_boundary":from<start||to>end}));
        }
        results.push(json!({"source":source,"sessions":sessions}));
    }
    Ok(
        json!({"sources":results,"algorithm_version":"sleep-sessions-v1","grouping_gap_seconds":5400,"evidence_status":"descriptive","notes":["Grouping uses a 90-minute engineering threshold, not a validated sleep classifier.","Gaps do not imply wakefulness. In-bed time is not inferred from session boundaries.","Main sleep and naps require confirmation. Raw stage intervals retain their original labels."]}),
    )
}
impl Database {
    pub async fn sleep_sessions(
        &self,
        user: &str,
        request: SleepRequest,
    ) -> Result<Value, AppError> {
        if request.end_at <= request.start_at
            || i128::from(request.end_at) - i128::from(request.start_at) > 90 * 86400
            || chrono::DateTime::from_timestamp(request.start_at, 0).is_none()
            || chrono::DateTime::from_timestamp(request.end_at, 0).is_none()
        {
            return Err(AppError::Invalid("Select a sleep range of at most 90 days"));
        }
        let timezone = request
            .timezone
            .parse::<chrono_tz::Tz>()
            .map_err(|_| AppError::Invalid("Unknown IANA timezone"))?;
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
        let annotations = self
            .list_wellness_entries(
                user,
                super::entries::ListRequest {
                    start_at: request
                        .start_at
                        .checked_sub(7 * 86400)
                        .ok_or(AppError::Invalid("Invalid sleep query"))?,
                    end_at: request.end_at,
                    kind: Some("sleep_correction".into()),
                },
            )
            .await?;
        let mut result = self
            .cpu
            .run(move || {
                let mut result = summarize(rows, request.start_at, request.end_at, timezone)?;
                apply_corrections(
                    &mut result,
                    annotations["entries"]
                        .as_array()
                        .ok_or(AppError::Internal)?,
                )?;
                Ok(result)
            })
            .await?;
        if self.data_revision(user).await? != revision {
            return Err(AppError::Conflict(
                "Archive changed; refresh sleep sessions",
            ));
        }
        result["data_revision"] = json!(revision);
        result["computed_at"] = json!(crate::authentication::now()?);
        Ok(result)
    }
}

pub fn apply_corrections(result: &mut Value, corrections: &[Value]) -> Result<(), AppError> {
    for source in result["sources"].as_array_mut().ok_or(AppError::Internal)? {
        let source_name = source["source"]
            .as_str()
            .ok_or(AppError::Internal)?
            .to_string();
        for session in source["sessions"]
            .as_array_mut()
            .ok_or(AppError::Internal)?
        {
            let matching: Vec<_> = corrections
                .iter()
                .filter(|record| {
                    let content = &record["entry"]["content"];
                    content["source"] == source_name
                        && ((content["session_start"] == session["start_at"]
                            && content["session_end"] == session["end_at"])
                            || session["timeline"].as_array().is_some_and(|timeline| {
                                timeline.iter().any(|stage| {
                                    stage["record_id"].as_str().is_some_and(|id| {
                                        content["basis_revisions"].get(id).is_some()
                                    })
                                })
                            }))
                })
                .collect();
            session["correction_state"] = json!("none");
            session["user_asleep_seconds"] = Value::Null;
            session["corrections"] = json!(matching);
            if matching.len() > 1 {
                session["correction_state"] = json!("conflicting_corrections");
                continue;
            }
            let Some(record) = matching.first() else {
                continue;
            };
            let content = &record["entry"]["content"];
            let basis = content["basis_revisions"]
                .as_object()
                .ok_or(AppError::Internal)?;
            let timeline = session["timeline"].as_array().ok_or(AppError::Internal)?;
            if content["session_start"] != session["start_at"]
                || content["session_end"] != session["end_at"]
                || basis.len() != timeline.len()
                || !timeline.iter().all(|stage| {
                    stage["record_id"]
                        .as_str()
                        .is_some_and(|id| basis.get(id) == Some(&stage["version"]))
                })
            {
                session["correction_state"] = json!("stale_basis");
                continue;
            }
            session["correction_state"] = json!("applied_user_report");
            session["classification"] = content["classification"].clone();
            session["user_asleep_seconds"] = content["corrected_asleep_minutes"]
                .as_f64()
                .map_or(Value::Null, |minutes| json!(minutes * 60.0));
        }
    }
    result["correction_notes"] = json!([
        "Corrections preserve the source intervals, duration and efficiency. The user estimate is a separate value.",
        "Classification and user duration apply only to the same source session and input revisions. Changed inputs require a new confirmation.",
        "Corrections do not create minute-level sleep/wake states and do not change SRI."
    ]);
    Ok(())
}
