//! Planned meals are separate from consumed food. Shopping quantities use recipe snapshots.
use crate::{database::Database, error::AppError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MealPlan {
    pub recipe: super::food::Recipe,
    pub planned_servings: f64,
    pub status: String,
}
impl MealPlan {
    pub fn valid(&self) -> bool {
        self.recipe.valid()
            && self.planned_servings.is_finite()
            && (0.01..=10000.0).contains(&self.planned_servings)
            && ["planned", "skipped"].contains(&self.status.as_str())
    }
}
pub fn shopping(entries: &[Value]) -> Result<Vec<Value>, AppError> {
    let mut groups: BTreeMap<String, Value> = BTreeMap::new();
    for entry in entries {
        let plan: MealPlan = serde_json::from_value(entry["entry"]["content"].clone())?;
        if !plan.valid() {
            return Err(AppError::Invalid("Invalid meal plan"));
        }
        if plan.status != "planned" {
            continue;
        }
        for ingredient in &plan.recipe.ingredients {
            let key = serde_json::to_string(&json!([
                ingredient.food_name,
                ingredient.source,
                ingredient.nutrients_per_100g
            ]))?;
            let quantity = ingredient.grams * plan.planned_servings / plan.recipe.servings;
            let group=groups.entry(key).or_insert_with(||json!({"food":ingredient.food_name,"source":ingredient.source,"grams":0.0,"inputs":[]}));
            group["grams"] = json!(group["grams"].as_f64().ok_or(AppError::Internal)? + quantity);
            group["inputs"].as_array_mut().ok_or(AppError::Internal)?.push(json!({"record_id":entry["record_id"],"version":entry["version"],"grams":quantity}));
        }
    }
    Ok(groups.into_values().collect())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub start_at: i64,
    pub end_at: i64,
}
impl Database {
    pub async fn meal_plans(&self, user: &str, request: Request) -> Result<Value, AppError> {
        let revision = self.data_revision(user).await?;
        let data = self
            .list_wellness_entries(
                user,
                super::entries::ListRequest {
                    start_at: request.start_at,
                    end_at: request.end_at,
                    kind: Some("meal_plan".into()),
                },
            )
            .await?;
        let entries = data["entries"].as_array().ok_or(AppError::Internal)?;
        let groceries = shopping(entries)?;
        if revision != self.data_revision(user).await? {
            return Err(AppError::Conflict("Archive changed; refresh meal plans"));
        }
        Ok(
            json!({"plans":entries,"shopping":groceries,"data_revision":revision,"algorithm_version":"meal-plan-v1","notes":["Planned food does not count as consumed food. Record actual portions separately.","Shopping quantities use the recipe snapshot and planned servings. Sources and different nutrient snapshots remain separate.","The list does not subtract pantry stock or infer package sizes."]}),
        )
    }
}
