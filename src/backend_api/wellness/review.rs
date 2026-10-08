//! Descriptive reviews of manual records. Missing values never become zero.
use crate::{authentication::now, database::Database, error::AppError};
use chrono::DateTime;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRequest {
    pub start_at: i64,
    pub end_at: i64,
    pub timezone: String,
}
impl Database {
    pub async fn wellness_review(
        &self,
        user: &str,
        request: ReviewRequest,
    ) -> Result<Value, AppError> {
        let timezone: chrono_tz::Tz = request
            .timezone
            .parse()
            .map_err(|_| AppError::Invalid("Unknown timezone"))?;
        let revision = self.data_revision(user).await?;
        let input = self
            .list_wellness_entries(
                user,
                super::entries::ListRequest {
                    start_at: request.start_at,
                    end_at: request.end_at,
                    kind: None,
                },
            )
            .await?;
        let entries = input["entries"].as_array().ok_or(AppError::Internal)?;
        let training =
            super::training::summarize(entries, request.start_at, request.end_at, timezone)?;
        let mut groups: BTreeMap<(String, String), Vec<&Value>> = BTreeMap::new();
        let mut best: BTreeMap<(String, u64), Value> = BTreeMap::new();
        let mut measurements = Vec::new();
        for entry in entries {
            let date = DateTime::from_timestamp(entry["at"].as_i64().ok_or(AppError::Internal)?, 0)
                .ok_or(AppError::Invalid("Entry timestamp cannot be displayed"))?
                .with_timezone(&timezone)
                .date_naive()
                .to_string();
            let kind = entry["entry"]["kind"].as_str().ok_or(AppError::Internal)?;
            groups.entry((date, kind.into())).or_default().push(entry);
            if kind == "training" {
                for (index, set) in entry["entry"]["content"]["sets"]
                    .as_array()
                    .ok_or(AppError::Internal)?
                    .iter()
                    .enumerate()
                {
                    let key = (
                        set["exercise"]
                            .as_str()
                            .ok_or(AppError::Internal)?
                            .to_string(),
                        set["repetitions"].as_u64().ok_or(AppError::Internal)?,
                    );
                    let weight = set["external_weight_kg"]
                        .as_f64()
                        .ok_or(AppError::Internal)?;
                    if best.get(&key).is_none_or(|b| {
                        b["external_weight_kg"].as_f64().is_some_and(|v| weight > v)
                    }) {
                        best.insert(key.clone(),json!({"exercise":key.0,"repetitions":key.1,"external_weight_kg":weight,"record_id":entry["record_id"],"version":entry["version"],"at":entry["at"],"set_index":index}));
                    }
                }
            }
            if matches!(kind, "body" | "blood_pressure" | "cycle" | "journal") {
                measurements.push(entry.clone());
            }
        }
        let mut days = Vec::new();
        for ((date, kind), records) in groups {
            let fields: &[(&str, &str)] = match kind.as_str() {
                "nutrition" => &[
                    ("energy_kcal", "kcal"),
                    ("protein_g", "g"),
                    ("carbohydrate_g", "g"),
                    ("fat_g", "g"),
                    ("fiber_g", "g"),
                    ("water_ml", "mL"),
                ],
                "training" => &[("duration_minutes", "minutes"), ("session_load_au", "AU")],
                "breathing" => &[("duration_minutes", "minutes")],
                _ => &[],
            };
            let bases: BTreeSet<_> = records
                .iter()
                .filter_map(|r| r["calculation"]["duration_basis"].as_str())
                .collect();
            let compatible = kind != "training" || bases.len() == 1;
            let mut totals = Vec::new();
            for (field, unit) in fields {
                let mut sum = 0.0;
                let mut known = 0;
                for record in &records {
                    let value = if *field == "session_load_au" {
                        &record["calculation"][field]
                    } else {
                        &record["entry"]["content"][field]
                    };
                    if let Some(value) = value.as_f64() {
                        sum += value;
                        known += 1;
                    }
                }
                totals.push(json!({"metric":field,"unit":unit,"duration_bases":bases,"protocol_state":if compatible {"compatible"} else {"incompatible_duration_protocols"},"observed_sum":if known>0 && compatible {Some(sum)} else {None},"known_count":known,"missing_count":records.len()-known,"all_logged_values_known":known==records.len()}));
            }
            let evidence: Vec<_> = records
                .iter()
                .map(|r| json!({"record_id":r["record_id"],"version":r["version"]}))
                .collect();
            days.push(json!({"date":date,"kind":kind,"record_count":records.len(),"totals":totals,"inputs":evidence}));
        }
        if self.data_revision(user).await? != revision {
            return Err(AppError::Conflict("Archive changed; refresh the review"));
        }
        Ok(
            json!({"start_at":request.start_at,"end_at":request.end_at,"timezone":request.timezone,"days":days,"training":training,"measurements":measurements,"strength_bests":best.into_values().collect::<Vec<_>>(),"source":"manual:helpyourself","data_revision":revision,"algorithm_version":"manual-review-v1","computed_at":now()?,"evidence_status":"descriptive","notes":["Totals include recorded entries only. Complete fields do not establish that a day is fully logged.","Days with no records are absent, not zero. Device estimates are not added to manual totals.","Strength bests are the largest recorded external weight at each exercise and repetition count within this window. They are not predicted one-repetition maxima.","Blood pressure values remain paired. Cycle dates are observations, not fertility predictions."]}),
        )
    }
}
