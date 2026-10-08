//! Source record views. Device classifications remain distinct from project calculations.
use crate::{
    database::{Database, queries},
    error::AppError,
    health::StoredHealthRecord,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cursor {
    pub platform: String,
    pub source_id: String,
    pub record_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListRequest {
    pub kind: String,
    pub after: Option<Cursor>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetRequest {
    pub platform: String,
    pub source_id: String,
    pub record_id: String,
    pub sample_offset: usize,
}
impl Database {
    pub async fn specialty_records(
        &self,
        user: &str,
        request: ListRequest,
    ) -> Result<Value, AppError> {
        if !["ecg", "clinical", "blood_pressure"].contains(&request.kind.as_str()) {
            return Err(AppError::Invalid(
                "Select ECG, clinical records or blood pressure",
            ));
        }
        let a = request.after;
        let rows = sqlx::query(queries::SPECIALTY_LIST)
            .bind(user)
            .bind(&request.kind)
            .bind(&request.kind)
            .bind(&request.kind)
            .bind(a.as_ref().map(|v| &v.platform))
            .bind(a.as_ref().map(|v| &v.platform))
            .bind(a.as_ref().map(|v| &v.source_id))
            .bind(a.as_ref().map(|v| &v.record_id))
            .fetch_all(&self.pool)
            .await?;
        let more = rows.len() > 50;
        let items:Vec<_>=rows.into_iter().take(50).map(|r|json!({"platform":r.get::<String,_>("platform"),"source_id":r.get::<String,_>("source_id"),"record_id":r.get::<String,_>("record_id"),"record_type":r.get::<String,_>("record_type"),"at":r.get::<i64,_>("start_at"),"version":r.get::<i64,_>("version")})).collect();
        let cursor = if more {
            items.last().map(|v|json!({"platform":v["platform"],"source_id":v["source_id"],"record_id":v["record_id"]}))
        } else {
            None
        };
        Ok(json!({"records":items,"next_cursor":cursor}))
    }
    pub async fn specialty_record(
        &self,
        user: &str,
        request: GetRequest,
    ) -> Result<Value, AppError> {
        if request.sample_offset > 1000000 {
            return Err(AppError::Invalid("Sample offset is too large"));
        }
        let revision = self.data_revision(user).await?;
        let row = sqlx::query_as::<_, StoredHealthRecord>(queries::SPECIALTY_GET)
            .bind(user)
            .bind(&request.platform)
            .bind(&request.source_id)
            .bind(&request.record_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)?;
        let record_id = row.record_id.clone();
        let platform = row.platform.clone();
        let source = row.source_id.clone();
        let mut result = self
            .cpu
            .run(move || project_record(row, request.sample_offset))
            .await?;
        let ids = result["related_sample_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut related = Vec::new();
        if ids.len() > 1000 {
            return Err(AppError::TooLarge);
        }
        for id in ids {
            let Some(id) = id.as_str() else { continue };
            let rows = sqlx::query(queries::RELATED_RECORDS)
                .bind(user)
                .bind(&platform)
                .bind(&source)
                .bind(id)
                .fetch_all(&self.pool)
                .await?;
            let items:Vec<_>=rows.into_iter().map(|r|json!({"record_id":id,"record_type":r.get::<String,_>("record_type"),"version":r.get::<i64,_>("version"),"at":r.get::<i64,_>("start_at"),"value":r.get::<Option<f64>,_>("value"),"unit":r.get::<Option<String>,_>("unit")})).collect();
            related.push(json!({"record_id":id,"state":if items.is_empty(){"missing"}else{"source_record"},"records":items}));
        }
        if self.data_revision(user).await? != revision {
            return Err(AppError::Conflict("Archive changed; refresh the record"));
        }
        result["related"] = json!(related);
        result["record_id"] = json!(record_id);
        result["data_revision"] = json!(revision);
        Ok(result)
    }
}
pub fn project_record(row: StoredHealthRecord, offset: usize) -> Result<Value, AppError> {
    let envelope: Value = serde_json::from_str(&row.payload_json)?;
    let p = &envelope["payload"];
    let training_points: Vec<_> = if p["format"] == "tcx-2" {
        p["laps"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|lap| lap["tracks"].as_array().into_iter().flatten())
            .flat_map(|track| track.as_array().into_iter().flatten())
            .cloned()
            .collect()
    } else {
        p["points"].as_array().cloned().unwrap_or_default()
    };
    let series = if row.record_type == "workout" {
        Some(&training_points)
    } else {
        p["series"].as_array()
    };
    let laps: Vec<_> = p["laps"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|lap| {
            let mut summary = lap.clone();
            if let Some(object) = summary.as_object_mut() {
                object.remove("tracks");
            }
            summary
        })
        .collect();
    let count = series.map_or(0, Vec::len);
    let samples: Vec<_> = series
        .into_iter()
        .flatten()
        .skip(offset)
        .take(5000)
        .cloned()
        .collect();
    let fhir = if let Some(encoded) = p["fhir"]["data_base64"].as_str() {
        if encoded.len() > 8 * 1024 * 1024 {
            json!({"state":"too_large_for_view","resource":null})
        } else {
            match STANDARD
                .decode(encoded)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            {
                Some(value) => json!({"state":"source_resource","resource":value}),
                None => json!({"state":"invalid_source_json","resource":null}),
            }
        }
    } else {
        json!({"state":"not_present","resource":null})
    };
    Ok(
        json!({"platform":row.platform,"source_id":row.source_id,"record_type":row.record_type,"version":row.version,"start_at":row.start_at,"end_at":row.end_at,"filename":p["filename"],"laps":laps,"distance_m":p["distance_m"],"source_total_time_seconds":p["source_total_time_seconds"],"elapsed_seconds":p["elapsed_seconds"],"source_name":p["source_name"],"source_classification":p["ecg_classification"],"sampling_frequency_hz":p["sampling_frequency_hz"],"fhir":fhir,"related_sample_ids":p["related_sample_ids"],"sample_offset":offset,"sample_count":count,"samples":samples,"next_sample_offset":if offset.saturating_add(5000)<count {Some(offset+5000)}else{None},"notes":["Classification belongs to the source device. This application does not diagnose rhythm.","Samples retain source timestamps and volts. Missing values remain gaps.","Clinical resources are source records, not instructions. Original payloads remain in the full export.","Related records resolve only within this account and this exact source."]}),
    )
}
