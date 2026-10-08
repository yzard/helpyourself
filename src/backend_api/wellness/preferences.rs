//! Versioned preferences stay with the user's exportable archive.
use crate::{
    database::{Database, queries},
    error::AppError,
    health::{HealthRecordInput, SyncRequest},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet};
const RECORD: &str = "a37f17a5-867c-4f39-b864-76ffdd87953d";
const SOURCE: &str = "helpyourself.preferences";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preferences {
    pub source_priority: BTreeMap<String, Vec<String>>,
    pub favorite_metrics: Vec<String>,
    pub sleep_target_minutes: Option<u16>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavePreferences {
    pub expected_version: i64,
    pub batch_id: String,
    pub preferences: Preferences,
}
impl Preferences {
    fn validate(&self) -> Result<(), AppError> {
        super::validate_priorities(&self.source_priority)?;
        if self.favorite_metrics.len() > super::KINDS.len()
            || self
                .favorite_metrics
                .iter()
                .any(|m| !super::KINDS.contains(&m.as_str()))
            || self.favorite_metrics.iter().collect::<BTreeSet<_>>().len()
                != self.favorite_metrics.len()
            || self
                .sleep_target_minutes
                .is_some_and(|m| m == 0 || m > 1440)
        {
            return Err(AppError::Invalid("Invalid metric layout or sleep target"));
        }
        Ok(())
    }
}
impl Database {
    pub async fn wellness_preferences(&self, user: &str) -> Result<Value, AppError> {
        let row = sqlx::query(queries::HEALTH_RECORD)
            .bind(user)
            .bind("manual")
            .bind(SOURCE)
            .bind(RECORD)
            .fetch_optional(&self.pool)
            .await?;
        if let Some(row) = row {
            let raw: Value = serde_json::from_str(&row.get::<String, _>("payload_json"))?;
            let preferences: Preferences = serde_json::from_value(raw["payload"].clone())
                .map_err(|_| AppError::Conflict("Stored preferences are invalid"))?;
            preferences.validate()?;
            Ok(json!({"version":row.get::<i64,_>("version"),"preferences":preferences}))
        } else {
            Ok(
                json!({"version":0,"preferences":Preferences {source_priority:BTreeMap::new(),favorite_metrics:vec![],sleep_target_minutes:None}}),
            )
        }
    }
    pub async fn save_wellness_preferences(
        &self,
        user: &str,
        request: SavePreferences,
    ) -> Result<Value, AppError> {
        request.preferences.validate()?;
        let version = request
            .expected_version
            .checked_add(1)
            .filter(|v| *v > 0)
            .ok_or(AppError::Invalid("Invalid preference version"))?;
        if uuid::Uuid::parse_str(&request.batch_id).is_err() {
            return Err(AppError::Invalid("Invalid preference request ID"));
        }
        let connection_id = self.manual_connection(user).await?;
        self.sync_health(
            user,
            SyncRequest {
                connection_id,
                batch_id: request.batch_id,
                record_type: "preferences".into(),
                coverage_status: "observed".into(),
                records: vec![HealthRecordInput {
                    record_id: RECORD.into(),
                    source_id: SOURCE.into(),
                    record_type: "preferences".into(),
                    start_at: 0,
                    end_at: 0,
                    version,
                    deleted: false,
                    payload: serde_json::to_value(&request.preferences)?,
                }],
            },
        )
        .await?;
        self.wellness_preferences(user).await
    }
}
