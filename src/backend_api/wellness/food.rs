//! Local food data, ingredient snapshots, and explicit portions. No missing nutrient becomes zero.
use crate::{
    database::{Database, queries},
    error::AppError,
    health::StoredHealthRecord,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub const NUTRIENTS: &[(&str, &str)] = &[
    ("energy_kcal", "kcal"),
    ("protein_g", "g"),
    ("carbohydrate_g", "g"),
    ("fat_g", "g"),
    ("fiber_g", "g"),
    ("water_ml", "mL"),
    ("saturated_fat_g", "g"),
    ("sugar_g", "g"),
    ("sodium_mg", "mg"),
    ("potassium_mg", "mg"),
    ("calcium_mg", "mg"),
    ("iron_mg", "mg"),
    ("magnesium_mg", "mg"),
    ("vitamin_c_mg", "mg"),
    ("vitamin_d_ug", "µg"),
    ("vitamin_b12_ug", "µg"),
    ("folate_ug", "µg"),
];
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Food {
    pub external_source: Option<super::food_lookup::ExternalSource>,
    pub name: String,
    pub brand: String,
    pub barcode: Option<String>,
    pub nutrients_per_100g: BTreeMap<String, f64>,
    pub source: String,
    pub note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ingredient {
    pub food_name: String,
    pub grams: f64,
    pub nutrients_per_100g: BTreeMap<String, f64>,
    pub source: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub name: String,
    pub servings: f64,
    pub ingredients: Vec<Ingredient>,
    pub instructions: String,
    pub note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Goals {
    pub daily_targets: BTreeMap<String, f64>,
    pub note: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortionBasis {
    pub record_id: String,
    pub version: i64,
    pub amount: f64,
    pub unit: String,
}
#[derive(Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "content",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Snapshot {
    Food(Food),
    Recipe(Recipe),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub basis: PortionBasis,
    pub source_snapshot: Snapshot,
}
impl Origin {
    pub fn valid(&self) -> bool {
        self.basis.valid()
            && match &self.source_snapshot {
                Snapshot::Food(v) => v.valid() && self.basis.unit == "g",
                Snapshot::Recipe(v) => v.valid() && self.basis.unit == "servings",
            }
    }
}
fn text(s: &str, max: usize, required: bool) -> bool {
    s.len() <= max && (!required || !s.trim().is_empty())
}
pub fn valid_nutrients(values: &BTreeMap<String, f64>) -> bool {
    values.iter().all(|(key, v)| {
        NUTRIENTS.iter().any(|(name, _)| key == name) && v.is_finite() && (0.0..=1e7).contains(v)
    })
}
impl Food {
    pub fn valid(&self) -> bool {
        self.external_source.as_ref().is_none_or(|v| v.valid())
            && text(&self.name, 256, true)
            && text(&self.brand, 256, false)
            && text(&self.source, 2048, true)
            && text(&self.note, 2048, false)
            && self.barcode.as_ref().is_none_or(|v| {
                (8..=14).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_digit())
            })
            && !self.nutrients_per_100g.is_empty()
            && valid_nutrients(&self.nutrients_per_100g)
    }
}
impl Recipe {
    pub fn valid(&self) -> bool {
        text(&self.name, 256, true)
            && self.servings.is_finite()
            && (0.01..=10000.0).contains(&self.servings)
            && !self.ingredients.is_empty()
            && self.ingredients.len() <= 200
            && self.ingredients.iter().all(|v| {
                text(&v.food_name, 256, true)
                    && text(&v.source, 2048, true)
                    && v.grams.is_finite()
                    && (0.01..=100000.0).contains(&v.grams)
                    && valid_nutrients(&v.nutrients_per_100g)
            })
            && text(&self.instructions, 10000, false)
            && text(&self.note, 2048, false)
    }
}
impl Goals {
    pub fn valid(&self) -> bool {
        !self.daily_targets.is_empty()
            && valid_nutrients(&self.daily_targets)
            && text(&self.note, 2048, false)
    }
}
impl PortionBasis {
    pub fn valid(&self) -> bool {
        uuid::Uuid::parse_str(&self.record_id).is_ok()
            && self.version > 0
            && self.amount.is_finite()
            && (0.01..=100000.0).contains(&self.amount)
            && ["g", "servings"].contains(&self.unit.as_str())
    }
}
pub fn recipe_totals(recipe: &Recipe) -> Result<Value, AppError> {
    if !recipe.valid() {
        return Err(AppError::Invalid("Invalid recipe or units"));
    }
    let keys: BTreeSet<_> = recipe
        .ingredients
        .iter()
        .flat_map(|i| i.nutrients_per_100g.keys())
        .collect();
    let mut totals = serde_json::Map::new();
    for key in keys {
        let mut sum = 0.0;
        let mut known = 0;
        for ingredient in &recipe.ingredients {
            if let Some(v) = ingredient.nutrients_per_100g.get(key) {
                sum += v * ingredient.grams / 100.0;
                known += 1;
            }
        }
        totals.insert(key.clone(),json!({"observed_total":sum,"total":if known==recipe.ingredients.len(){Some(sum)}else{None},"per_serving":if known==recipe.ingredients.len(){Some(sum/recipe.servings)}else{None},"known_ingredients":known,"missing_ingredients":recipe.ingredients.len()-known}));
    }
    Ok(
        json!({"nutrients":totals,"servings":recipe.servings,"algorithm_version":"recipe-snapshot-v1"}),
    )
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortionRequest {
    pub basis: PortionBasis,
    pub meal: String,
}
impl Database {
    pub async fn food_portion(
        &self,
        user: &str,
        request: PortionRequest,
    ) -> Result<Value, AppError> {
        if !request.basis.valid()
            || !["breakfast", "lunch", "dinner", "snack", "drink"].contains(&request.meal.as_str())
        {
            return Err(AppError::Invalid(
                "Select an explicit food portion and meal",
            ));
        }
        let revision = self.data_revision(user).await?;
        let row = sqlx::query_as::<_, StoredHealthRecord>(queries::FOOD_PORTION)
            .bind(user)
            .bind(&request.basis.record_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)?;
        if row.version != request.basis.version {
            return Err(AppError::Conflict(
                "Food changed; review the current revision",
            ));
        }
        let envelope: Value = serde_json::from_str(&row.payload_json)?;
        let entry: super::entries::Entry =
            serde_json::from_value(envelope["payload"]["entry"].clone())?;
        let (name, nutrients, missing) = match entry {
            super::entries::Entry::Food(food) if request.basis.unit == "g" => {
                if !food.valid() {
                    return Err(AppError::Internal);
                }
                (
                    food.name,
                    food.nutrients_per_100g
                        .into_iter()
                        .map(|(k, v)| (k, v * request.basis.amount / 100.0))
                        .collect::<BTreeMap<_, _>>(),
                    Vec::new(),
                )
            }
            super::entries::Entry::Recipe(recipe) if request.basis.unit == "servings" => {
                let total = recipe_totals(&recipe)?;
                let mut known = BTreeMap::new();
                let mut missing = Vec::new();
                for (key, value) in total["nutrients"].as_object().ok_or(AppError::Internal)? {
                    if let Some(v) = value["per_serving"].as_f64() {
                        known.insert(key.clone(), v * request.basis.amount);
                    } else {
                        missing.push(key.clone());
                    }
                }
                (recipe.name, known, missing)
            }
            _ => {
                return Err(AppError::Invalid(
                    "Food portions use grams; recipe portions use servings",
                ));
            }
        };
        if self.data_revision(user).await? != revision {
            return Err(AppError::Conflict("Archive changed; review the portion"));
        }
        Ok(
            json!({"food":name,"meal":request.meal,"nutrients":nutrients,"incomplete_nutrients":missing,"basis":request.basis,"source_snapshot":envelope["payload"]["entry"],"data_revision":revision,"note":"Review this calculated portion before saving a food log. Missing values remain unknown."}),
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DayRequest {
    pub date: String,
    pub timezone: String,
}
impl Database {
    pub async fn nutrition_day(&self, user: &str, request: DayRequest) -> Result<Value, AppError> {
        use chrono::TimeZone;
        let zone: chrono_tz::Tz = request
            .timezone
            .parse()
            .map_err(|_| AppError::Invalid("Unknown timezone"))?;
        let date = chrono::NaiveDate::parse_from_str(&request.date, "%Y-%m-%d")
            .map_err(|_| AppError::Invalid("Invalid date"))?;
        if date.to_string() != request.date {
            return Err(AppError::Invalid("Use YYYY-MM-DD"));
        }
        let boundary = |date: chrono::NaiveDate| {
            zone.from_local_datetime(&date.and_hms_opt(0, 0, 0).ok_or(AppError::Internal)?)
                .earliest()
                .map(|v| v.timestamp())
                .ok_or(AppError::Invalid("No midnight on this date"))
        };
        let start = boundary(date)?;
        let end = boundary(date.succ_opt().ok_or(AppError::Invalid("Date overflow"))?)?;
        let revision = self.data_revision(user).await?;
        let input = self
            .list_wellness_entries(
                user,
                super::entries::ListRequest {
                    start_at: start,
                    end_at: end,
                    kind: Some("nutrition".into()),
                },
            )
            .await?;
        let goals = self
            .wellness_library(
                user,
                super::planning::LibraryRequest {
                    kind: "nutrition_goals".into(),
                },
            )
            .await?;
        let applicable: Vec<_> = goals["entries"]
            .as_array()
            .ok_or(AppError::Internal)?
            .iter()
            .filter(|g| g["at"].as_i64().is_some_and(|at| at < end))
            .collect();
        let goal = applicable.first().copied();
        let conflict =
            goal.is_some_and(|first| applicable.iter().skip(1).any(|g| g["at"] == first["at"]));
        let records = input["entries"].as_array().ok_or(AppError::Internal)?;
        let summary = daily_nutrients(records, if conflict { None } else { goal });
        if self.data_revision(user).await? != revision {
            return Err(AppError::Conflict("Archive changed; refresh nutrition"));
        }
        Ok(
            json!({"date":request.date,"timezone":request.timezone,"nutrients":summary,"records":records,"goal":if conflict {None}else{goal},"goal_state":if conflict{"conflicting_goals"}else if goal.is_some(){"user_defined"}else{"not_set"},"data_revision":revision,"notes":["Totals cover recorded foods only. A complete set of nutrient values does not establish a fully logged day.","Targets are user-defined, not medical recommendations. Earlier goal records remain in the archive.","Amounts copied from food labels remain editable; the original portion snapshot stays attached."]}),
        )
    }
}
pub fn daily_nutrients(records: &[Value], goal: Option<&Value>) -> Value {
    json!(NUTRIENTS.iter().map(|(key,unit)|{
    let mut sum=0.0;let mut known=0;
    for record in records {let content=&record["entry"]["content"];if let Some(value)=content[*key].as_f64().or_else(||content["micronutrients"][*key].as_f64()){sum+=value;known+=1;}}
    let target=goal.and_then(|g|g["entry"]["content"]["daily_targets"][*key].as_f64());
    json!({"nutrient":key,"unit":unit,"observed_sum":if known>0{Some(sum)}else{None},"known_records":known,"missing_records":records.len()-known,"target":target,"recorded_fraction_of_target":if known==records.len()&&known>0 {target.filter(|v|*v>0.0).map(|v|sum/v)}else{None}})
 }).collect::<Vec<_>>())
}
