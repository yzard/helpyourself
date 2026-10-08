//! Calendar-day sRPE windows require explicit daily completeness declarations.
use crate::error::AppError;
use chrono::{DateTime, Days, TimeZone};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn summarize(
    entries: &[Value],
    start: i64,
    end: i64,
    timezone: chrono_tz::Tz,
) -> Result<Value, AppError> {
    let exclusive_date = DateTime::from_timestamp(end, 0)
        .ok_or(AppError::Invalid("Invalid training window"))?
        .with_timezone(&timezone)
        .date_naive();
    let mut days = Vec::new();
    for offset in (1..=28).rev() {
        let date = exclusive_date
            .checked_sub_days(Days::new(offset))
            .ok_or(AppError::Invalid("Invalid training date"))?;
        let next = date
            .succ_opt()
            .ok_or(AppError::Invalid("Invalid training date"))?;
        let midnight = |date: chrono::NaiveDate| {
            timezone
                .from_local_datetime(&date.and_hms_opt(0, 0, 0).ok_or(AppError::Internal)?)
                .earliest()
                .map(|v| v.timestamp())
                .ok_or(AppError::Invalid(
                    "This timezone has no midnight on the selected date",
                ))
        };
        let from = midnight(date)?;
        let to = midnight(next)?;
        let mut inputs = Vec::new();
        let (mut count, mut known, mut sum, mut rest, mut complete) = (0, 0, 0.0, false, false);
        let mut bases = BTreeSet::new();
        for entry in entries {
            let at = entry["at"].as_i64().ok_or(AppError::Internal)?;
            if at < from || at >= to {
                continue;
            }
            match entry["entry"]["kind"].as_str() {
                Some("training") => {
                    count += 1;
                    bases.insert(
                        entry["calculation"]["duration_basis"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string(),
                    );
                    if let Some(load) = entry["calculation"]["session_load_au"].as_f64() {
                        known += 1;
                        sum += load;
                    }
                }
                Some("training_day") if entry["timezone"].as_str() == Some(timezone.name()) => {
                    match entry["entry"]["content"]["status"].as_str() {
                        Some("rest") => rest = true,
                        Some("all_sessions_logged") => complete = true,
                        _ => return Err(AppError::Internal),
                    }
                }
                _ => continue,
            }
            inputs.push(json!({"record_id":entry["record_id"],"version":entry["version"]}));
        }
        let incompatible = bases.len() > 1 || bases.contains("unknown");
        let state = if from < start || to > end {
            "outside_query"
        } else if incompatible {
            "incompatible_duration_protocols"
        } else if rest && (complete || count > 0) {
            "conflicting_declarations"
        } else if rest {
            "confirmed_rest"
        } else if complete && count > 0 && known == count {
            "complete"
        } else if complete && known < count {
            "missing_effort"
        } else {
            "unconfirmed"
        };
        let value = matches!(state, "complete" | "confirmed_rest").then_some(sum);
        days.push(json!({"date":date.to_string(),"state":state,"load_au":value,"duration_bases":bases,"observed_sum_au":if incompatible {None} else if known>0 {Some(sum)} else if state=="confirmed_rest" {Some(0.0)} else {None},"sessions":count,"known_effort_sessions":known,"inputs":inputs}));
    }
    let windows:Vec<_> = [7,28].into_iter().map(|length| {
        let records=&days[28-length..];
        let complete=records.iter().filter(|day|day["load_au"].is_number()).count();
        let observed:Vec<_>=records.iter().filter_map(|day|day["observed_sum_au"].as_f64()).collect();
        let sum:f64=observed.iter().sum();
        let bases:BTreeSet<_>=records.iter().flat_map(|day|day["duration_bases"].as_array().into_iter().flatten()).filter_map(Value::as_str).collect();
        let compatible=bases.len()<=1 && !bases.contains("unknown");
        json!({"days":length,"start_date":records[0]["date"],"end_date":records[length-1]["date"],"complete_days":complete,"unknown_days":length-complete,"observed_sum_au":if observed.is_empty() || !compatible {None}else{Some(sum)},"duration_bases":bases,"protocol_state":if compatible {"compatible"} else {"incompatible_duration_protocols"},"total_load_au":(complete==length && compatible).then_some(sum),"daily_mean_au":(complete==length && compatible).then_some(sum/length as f64)})
    }).collect();
    Ok(
        json!({"days":days,"windows":windows,"timezone":timezone.name(),"algorithm_version":"srpe-calendar-windows-v1","notes":["Windows contain the last 7 and 28 completed local calendar days. Today is excluded.","Only explicit rest days contribute zero. Confirm all sessions after recording them and their CR10 effort.","Declarations apply only in their recorded timezone. Conflicting declarations remain unknown.","These descriptive windows do not predict injury or combine device effort, heart-rate load, or strength volume."]}),
    )
}
