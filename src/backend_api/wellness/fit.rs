//! Bounded FIT activity import. Original bytes and all decoded messages remain archived.
use crate::{database::Database, error::AppError, health::HealthRecordInput};
use base64::{Engine, engine::general_purpose::STANDARD};
use fitparser::{FitDataRecord, Value as FitValue, profile::MesgNum};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub filename: String,
    pub data_base64: String,
}

/// Read only message sizes before allocating profile-decoded fields.
pub fn preflight(bytes: &[u8]) -> Result<(), AppError> {
    let invalid = || AppError::Invalid("Invalid or unsupported FIT framing");
    if bytes.len() < 14
        || bytes.len() > 4 * 1024 * 1024
        || !matches!(bytes[0], 12 | 14)
        || &bytes[8..12] != b".FIT"
    {
        return Err(invalid());
    }
    let header = bytes[0] as usize;
    let size = u32::from_le_bytes(bytes[4..8].try_into().map_err(|_| invalid())?) as usize;
    if header.checked_add(size).and_then(|v| v.checked_add(2)) != Some(bytes.len()) {
        return Err(invalid());
    }
    let end = header + size;
    let mut at = header;
    let mut definitions = [None::<(usize, bool)>; 16];
    let mut count = 0;
    while at < end {
        count += 1;
        if count > 50000 {
            return Err(AppError::TooLarge);
        }
        let head = bytes[at];
        at += 1;
        let compressed = head & 0x80 != 0;
        let local = if compressed {
            ((head >> 5) & 3) as usize
        } else {
            (head & 15) as usize
        };
        if !compressed && head & 0x40 != 0 {
            if at + 5 > end {
                return Err(invalid());
            }
            let fields = bytes[at + 4] as usize;
            at += 5;
            if at + fields * 3 > end {
                return Err(invalid());
            }
            let timestamp = fields > 0 && bytes[at] == 253 && bytes[at + 1] == 4;
            let mut length = 0usize;
            for field in bytes[at..at + fields * 3].as_chunks::<3>().0 {
                length += field[1] as usize;
            }
            at += fields * 3;
            if head & 0x20 != 0 {
                if at >= end {
                    return Err(invalid());
                }
                let developers = bytes[at] as usize;
                at += 1;
                if at + developers * 3 > end {
                    return Err(invalid());
                }
                for field in bytes[at..at + developers * 3].as_chunks::<3>().0 {
                    length += field[1] as usize;
                }
                at += developers * 3;
            }
            definitions[local] = Some((length, timestamp));
        } else {
            let (length, timestamp) = definitions[local].ok_or_else(invalid)?;
            if compressed && !timestamp {
                return Err(invalid());
            }
            let length = if compressed {
                length.checked_sub(4).ok_or_else(invalid)?
            } else {
                length
            };
            at = at
                .checked_add(length)
                .filter(|v| *v <= end)
                .ok_or_else(invalid)?;
        }
    }
    Ok(())
}
fn field<'a>(record: &'a FitDataRecord, name: &str) -> Option<&'a FitValue> {
    record
        .fields()
        .iter()
        .find(|f| f.name() == name)
        .map(|f| f.value())
}
fn number(record: &FitDataRecord, name: &str) -> Option<f64> {
    field(record, name)
        .and_then(|v| v.clone().try_into().ok())
        .filter(|v: &f64| v.is_finite() && *v >= 0.0)
}
fn timestamp(record: &FitDataRecord, name: &str) -> Option<i64> {
    match field(record, name) {
        Some(FitValue::Timestamp(v)) => Some(v.timestamp()),
        _ => None,
    }
}
fn message(record: &FitDataRecord) -> Result<Value, AppError> {
    let mut result = serde_json::to_value(record)?;
    if let Some(fields) = result["fields"].as_array_mut() {
        for (field, original) in fields.iter_mut().zip(record.fields()) {
            if let FitValue::Timestamp(value) = original.value() {
                field["value"] = json!(value.timestamp());
                field["value_representation"] = json!("unix_seconds_utc");
            }
        }
    }
    Ok(result)
}
pub fn parse(request: &Request) -> Result<HealthRecordInput, AppError> {
    if request.filename.is_empty()
        || request.filename.len() > 255
        || request.filename.contains(['/', '\\'])
        || request.data_base64.len() > 6 * 1024 * 1024
    {
        return Err(AppError::Invalid("Select a named FIT file up to 4 MiB"));
    }
    let bytes = STANDARD
        .decode(&request.data_base64)
        .map_err(|_| AppError::Invalid("Invalid base64 FIT data"))?;
    preflight(&bytes)?;
    let records = fitparser::de::from_bytes_with_options(
        &bytes,
        &[fitparser::de::DecodeOption::KeepCompositeFields]
            .into_iter()
            .collect(),
    )
    .map_err(|_| AppError::Invalid("FIT profile or CRC validation failed"))?;
    if !records.iter().any(|r| {
        r.kind() == MesgNum::FileId && field(r, "type").is_some_and(|v| v.to_string() == "activity")
    }) {
        return Err(AppError::Invalid("A FIT activity file is required"));
    }
    let sessions: Vec<_> = records
        .iter()
        .filter(|r| r.kind() == MesgNum::Session)
        .collect();
    if sessions.len() != 1 {
        return Err(AppError::Invalid("Import one FIT session per file"));
    }
    let session = sessions[0];
    let start = timestamp(session, "start_time")
        .ok_or(AppError::Invalid("FIT session start is missing"))?;
    let end =
        timestamp(session, "timestamp").ok_or(AppError::Invalid("FIT session end is missing"))?;
    if end <= start || i128::from(end) - i128::from(start) > 604800 {
        return Err(AppError::Invalid("FIT session must fit within seven days"));
    }
    let points:Vec<_>=records.iter().filter(|r|r.kind()==MesgNum::Record).map(|r|json!({"at":timestamp(r,"timestamp"),"power_watts":number(r,"power"),"cadence_rpm":number(r,"cadence"),"heart_rate_bpm":number(r,"heart_rate"),"speed_mps":number(r,"enhanced_speed").or_else(||number(r,"speed")),"distance_m":number(r,"distance")})).collect();
    let laps: Vec<_> = records.iter().filter(|r| r.kind() == MesgNum::Lap).map(|r| json!({"start_at":timestamp(r,"start_time"),"end_at":timestamp(r,"timestamp"),"source_total_time_seconds":number(r,"total_timer_time"),"source_distance_m":number(r,"total_distance"),"average_power_watts":number(r,"avg_power"),"average_cadence_rpm":number(r,"avg_cadence"),"average_heart_rate_bpm":number(r,"avg_heart_rate")})).collect();
    let messages = records.iter().map(message).collect::<Result<Vec<_>, _>>()?;
    let hash = hex::encode(Sha256::digest(&bytes));
    let payload = json!({"format":"fit","filename":request.filename,"original_base64":request.data_base64,"sha256":hash,"decoded_messages":messages,"points":points,"laps":laps,"track_points":points.len(),"distance_m":number(session,"total_distance"),"source_total_time_seconds":number(session,"total_timer_time"),"source_elapsed_seconds":number(session,"total_elapsed_time"),"elapsed_seconds":end-start,"algorithm_version":"fitparser-0.11.0-activity-v1","duration_semantics":"source_timer_and_elapsed_are_separate","distance_semantics":"source_session_distance_or_unknown"});
    if serde_json::to_vec(&payload)?.len() > 32 * 1024 * 1024 {
        return Err(AppError::TooLarge);
    }
    Ok(HealthRecordInput {
        record_id: hash,
        source_id: "fit".into(),
        record_type: "workout".into(),
        start_at: start,
        end_at: end,
        version: 1,
        deleted: false,
        payload,
    })
}
impl Database {
    pub async fn import_fit(&self, user: &str, request: Request) -> Result<Value, AppError> {
        let record = self.cpu.run(move || parse(&request)).await?;
        self.import_training_record(user, record).await
    }
}
