//! User-authored domain entries reuse the archive's revisions, exports and deletion semantics.
use crate::{
    database::{Database, queries},
    error::AppError,
    health::{ConnectionRequest, HealthRecordInput, StoredHealthRecord, SyncRequest},
};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const INSTALLATION: &str = "ac0a4800-ef02-4b92-b8ae-b4e0ebea01b1";

#[derive(Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "content",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Entry {
    Journal(Journal),
    Memory(super::coach::Memory),
    Reminder(super::reminders::Reminder),
    Training(Training),
    Exercise(super::planning::Exercise),
    WorkoutTemplate(super::planning::WorkoutTemplate),
    PlannedWorkout(super::planning::PlannedWorkout),
    TrainingDay(TrainingDay),
    SleepCorrection(SleepCorrection),
    Nutrition(Nutrition),
    MealPlan(super::meals::MealPlan),
    DietQuality(super::diet::Assessment),
    Food(super::food::Food),
    Recipe(super::food::Recipe),
    NutritionGoals(super::food::Goals),
    Cycle(Cycle),
    CyclePrediction(CyclePrediction),
    Body(Body),
    BloodPressure(BloodPressure),
    Breathing(Breathing),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub behaviors: BTreeMap<String, bool>,
    pub measurements: BTreeMap<String, BehaviorMeasurement>,
    pub times: BTreeMap<String, u16>,
    pub mood: Option<u8>,
    pub perceived_stress: Option<u8>,
    pub note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorMeasurement {
    pub value: f64,
    pub unit: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SleepCorrection {
    pub source: String,
    pub session_start: i64,
    pub session_end: i64,
    pub classification: String,
    pub corrected_asleep_minutes: Option<f64>,
    pub basis_revisions: BTreeMap<String, i64>,
    pub note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrainingDay {
    pub status: String,
    pub note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Training {
    pub activity: String,
    pub duration_minutes: f64,
    pub rpe_cr10: Option<f64>,
    pub ended_at: i64,
    pub paused_minutes: f64,
    pub duration_basis: String,
    pub rpe_answered_at: Option<i64>,
    pub sets: Vec<StrengthSet>,
    pub note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrengthSet {
    pub exercise: String,
    pub repetitions: u16,
    pub external_weight_kg: f64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Nutrition {
    pub food: String,
    pub meal: String,
    pub energy_kcal: Option<f64>,
    pub protein_g: Option<f64>,
    pub carbohydrate_g: Option<f64>,
    pub fat_g: Option<f64>,
    pub fiber_g: Option<f64>,
    pub water_ml: Option<f64>,
    pub micronutrients: BTreeMap<String, f64>,
    pub origin: Option<Box<super::food::Origin>>,
    pub note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cycle {
    pub flow: String,
    pub symptoms: Vec<String>,
    pub context: String,
    pub note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CyclePrediction {
    pub start_date: String,
    pub end_date: String,
    pub generated_at: i64,
    pub source: String,
    pub uncertainty: String,
    pub note: String,
}
impl CyclePrediction {
    pub fn valid(&self) -> bool {
        let date = |s: &str| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()
                .filter(|d| d.to_string() == s)
        };
        let dates = match (date(&self.start_date), date(&self.end_date)) {
            (Some(start), Some(end)) => end >= start && (end - start).num_days() <= 366,
            _ => false,
        };
        dates
            && chrono::DateTime::from_timestamp(self.generated_at, 0).is_some()
            && self.generated_at >= 0
            && text(&self.source, true)
            && text(&self.uncertainty, true)
            && text(&self.note, false)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Body {
    pub weight_kg: f64,
    pub body_fat_percent: Option<f64>,
    pub waist_cm: Option<f64>,
    pub note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BloodPressure {
    pub systolic_mmhg: f64,
    pub diastolic_mmhg: f64,
    pub pulse_bpm: Option<f64>,
    pub posture: String,
    pub arm: String,
    pub note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Breathing {
    pub duration_minutes: f64,
    pub breaths_per_minute: Option<f64>,
    pub note: String,
}
fn bounded(value: f64, low: f64, high: f64) -> bool {
    value.is_finite() && value >= low && value <= high
}
fn optional(value: Option<f64>, low: f64, high: f64) -> bool {
    value.is_none_or(|v| bounded(v, low, high))
}
fn text(value: &str, required: bool) -> bool {
    value.len() <= 2048 && (!required || !value.trim().is_empty())
}
impl Entry {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Journal(_) => "journal",
            Self::Memory(_) => "memory",
            Self::Reminder(_) => "reminder",
            Self::Training(_) => "training",
            Self::Exercise(_) => "exercise",
            Self::WorkoutTemplate(_) => "workout_template",
            Self::PlannedWorkout(_) => "planned_workout",
            Self::TrainingDay(_) => "training_day",
            Self::SleepCorrection(_) => "sleep_correction",
            Self::Nutrition(_) => "nutrition",
            Self::MealPlan(_) => "meal_plan",
            Self::DietQuality(_) => "diet_quality",
            Self::Food(_) => "food",
            Self::Recipe(_) => "recipe",
            Self::NutritionGoals(_) => "nutrition_goals",
            Self::Cycle(_) => "cycle",
            Self::CyclePrediction(_) => "cycle_prediction",
            Self::Body(_) => "body",
            Self::BloodPressure(_) => "blood_pressure",
            Self::Breathing(_) => "breathing",
        }
    }
    pub fn validate(&self) -> Result<(), AppError> {
        let valid = match self {
            Self::Memory(v) => v.valid(),
            Self::Reminder(v) => v.valid(),
            Self::Food(v) => v.valid(),
            Self::Recipe(v) => v.valid(),
            Self::NutritionGoals(v) => v.valid(),
            Self::DietQuality(v) => v.valid(),
            Self::MealPlan(v) => v.valid(),
            Self::CyclePrediction(v) => v.valid(),
            Self::Exercise(v) => v.valid(),
            Self::WorkoutTemplate(v) => v.valid(),
            Self::PlannedWorkout(v) => v.valid(),
            Self::Journal(v) => {
                v.behaviors.len() + v.measurements.len() + v.times.len() <= 64
                    && v.measurements.iter().all(|(key, value)| {
                        text(key, true)
                            && key.len() <= 80
                            && bounded(value.value, -1e9, 1e9)
                            && !value.unit.trim().is_empty()
                            && value.unit.len() <= 32
                            && !v.behaviors.contains_key(key)
                            && !v.times.contains_key(key)
                    })
                    && v.times.iter().all(|(key, minute)| {
                        text(key, true)
                            && key.len() <= 80
                            && *minute < 1440
                            && !v.behaviors.contains_key(key)
                    })
                    && v.behaviors.keys().all(|k| text(k, true) && k.len() <= 80)
                    && v.mood.is_none_or(|x| x <= 10)
                    && v.perceived_stress.is_none_or(|x| x <= 10)
                    && text(&v.note, false)
            }
            Self::SleepCorrection(v) => {
                let duration = i128::from(v.session_end) - i128::from(v.session_start);
                text(&v.source, true)
                    && v.source.len() <= 256
                    && duration > 0
                    && duration <= 7 * 86400
                    && chrono::DateTime::from_timestamp(v.session_start, 0).is_some()
                    && chrono::DateTime::from_timestamp(v.session_end, 0).is_some()
                    && ["main_sleep", "nap", "unclassified_session"]
                        .contains(&v.classification.as_str())
                    && optional(v.corrected_asleep_minutes, 0.0, duration as f64 / 60.0)
                    && !v.basis_revisions.is_empty()
                    && v.basis_revisions.len() <= 1000
                    && v.basis_revisions
                        .iter()
                        .all(|(id, version)| !id.is_empty() && id.len() <= 256 && *version > 0)
                    && text(&v.note, true)
            }
            Self::TrainingDay(v) => {
                ["rest", "all_sessions_logged"].contains(&v.status.as_str()) && text(&v.note, false)
            }
            Self::Training(v) => {
                text(&v.activity, true)
                    && bounded(v.duration_minutes, 0.01, 1440.0)
                    && optional(v.rpe_cr10, 0.0, 10.0)
                    && bounded(v.paused_minutes, 0.0, 1440.0)
                    && ["elapsed_including_pauses", "active_excluding_pauses"]
                        .contains(&v.duration_basis.as_str())
                    && (v.duration_basis != "elapsed_including_pauses"
                        || v.paused_minutes <= v.duration_minutes)
                    && chrono::DateTime::from_timestamp(v.ended_at, 0).is_some()
                    && v.rpe_cr10.is_some() == v.rpe_answered_at.is_some()
                    && v.rpe_answered_at.is_none_or(|at| {
                        at >= v.ended_at && chrono::DateTime::from_timestamp(at, 0).is_some()
                    })
                    && v.sets.len() <= 500
                    && v.sets.iter().all(|s| {
                        text(&s.exercise, true)
                            && s.exercise.len() <= 80
                            && (1..=1000).contains(&s.repetitions)
                            && bounded(s.external_weight_kg, 0.0, 2000.0)
                    })
                    && text(&v.note, false)
            }
            Self::Nutrition(v) => {
                text(&v.food, true)
                    && text(&v.meal, true)
                    && optional(v.energy_kcal, 0.0, 30000.0)
                    && [v.protein_g, v.carbohydrate_g, v.fat_g, v.fiber_g]
                        .iter()
                        .all(|v| optional(*v, 0.0, 10000.0))
                    && optional(v.water_ml, 0.0, 30000.0)
                    && super::food::valid_nutrients(&v.micronutrients)
                    && v.micronutrients.keys().all(|k| {
                        ![
                            "energy_kcal",
                            "protein_g",
                            "carbohydrate_g",
                            "fat_g",
                            "fiber_g",
                            "water_ml",
                        ]
                        .contains(&k.as_str())
                    })
                    && v.origin.as_ref().is_none_or(|origin| origin.valid())
                    && text(&v.note, false)
            }
            Self::Cycle(v) => {
                ["none", "spotting", "light", "medium", "heavy"].contains(&v.flow.as_str())
                    && [
                        "cycle",
                        "pregnancy",
                        "postpartum",
                        "perimenopause",
                        "unknown",
                    ]
                    .contains(&v.context.as_str())
                    && v.symptoms.len() <= 32
                    && v.symptoms.iter().all(|s| text(s, true))
                    && text(&v.note, false)
            }
            Self::Body(v) => {
                bounded(v.weight_kg, 0.1, 1000.0)
                    && optional(v.body_fat_percent, 0.0, 100.0)
                    && optional(v.waist_cm, 1.0, 500.0)
                    && text(&v.note, false)
            }
            Self::BloodPressure(v) => {
                bounded(v.systolic_mmhg, 20.0, 400.0)
                    && bounded(v.diastolic_mmhg, 10.0, 300.0)
                    && v.systolic_mmhg > v.diastolic_mmhg
                    && optional(v.pulse_bpm, 10.0, 400.0)
                    && ["left", "right", "unknown"].contains(&v.arm.as_str())
                    && ["seated", "standing", "supine", "unknown"].contains(&v.posture.as_str())
                    && text(&v.note, false)
            }
            Self::Breathing(v) => {
                bounded(v.duration_minutes, 0.01, 180.0)
                    && optional(v.breaths_per_minute, 1.0, 60.0)
                    && text(&v.note, false)
            }
        };
        if !valid {
            return Err(AppError::Invalid("Invalid entry fields or units"));
        }
        Ok(())
    }
    pub fn calculation(&self) -> Result<Value, AppError> {
        self.validate()?;
        Ok(match self {
            Self::Training(v) => {
                let mut volume = BTreeMap::<&str, f64>::new();
                for set in &v.sets {
                    *volume.entry(&set.exercise).or_default() +=
                        f64::from(set.repetitions) * set.external_weight_kg;
                }
                json!({"algorithm_version":"training-descriptive-v1", "session_load_au":v.rpe_cr10.map(|rpe| super::algorithms::session_load(rpe, v.duration_minutes)).transpose()?, "rpe_protocol":"whole_session_cr10", "rpe_delay_minutes":v.rpe_answered_at.map(|at|(i128::from(at)-i128::from(v.ended_at)) as f64/60.0), "duration_basis":v.duration_basis, "paused_minutes":v.paused_minutes, "external_volume_kg_repetitions_by_exercise":volume, "evidence_status":"descriptive", "note":"Volume excludes body mass and is comparable only within the same exercise and equipment."})
            }
            Self::Recipe(v) => super::food::recipe_totals(v)?,
            _ => json!({"evidence_status":"user_reported", "algorithm_version":"manual-entry-v1"}),
        })
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveRequest {
    pub record_id: String,
    pub version: i64,
    pub batch_id: String,
    pub at: i64,
    pub timezone: String,
    pub entry: Entry,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListRequest {
    pub start_at: i64,
    pub end_at: i64,
    pub kind: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteRequest {
    pub record_id: String,
    pub version: i64,
    pub batch_id: String,
    pub kind: String,
}
fn valid_kind(kind: &str) -> bool {
    [
        "journal",
        "memory",
        "reminder",
        "training",
        "exercise",
        "workout_template",
        "planned_workout",
        "training_day",
        "sleep_correction",
        "nutrition",
        "food",
        "recipe",
        "nutrition_goals",
        "diet_quality",
        "meal_plan",
        "cycle",
        "cycle_prediction",
        "body",
        "blood_pressure",
        "breathing",
    ]
    .contains(&kind)
}
impl Database {
    pub(crate) async fn manual_connection(&self, user: &str) -> Result<String, AppError> {
        Ok(self
            .create_connection(
                user,
                ConnectionRequest {
                    platform: "manual".into(),
                    installation_id: INSTALLATION.into(),
                },
            )
            .await?
            .connection_id)
    }
    pub async fn save_wellness_entry(
        &self,
        user: &str,
        request: SaveRequest,
    ) -> Result<Value, AppError> {
        request.entry.validate()?;
        if let Entry::SleepCorrection(correction) = &request.entry
            && request.at != correction.session_start
        {
            return Err(AppError::Invalid(
                "A sleep correction timestamp must equal its session start",
            ));
        }
        if request.version < 1
            || uuid::Uuid::parse_str(&request.batch_id).is_err()
            || uuid::Uuid::parse_str(&request.record_id).is_err()
            || request.timezone.parse::<chrono_tz::Tz>().is_err()
            || chrono::DateTime::from_timestamp(request.at, 0).is_none()
        {
            return Err(AppError::Invalid(
                "Invalid entry identifier, timestamp or timezone",
            ));
        }
        if let Entry::Training(training) = &request.entry {
            let elapsed = (i128::from(training.ended_at) - i128::from(request.at)) as f64 / 60.0;
            let expected = training.duration_minutes
                + if training.duration_basis == "active_excluding_pauses" {
                    training.paused_minutes
                } else {
                    0.0
                };
            if elapsed <= 0.0 || elapsed > 1440.0 || (elapsed - expected).abs() > 1.0 / 60.0 {
                return Err(AppError::Invalid(
                    "Training start, end, duration and pauses must agree within one second",
                ));
            }
            let now = crate::authentication::now()?;
            if training.ended_at > now || training.rpe_answered_at.is_some_and(|at| at > now) {
                return Err(AppError::Invalid(
                    "Completed sessions and effort answers cannot be in the future",
                ));
            }
        }
        let kind = request.entry.kind();
        let payload = json!({"entry":request.entry,"timezone":request.timezone,"schema_version":1});
        let connection_id = self.manual_connection(user).await?;
        let result = self
            .sync_health(
                user,
                SyncRequest {
                    connection_id,
                    batch_id: request.batch_id,
                    record_type: kind.into(),
                    coverage_status: "observed".into(),
                    records: vec![HealthRecordInput {
                        record_id: request.record_id.clone(),
                        source_id: "helpyourself".into(),
                        record_type: kind.into(),
                        start_at: request.at,
                        end_at: request.at,
                        version: request.version,
                        deleted: false,
                        payload,
                    }],
                },
            )
            .await?;
        Ok(json!({"sync":result,"record_id":request.record_id}))
    }
    pub async fn list_wellness_entries(
        &self,
        user: &str,
        request: ListRequest,
    ) -> Result<Value, AppError> {
        if request.end_at <= request.start_at
            || i128::from(request.end_at) - i128::from(request.start_at) > 366 * 86400
            || request.kind.as_ref().is_some_and(|k| !valid_kind(k))
        {
            return Err(AppError::Invalid(
                "Select a valid entry type and a range of at most 366 days",
            ));
        }
        let mut rows = Vec::<StoredHealthRecord>::new();
        let mut bytes = 0_usize;
        let mut stream = sqlx::query_as::<_, StoredHealthRecord>(queries::WELLNESS_ENTRIES)
            .bind(user)
            .bind(request.start_at)
            .bind(request.end_at)
            .bind(&request.kind)
            .bind(&request.kind)
            .fetch(&self.pool);
        while let Some(row) = stream.try_next().await? {
            bytes = bytes.saturating_add(row.payload_json.len());
            if bytes > 4 * 1024 * 1024 || rows.len() >= 1000 {
                return Err(AppError::Invalid(
                    "Too many entries; select a shorter range",
                ));
            }
            rows.push(row);
        }
        drop(stream);
        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            let raw: Value = serde_json::from_str(&row.payload_json)?;
            let entry: Entry = serde_json::from_value(raw["payload"]["entry"].clone())
                .map_err(|_| AppError::Conflict("Stored entry format is unsupported"))?;
            entries.push(json!({"record_id":row.record_id,"version":row.version,"at":row.start_at,"source":"manual:helpyourself","timezone":raw["payload"]["timezone"],"entry":entry,"calculation":entry.calculation()?}));
        }
        Ok(json!({"entries":entries,"missing_is_zero":false}))
    }
    pub async fn delete_wellness_entry(
        &self,
        user: &str,
        request: DeleteRequest,
    ) -> Result<Value, AppError> {
        if request.version < 1
            || uuid::Uuid::parse_str(&request.batch_id).is_err()
            || !valid_kind(&request.kind)
            || uuid::Uuid::parse_str(&request.record_id).is_err()
        {
            return Err(AppError::Invalid("Invalid entry identifier or kind"));
        }
        let connection_id = self.manual_connection(user).await?;
        self.sync_health(
            user,
            SyncRequest {
                connection_id,
                batch_id: request.batch_id,
                record_type: request.kind.clone(),
                coverage_status: "observed".into(),
                records: vec![HealthRecordInput {
                    record_id: request.record_id,
                    source_id: "helpyourself".into(),
                    record_type: request.kind,
                    start_at: 0,
                    end_at: 0,
                    version: request.version,
                    deleted: true,
                    payload: json!({}),
                }],
            },
        )
        .await
    }
}
