//! Explicit user reminders with calendar recurrence and quiet-time suppression.
use crate::{database::Database, error::AppError};
use chrono::{Datelike, NaiveDate, NaiveTime, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reminder {
    pub title: String,
    pub body: String,
    pub enabled: bool,
    pub recurrence: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub local_time: String,
    pub weekdays: Vec<u8>,
    pub quiet_start_minute: Option<u16>,
    pub quiet_end_minute: Option<u16>,
    pub note: String,
}
impl Reminder {
    pub fn valid(&self) -> bool {
        let start = NaiveDate::parse_from_str(&self.start_date, "%Y-%m-%d");
        let time = NaiveTime::parse_from_str(&self.local_time, "%H:%M");
        !self.title.trim().is_empty()
            && self.title.len() <= 200
            && self.body.len() <= 2048
            && self.note.len() <= 2048
            && start
                .as_ref()
                .is_ok_and(|d| d.to_string() == self.start_date)
            && time
                .as_ref()
                .is_ok_and(|t| t.format("%H:%M").to_string() == self.local_time)
            && self.end_date.as_ref().is_none_or(|end| {
                NaiveDate::parse_from_str(end, "%Y-%m-%d")
                    .is_ok_and(|d| d.to_string() == *end && start.as_ref().is_ok_and(|s| d >= *s))
            })
            && ["once", "daily", "weekly"].contains(&self.recurrence.as_str())
            && self.weekdays.iter().all(|d| (1..=7).contains(d))
            && self
                .weekdays
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.weekdays.len()
            && if self.recurrence == "weekly" {
                !self.weekdays.is_empty()
            } else {
                self.weekdays.is_empty()
            }
            && self.quiet_start_minute.is_some() == self.quiet_end_minute.is_some()
            && self.quiet_start_minute.is_none_or(|v| v < 1440)
            && self.quiet_end_minute.is_none_or(|v| v < 1440)
            && (self.quiet_start_minute.is_none()
                || self.quiet_start_minute != self.quiet_end_minute)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub start_at: i64,
    pub end_at: i64,
}
pub fn occurrences(record: &Value, start: i64, end: i64) -> Result<Vec<Value>, AppError> {
    let reminder: Reminder = serde_json::from_value(record["entry"]["content"].clone())?;
    if !reminder.valid() {
        return Err(AppError::Invalid("Invalid reminder"));
    }
    if !reminder.enabled {
        return Ok(Vec::new());
    }
    let zone: chrono_tz::Tz = record["timezone"]
        .as_str()
        .ok_or(AppError::Internal)?
        .parse()
        .map_err(|_| AppError::Internal)?;
    let mut date = zone
        .timestamp_opt(start, 0)
        .single()
        .ok_or(AppError::Invalid("Invalid time"))?
        .date_naive();
    let last = zone
        .timestamp_opt(end, 0)
        .single()
        .ok_or(AppError::Invalid("Invalid time"))?
        .date_naive();
    let time =
        NaiveTime::parse_from_str(&reminder.local_time, "%H:%M").map_err(|_| AppError::Internal)?;
    let minute = (time.hour() * 60 + time.minute()) as u16;
    let quiet = match (reminder.quiet_start_minute, reminder.quiet_end_minute) {
        (Some(a), Some(b)) => {
            if a < b {
                (a..b).contains(&minute)
            } else {
                minute >= a || minute < b
            }
        }
        _ => false,
    };
    let mut output = Vec::new();
    while date <= last {
        let day = date.to_string();
        let applicable = day >= reminder.start_date
            && reminder.end_date.as_ref().is_none_or(|end| day <= *end)
            && match reminder.recurrence.as_str() {
                "once" => day == reminder.start_date,
                "weekly" => reminder
                    .weekdays
                    .contains(&(date.weekday().number_from_monday() as u8)),
                _ => true,
            };
        if applicable {
            let resolved = zone.from_local_datetime(&date.and_time(time)).earliest();
            let at = resolved.map(|v| v.timestamp());
            let in_window = match at {
                Some(value) => value >= start && value < end,
                None => {
                    let local = date.and_time(time);
                    local
                        >= zone
                            .timestamp_opt(start, 0)
                            .single()
                            .ok_or(AppError::Invalid("Invalid time"))?
                            .naive_local()
                        && local
                            < zone
                                .timestamp_opt(end, 0)
                                .single()
                                .ok_or(AppError::Invalid("Invalid time"))?
                                .naive_local()
                }
            };
            if in_window {
                output.push(json!({"record_id":record["record_id"],"version":record["version"],"date":day,"local_time":reminder.local_time,"timezone":zone.name(),"at":at,"title":reminder.title,"body":reminder.body,"state":if at.is_none(){"nonexistent_local_time"}else if quiet{"quiet_time"}else{"scheduled"}}));
            }
        }
        date = date.succ_opt().ok_or(AppError::Invalid("Date overflow"))?;
    }
    Ok(output)
}
impl Database {
    pub async fn reminder_occurrences(
        &self,
        user: &str,
        request: Request,
    ) -> Result<Value, AppError> {
        if request.end_at <= request.start_at
            || i128::from(request.end_at) - i128::from(request.start_at) > 31 * 86400
        {
            return Err(AppError::Invalid("Select at most 31 days"));
        }
        let input = self
            .wellness_library(
                user,
                super::planning::LibraryRequest {
                    kind: "reminder".into(),
                },
            )
            .await?;
        let mut items = Vec::new();
        for record in input["entries"].as_array().ok_or(AppError::Internal)? {
            items.extend(occurrences(record, request.start_at, request.end_at)?);
            if items.len() > 10000 {
                return Err(AppError::TooLarge);
            }
        }
        items.sort_by(|a, b| {
            a["date"]
                .as_str()
                .cmp(&b["date"].as_str())
                .then(a["at"].as_i64().cmp(&b["at"].as_i64()))
                .then(a["record_id"].as_str().cmp(&b["record_id"].as_str()))
        });
        Ok(
            json!({"occurrences":items,"data_revision":input["data_revision"],"notes":["Quiet hours suppress the occurrence. They do not shift it.","A repeated clock time uses its first occurrence. A nonexistent local time is skipped.","This feed does not prove delivery. Each client reports its own notification permission and scheduling horizon."]}),
        )
    }
}
