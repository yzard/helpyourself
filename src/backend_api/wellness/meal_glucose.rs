//! Time alignment of archived glucose and user-confirmed meals. No causal scoring.
use crate::{database::Database, error::AppError};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub start_at: i64,
    pub end_at: i64,
    pub maximum_gap_seconds: i64,
}
impl Database {
    pub async fn meal_glucose(&self, user: &str, request: Request) -> Result<Value, AppError> {
        if request.end_at <= request.start_at
            || i128::from(request.end_at) - i128::from(request.start_at) > 7 * 86400
        {
            return Err(AppError::Invalid(
                "Select up to seven days for meal alignment",
            ));
        }
        let revision = self.data_revision(user).await?;
        let series = self
            .wellness_series(
                user,
                super::series::SeriesRequest {
                    record_type: "blood_glucose".into(),
                    start_at: request.start_at,
                    end_at: request.end_at,
                    maximum_gap_seconds: request.maximum_gap_seconds,
                    declared_maximum: None,
                },
            )
            .await?;
        let entries = self
            .list_wellness_entries(
                user,
                super::entries::ListRequest {
                    start_at: request.start_at,
                    end_at: request.end_at,
                    kind: None,
                },
            )
            .await?;
        let events: Vec<_> = entries["entries"]
            .as_array()
            .ok_or(AppError::Internal)?
            .iter()
            .filter(|v| matches!(v["entry"]["kind"].as_str(), Some("nutrition" | "training")))
            .cloned()
            .collect();
        if revision != self.data_revision(user).await? {
            return Err(AppError::Conflict(
                "Archive changed; refresh meal alignment",
            ));
        }
        Ok(
            json!({"series":series,"events":events,"data_revision":revision,"algorithm_version":"meal-glucose-alignment-v1","evidence_status":"descriptive","notes":["Events and source measurements share a time axis. Temporal proximity does not establish a food effect.","Sources remain separate. Measurements do not establish that a source is a continuous glucose monitor.","Missing meals, exercise, illness, medication and sensor gaps can affect interpretation. No meal ranking or glucose prediction is generated."]}),
        )
    }
}
