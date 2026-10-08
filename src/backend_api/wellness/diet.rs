//! HEI-2020 descriptive scoring of explicit, confirmed dietary equivalents.
use crate::{database::Database, error::AppError};
use chrono::{DateTime, Days, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub const KEYS: &[&str] = &[
    "energy_kcal",
    "total_fruit_cup_eq",
    "whole_fruit_cup_eq",
    "vegetables_cup_eq",
    "greens_beans_cup_eq",
    "whole_grains_oz_eq",
    "dairy_cup_eq",
    "protein_foods_oz_eq",
    "seafood_plant_oz_eq",
    "unsaturated_fat_g",
    "saturated_fat_g",
    "refined_grains_oz_eq",
    "sodium_mg",
    "added_sugars_tsp_eq",
];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assessment {
    pub totals: BTreeMap<String, f64>,
    pub source: String,
    pub complete_day: bool,
    pub age_two_or_older: bool,
    pub note: String,
}
impl Assessment {
    pub fn valid(&self) -> bool {
        !self.source.trim().is_empty()
            && self.source.len() <= 2048
            && self.note.len() <= 2048
            && self.totals.iter().all(|(k, v)| {
                KEYS.contains(&k.as_str()) && v.is_finite() && (0.0..=1e7).contains(v)
            })
            && [
                ("whole_fruit_cup_eq", "total_fruit_cup_eq"),
                ("greens_beans_cup_eq", "vegetables_cup_eq"),
                ("seafood_plant_oz_eq", "protein_foods_oz_eq"),
            ]
            .iter()
            .all(|(a, b)| match (self.totals.get(*a), self.totals.get(*b)) {
                (Some(a), Some(b)) => a <= b,
                _ => true,
            })
    }
}
pub fn score(totals: &BTreeMap<String, f64>) -> Result<Value, AppError> {
    if totals
        .iter()
        .any(|(k, v)| !KEYS.contains(&k.as_str()) || !v.is_finite() || *v < 0.0)
    {
        return Err(AppError::Invalid("Invalid dietary equivalents"));
    }
    let missing: Vec<_> = KEYS.iter().filter(|k| !totals.contains_key(**k)).collect();
    if !missing.is_empty() || totals.get("energy_kcal").is_none_or(|v| *v <= 0.0) {
        return Ok(
            json!({"state":"incomplete_inputs","missing":missing,"total":null,"components":[]}),
        );
    }
    let energy = totals["energy_kcal"];
    let mut components = Vec::new();
    for (key, maximum, threshold) in [
        ("total_fruit_cup_eq", 5.0, 0.8),
        ("whole_fruit_cup_eq", 5.0, 0.4),
        ("vegetables_cup_eq", 5.0, 1.1),
        ("greens_beans_cup_eq", 5.0, 0.2),
        ("whole_grains_oz_eq", 10.0, 1.5),
        ("dairy_cup_eq", 10.0, 1.3),
        ("protein_foods_oz_eq", 5.0, 2.5),
        ("seafood_plant_oz_eq", 5.0, 0.8),
    ] {
        let density = totals[key] * 1000.0 / energy;
        components.push(json!({"component":key,"value":density,"basis":"per_1000_kcal","maximum":maximum,"score":maximum*(density/threshold).clamp(0.0,1.0)}));
    }
    let saturated = totals["saturated_fat_g"];
    let unsaturated = totals["unsaturated_fat_g"];
    let ratio = (saturated > 0.0).then(|| unsaturated / saturated);
    let fatty = ratio.map_or(if unsaturated > 0.0 { 10.0 } else { 0.0 }, |r| {
        10.0 * ((r - 1.2) / 1.3).clamp(0.0, 1.0)
    });
    components.push(json!({"component":"fatty_acids","value":ratio,"basis":"unsaturated_to_saturated_grams","maximum":10,"score":fatty}));
    for (key, value, low, high, basis) in [
        (
            "refined_grains",
            totals["refined_grains_oz_eq"] * 1000.0 / energy,
            1.8,
            4.3,
            "oz_eq_per_1000_kcal",
        ),
        (
            "sodium",
            totals["sodium_mg"] / energy,
            1.1,
            2.0,
            "grams_per_1000_kcal",
        ),
        (
            "added_sugars",
            totals["added_sugars_tsp_eq"] * 16.0 * 100.0 / energy,
            6.5,
            26.0,
            "percent_energy",
        ),
        (
            "saturated_fat",
            saturated * 9.0 * 100.0 / energy,
            8.0,
            16.0,
            "percent_energy",
        ),
    ] {
        components.push(json!({"component":key,"value":value,"basis":basis,"maximum":10,"score":10.0*((high-value)/(high-low)).clamp(0.0,1.0)}));
    }
    let total: f64 = components.iter().filter_map(|v| v["score"].as_f64()).sum();
    Ok(
        json!({"state":"computed","total":total,"components":components,"algorithm_version":"hei-2020-nci-v1","input_totals":totals,"evidence_status":"descriptive"}),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub date: String,
    pub timezone: String,
}
impl Database {
    pub async fn diet_quality(&self, user: &str, request: Request) -> Result<Value, AppError> {
        let date = NaiveDate::parse_from_str(&request.date, "%Y-%m-%d")
            .ok()
            .filter(|v| v.to_string() == request.date)
            .ok_or(AppError::Invalid("Use a date in YYYY-MM-DD format"))?;
        let tz: chrono_tz::Tz = request
            .timezone
            .parse()
            .map_err(|_| AppError::Invalid("Unknown timezone"))?;
        let midnight = |d: NaiveDate| {
            tz.from_local_datetime(&d.and_hms_opt(0, 0, 0).ok_or(AppError::Internal)?)
                .earliest()
                .map(|v| v.timestamp())
                .ok_or(AppError::Invalid("No midnight on this date"))
        };
        let start = date
            .checked_sub_days(Days::new(27))
            .ok_or(AppError::Invalid("Invalid date"))?;
        let revision = self.data_revision(user).await?;
        let data = self
            .list_wellness_entries(
                user,
                super::entries::ListRequest {
                    start_at: midnight(start)?,
                    end_at: midnight(date.succ_opt().ok_or(AppError::Invalid("Invalid date"))?)?,
                    kind: Some("diet_quality".into()),
                },
            )
            .await?;
        let mut days: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for entry in data["entries"].as_array().ok_or(AppError::Internal)? {
            if entry["timezone"] != request.timezone {
                continue;
            }
            let day = DateTime::from_timestamp(entry["at"].as_i64().ok_or(AppError::Internal)?, 0)
                .ok_or(AppError::Internal)?
                .with_timezone(&tz)
                .date_naive();
            days.entry(day.to_string()).or_default().push(entry);
        }
        let mut windows = Vec::new();
        for length in [1u64, 7, 28] {
            let lower = date
                .checked_sub_days(Days::new(length - 1))
                .ok_or(AppError::Invalid("Invalid date"))?;
            let mut totals: BTreeMap<String, f64> = BTreeMap::new();
            let mut inputs = Vec::new();
            let mut complete = 0;
            let mut conflicts = 0;
            for (day, entries) in days.range(lower.to_string()..=date.to_string()) {
                if entries.len() != 1 {
                    conflicts += 1;
                    continue;
                }
                let entry = entries[0];
                let a: Assessment = serde_json::from_value(entry["entry"]["content"].clone())?;
                inputs.push(json!({"date":day,"record_id":entry["record_id"],"version":entry["version"],"source":a.source}));
                if !a.complete_day
                    || !a.age_two_or_older
                    || KEYS.iter().any(|k| !a.totals.contains_key(*k))
                    || a.totals["energy_kcal"] <= 0.0
                {
                    continue;
                }
                complete += 1;
                for (k, v) in a.totals {
                    *totals.entry(k).or_default() += v;
                }
            }
            let result = if complete == length {
                score(&totals)?
            } else {
                json!({"state":"incomplete_days","total":null,"components":[]})
            };
            windows.push(json!({"days":length,"complete_days":complete,"conflicting_days":conflicts,"inputs":inputs,"result":result}));
        }
        if revision != self.data_revision(user).await? {
            return Err(AppError::Conflict(
                "Archive changed; refresh the diet assessment",
            ));
        }
        Ok(
            json!({"date":request.date,"timezone":request.timezone,"windows":windows,"data_revision":revision,"algorithm_version":"hei-2020-nci-v1","notes":["Use confirmed full-day dietary equivalents for people aged two or older. Ordinary portion weights are not food-pattern equivalents.","Include legumes in both vegetable and protein totals under HEI-2020. Do not substitute total sugar for added sugar.","All days must be complete. Multi-day scoring sums inputs before calculating ratios. Conflicting assessments stop that window.","The score describes recorded diet composition. It does not estimate personal disease risk or lifespan."]}),
        )
    }
}
