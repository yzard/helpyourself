//! User-owned exercise definitions and scheduled prescriptions, separate from completed training.
use crate::{
    database::{Database, queries},
    error::AppError,
    health::StoredHealthRecord,
};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Exercise {
    pub name: String,
    pub equipment: String,
    pub muscle_groups: Vec<String>,
    pub instructions: String,
    pub note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prescription {
    pub exercise: String,
    pub equipment: String,
    pub sets: u16,
    pub repetitions: Option<u16>,
    pub duration_seconds: Option<u32>,
    pub external_weight_kg: Option<f64>,
    pub rest_seconds: u32,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkoutTemplate {
    pub name: String,
    pub blocks: Vec<Prescription>,
    pub note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedWorkout {
    pub title: String,
    pub duration_minutes: f64,
    pub blocks: Vec<Prescription>,
    pub status: String,
    pub note: String,
}
fn text(v: &str, max: usize, required: bool) -> bool {
    v.len() <= max && (!required || !v.trim().is_empty())
}
fn blocks(items: &[Prescription]) -> bool {
    !items.is_empty()
        && items.len() <= 100
        && items.iter().all(|b| {
            text(&b.exercise, 80, true)
                && text(&b.equipment, 80, true)
                && (1..=100).contains(&b.sets)
                && b.repetitions.is_none_or(|v| (1..=1000).contains(&v))
                && b.duration_seconds.is_none_or(|v| (1..=86400).contains(&v))
                && (b.repetitions.is_some() || b.duration_seconds.is_some())
                && b.external_weight_kg
                    .is_none_or(|v| v.is_finite() && (0.0..=2000.0).contains(&v))
                && b.rest_seconds <= 3600
        })
}
impl Exercise {
    pub fn valid(&self) -> bool {
        text(&self.name, 80, true)
            && text(&self.equipment, 80, true)
            && !self.muscle_groups.is_empty()
            && self.muscle_groups.len() <= 20
            && self.muscle_groups.iter().all(|v| text(v, 80, true))
            && text(&self.instructions, 10000, false)
            && text(&self.note, 2048, false)
    }
}
impl WorkoutTemplate {
    pub fn valid(&self) -> bool {
        text(&self.name, 128, true) && blocks(&self.blocks) && text(&self.note, 2048, false)
    }
}
impl PlannedWorkout {
    pub fn valid(&self) -> bool {
        text(&self.title, 128, true)
            && self.duration_minutes.is_finite()
            && (0.01..=1440.0).contains(&self.duration_minutes)
            && blocks(&self.blocks)
            && ["planned", "skipped"].contains(&self.status.as_str())
            && text(&self.note, 2048, false)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryRequest {
    pub kind: String,
}
impl Database {
    pub async fn wellness_library(
        &self,
        user: &str,
        request: LibraryRequest,
    ) -> Result<Value, AppError> {
        if ![
            "exercise",
            "workout_template",
            "food",
            "recipe",
            "nutrition_goals",
            "reminder",
            "memory",
        ]
        .contains(&request.kind.as_str())
        {
            return Err(AppError::Invalid("Select a supported personal library"));
        }
        let revision = self.data_revision(user).await?;
        let mut bytes = 0usize;
        let mut entries = Vec::new();
        let mut rows = sqlx::query_as::<_, StoredHealthRecord>(queries::WELLNESS_LIBRARY)
            .bind(user)
            .bind(&request.kind)
            .fetch(&self.pool);
        while let Some(row) = rows.try_next().await? {
            bytes = bytes.saturating_add(row.payload_json.len());
            if bytes > 4 * 1024 * 1024 || entries.len() >= 1000 {
                return Err(AppError::TooLarge);
            }
            let payload: Value = serde_json::from_str(&row.payload_json)?;
            entries.push(json!({"record_id":row.record_id,"version":row.version,"at":row.start_at,"timezone":payload["payload"]["timezone"],"entry":payload["payload"]["entry"]}));
        }
        drop(rows);
        if self.data_revision(user).await? != revision {
            return Err(AppError::Conflict("Archive changed; refresh the library"));
        }
        Ok(json!({"entries":entries,"data_revision":revision}))
    }
}
