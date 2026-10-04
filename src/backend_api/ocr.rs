use crate::{
    config::OcrConfig, error::AppError, provider::Completion, reports::ObservationPayload,
};
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionResponse {
    pub model: String,
    pub engine: String,
    pub prompt_version: String,
    pub content: String,
    pub raw_response_body: String,
    pub observations: Vec<ObservationPayload>,
    pub warnings: Vec<String>,
}

pub async fn extract(
    cpu: &crate::execution::CpuExecutor,
    config: &OcrConfig,
    page: i64,
    image_url: String,
    text_layer: Option<crate::documents::TextLayer>,
) -> Result<Completion, AppError> {
    if !config.enabled {
        return Err(AppError::Invalid("OCR service is disabled"));
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(config.timeout_seconds))
        .build()
        .map_err(|_| AppError::Internal)?;
    let request_body = cpu
        .run_background(move || {
            Ok(serde_json::to_vec(
                &json!({"page":page,"image_url":image_url,"text_layer":text_layer}),
            )?)
        })
        .await?;
    let response = client
        .post(format!(
            "{}/api/v1/documents/extract",
            config.url.trim_end_matches('/')
        ))
        .bearer_auth(&config.api_key)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(request_body)
        .send()
        .await
        .map_err(|_| AppError::ProviderUnavailable)?;
    crate::provider::read_completion(response).await
}

pub async fn probe(config: &OcrConfig) -> Result<(), AppError> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| AppError::Internal)?;
    let result = client
        .get(format!("{}/health", config.url.trim_end_matches('/')))
        .send()
        .await
        .map_err(|_| AppError::ProviderUnavailable)?;
    if !result.status().is_success() {
        return Err(AppError::ProviderUnavailable);
    }
    Ok(())
}
