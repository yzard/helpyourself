//! Public product lookup. Only a reviewed save creates a local food record.
use crate::{authentication::now, error::AppError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, Instant},
};
static LAST_LOOKUP: Mutex<Option<Instant>> = Mutex::new(None);
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub barcode: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalSource {
    pub provider: String,
    pub barcode: String,
    pub retrieved_at: i64,
    pub mass_basis_confirmed: bool,
    pub response: Value,
}
impl ExternalSource {
    pub fn valid(&self) -> bool {
        self.provider == "open_food_facts"
            && barcode(&self.barcode)
            && self.mass_basis_confirmed
            && self.retrieved_at >= 0
            && self.response.is_object()
            && serde_json::to_vec(&self.response).is_ok_and(|v| v.len() <= 512 * 1024)
    }
}
fn barcode(value: &str) -> bool {
    (8..=14).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_digit())
}
pub fn project(code: &str, response: Value, retrieved_at: i64) -> Result<Value, AppError> {
    if !barcode(code) {
        return Err(AppError::Invalid("Enter a barcode with 8 to 14 digits"));
    }
    if response["status"].as_i64() != Some(1) {
        return Err(AppError::NotFound);
    }
    let product = &response["product"];
    if product["code"].as_str().is_some_and(|v| v != code) {
        return Err(AppError::ProviderUnavailable);
    }
    let name = product["product_name"]
        .as_str()
        .filter(|v| !v.trim().is_empty())
        .ok_or(AppError::Invalid("This product has no readable name"))?;
    let mut nutrients = BTreeMap::new();
    for (source, target, factor) in [
        ("energy-kcal_100g", "energy_kcal", 1.0),
        ("proteins_100g", "protein_g", 1.0),
        ("carbohydrates_100g", "carbohydrate_g", 1.0),
        ("fat_100g", "fat_g", 1.0),
        ("fiber_100g", "fiber_g", 1.0),
        ("saturated-fat_100g", "saturated_fat_g", 1.0),
        ("sugars_100g", "sugar_g", 1.0),
        ("sodium_100g", "sodium_mg", 1000.0),
        ("potassium_100g", "potassium_mg", 1000.0),
        ("calcium_100g", "calcium_mg", 1000.0),
        ("iron_100g", "iron_mg", 1000.0),
        ("magnesium_100g", "magnesium_mg", 1000.0),
        ("vitamin-c_100g", "vitamin_c_mg", 1000.0),
        ("vitamin-d_100g", "vitamin_d_ug", 1e6),
        ("vitamin-b12_100g", "vitamin_b12_ug", 1e6),
        ("folates_100g", "folate_ug", 1e6),
    ] {
        if let Some(v) = product["nutriments"][source]
            .as_f64()
            .filter(|v| v.is_finite() && *v >= 0.0 && *v * factor <= 1e7)
        {
            nutrients.insert(target, v * factor);
        }
    }
    if nutrients.is_empty() {
        return Err(AppError::Invalid(
            "This product has no supported numeric nutrients",
        ));
    }
    if serde_json::to_vec(&response)?.len() > 512 * 1024 {
        return Err(AppError::TooLarge);
    }
    Ok(
        json!({"candidate":{"kind":"food","content":{"name":name,"brand":product["brands"].as_str().unwrap_or(""),"barcode":code,"nutrients_per_100g":nutrients,"source":format!("Open Food Facts · https://world.openfoodfacts.org/product/{code} · ODbL"),"note":"Review the product and its label before use.","external_source":{"provider":"open_food_facts","barcode":code,"retrieved_at":retrieved_at,"mass_basis_confirmed":false,"response":response}}},"attribution":"Open Food Facts, Open Database License (ODbL)","state":"requires_label_and_mass_basis_review","notes":["The lookup sends only the barcode to Open Food Facts. It does not upload your archive.","Confirm that these quantities apply to 100 grams. Values per 100 milliliters require a density conversion and are not supported here.","Public product data can contain errors. Correct the fields before saving. The original lookup response remains attached to the saved food."]}),
    )
}
pub async fn lookup(request: Request) -> Result<Value, AppError> {
    if !barcode(&request.barcode) {
        return Err(AppError::Invalid("Enter a barcode with 8 to 14 digits"));
    }
    {
        let mut last = LAST_LOOKUP.lock().map_err(|_| AppError::Internal)?;
        if last.is_some_and(|at| at.elapsed() < Duration::from_secs(1)) {
            return Err(AppError::RateLimited);
        }
        *last = Some(Instant::now());
    }
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .user_agent("Helpyourself/0.1 (https://github.com/yzard/helpyourself)")
        .build()
        .map_err(|_| AppError::Internal)?;
    let mut response=client.get(format!("https://world.openfoodfacts.org/api/v2/product/{}.json",request.barcode)).query(&[("fields","code,product_name,brands,nutriments,nutrition_data_per,quantity,serving_size,last_modified_t")]).send().await.map_err(|_|AppError::ProviderUnavailable)?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(AppError::NotFound);
    }
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(AppError::RateLimited);
    }
    if !response.status().is_success() {
        return Err(AppError::ProviderUnavailable);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AppError::ProviderUnavailable)?
    {
        if bytes.len() + chunk.len() > 512 * 1024 {
            return Err(AppError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    let value = serde_json::from_slice(&bytes).map_err(|_| AppError::ProviderUnavailable)?;
    project(&request.barcode, value, now()?)
}
