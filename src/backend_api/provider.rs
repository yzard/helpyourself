use crate::{config::ProviderConfig, error::AppError};
use serde_json::{Value, json};
use std::time::Duration;

pub struct Completion {
    pub response_body: String,
    pub status_code: u16,
}
impl Completion {
    pub fn text(&self) -> Result<String, AppError> {
        if self.status_code == 429 || self.status_code >= 500 {
            return Err(AppError::ProviderUnavailable);
        }
        if !(200..300).contains(&self.status_code) {
            return Err(AppError::Invalid(
                "Provider returned an unsuccessful status",
            ));
        }
        let response: Value = serde_json::from_str(&self.response_body)
            .map_err(|_| AppError::Invalid("Provider did not return JSON"))?;
        response
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(AppError::Invalid("Provider returned no text content"))
    }
}

pub async fn complete(provider: &ProviderConfig, messages: Value) -> Result<Completion, AppError> {
    for attempt in 0..3 {
        match complete_once(provider, messages.clone()).await {
            Ok(reply) if (reply.status_code == 429 || reply.status_code >= 500) && attempt < 2 => {
                tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
            }
            Err(AppError::ProviderUnavailable) if attempt < 2 => {
                tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
            }
            result => return result,
        }
    }
    Err(AppError::ProviderUnavailable)
}

async fn complete_once(provider: &ProviderConfig, messages: Value) -> Result<Completion, AppError> {
    if !provider.enabled {
        return Err(AppError::Invalid("Provider is disabled"));
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(provider.timeout_seconds))
        .build()
        .map_err(|_| AppError::Internal)?;
    let mut body = provider.extra_body.clone().unwrap_or_default();
    body.insert("model".into(), json!(provider.model));
    body.insert("messages".into(), messages);
    body.insert("stream".into(), json!(false));
    let address = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );
    let mut request = client.post(address).json(&body);
    if !provider.api_key.is_empty() {
        request = request.bearer_auth(&provider.api_key);
    }
    let response = request
        .send()
        .await
        .map_err(|_| AppError::ProviderUnavailable)?;
    read_completion(response).await
}

pub async fn read_completion(mut response: reqwest::Response) -> Result<Completion, AppError> {
    let status_code = response.status().as_u16();
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| AppError::ProviderUnavailable)?
    {
        if bytes.len() + chunk.len() > 8 * 1024 * 1024 {
            return Err(AppError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(Completion {
        status_code,
        response_body: String::from_utf8(bytes)
            .map_err(|_| AppError::Invalid("Provider did not return UTF-8"))?,
    })
}

pub fn parse_json<T: serde::de::DeserializeOwned>(content: &str) -> Result<T, AppError> {
    let content = content.trim();
    let content = if let Some(without_fence) = content
        .strip_prefix("```json")
        .or_else(|| content.strip_prefix("```"))
    {
        without_fence
            .trim()
            .strip_suffix("```")
            .ok_or(AppError::Invalid("Incomplete JSON fence"))?
            .trim()
    } else {
        content
    };
    serde_json::from_str(content)
        .map_err(|_| AppError::Invalid("Model output does not match the required schema"))
}

pub async fn probe(provider: &ProviderConfig) -> Result<(), AppError> {
    let output=complete(provider,json!([{"role":"user","content":"Reply with the word ready. This is synthetic capability-test input."}])).await?;
    if output.text()?.trim().is_empty() {
        return Err(AppError::Invalid("Empty provider response"));
    }
    Ok(())
}
