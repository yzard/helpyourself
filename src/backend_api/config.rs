use crate::error::AppError;
use serde::Deserialize;
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
};

pub const TEMPLATE: &str = include_str!("config.toml");

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(skip)]
    pub data_dir: PathBuf,
    pub server: ServerConfig,
    pub security: SecurityConfig,
    pub storage: StorageConfig,
    pub jobs: JobConfig,
    pub ocr: OcrConfig,
    pub providers: ProviderConfigs,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub listen: SocketAddr,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityConfig {
    pub session_ttl_seconds: i64,
    pub login_window_seconds: i64,
    pub login_attempts_per_window: i64,
    pub maximum_password_checks: usize,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    pub maximum_upload_bytes: usize,
    pub maximum_pdf_pages: usize,
    pub maximum_image_pixels: u64,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobConfig {
    pub lease_seconds: i64,
    pub maximum_attempts: i64,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OcrConfig {
    pub enabled: bool,
    pub url: String,
    pub timeout_seconds: u64,
    pub api_key: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfigs {
    pub analysis: ProviderConfig,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub adapter: String,
    pub timeout_seconds: u64,
    pub api_key: String,
    pub extra_body: Option<serde_json::Map<String, serde_json::Value>>,
}

impl Config {
    pub fn load(data_dir: &Path) -> Result<Self, AppError> {
        if !data_dir.is_absolute() {
            return Err(AppError::Invalid(
                "--data-dir must be an absolute directory",
            ));
        }
        let directory = data_dir.canonicalize()?;
        let content = std::fs::read_to_string(directory.join("config.toml"))?;
        let mut configuration: Self = toml::from_str(&content).map_err(|_| {
            AppError::Invalid("Invalid TOML configuration; check schema and field types")
        })?;
        configuration.data_dir = directory;
        configuration.validate()?;
        Ok(configuration)
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if !self.data_dir.is_absolute() {
            return Err(AppError::Invalid(
                "--data-dir must be an absolute directory",
            ));
        }
        if !(60..=31_536_000).contains(&self.security.session_ttl_seconds)
            || !(1..=86_400).contains(&self.security.login_window_seconds)
            || !(1..=1000).contains(&self.security.login_attempts_per_window)
            || !(1..=32).contains(&self.security.maximum_password_checks)
        {
            return Err(AppError::Invalid("Invalid security limits"));
        }
        if !(1024..=104_857_600).contains(&self.storage.maximum_upload_bytes)
            || !(1..=1000).contains(&self.storage.maximum_pdf_pages)
            || !(1..=100_000_000).contains(&self.storage.maximum_image_pixels)
        {
            return Err(AppError::Invalid("Invalid storage limits"));
        }
        if !(1..=86_400).contains(&self.jobs.lease_seconds)
            || !(1..=20).contains(&self.jobs.maximum_attempts)
        {
            return Err(AppError::Invalid("Invalid job limits"));
        }
        validate_url(&self.ocr.url)?;
        if !(1..=3600).contains(&self.ocr.timeout_seconds) {
            return Err(AppError::Invalid("Invalid OCR service timeout"));
        }
        if self.ocr.enabled || !self.ocr.api_key.is_empty() {
            validate_key(&self.ocr.api_key, 24)?;
        }
        {
            let provider = &self.providers.analysis;
            if !provider.api_key.is_empty() {
                validate_key(&provider.api_key, 1)?;
            }
            let address = url::Url::parse(&provider.base_url)
                .map_err(|_| AppError::Invalid("Invalid provider URL"))?;
            if !["http", "https"].contains(&address.scheme())
                || address.host_str().is_none()
                || !address.username().is_empty()
                || address.password().is_some()
                || address.query().is_some()
                || address.fragment().is_some()
            {
                return Err(AppError::Invalid(
                    "Provider URL must be HTTP(S) without credentials, query or fragment",
                ));
            }
            if provider.model.trim().is_empty()
                || provider.adapter != "openai_chat"
                || !(1..=3600).contains(&provider.timeout_seconds)
            {
                return Err(AppError::Invalid(
                    "Invalid provider model, adapter or timeout",
                ));
            }
            if provider.extra_body.as_ref().is_some_and(|fields| {
                fields
                    .keys()
                    .any(|key| ["model", "messages", "stream"].contains(&key.as_str()))
            }) {
                return Err(AppError::Invalid(
                    "extra_body cannot override model, messages or stream",
                ));
            }
        }
        Ok(())
    }
}

fn validate_key(key: &str, minimum: usize) -> Result<(), AppError> {
    if !(minimum..=8192).contains(&key.len()) || !key.bytes().all(|byte| (33..=126).contains(&byte))
    {
        return Err(AppError::Invalid("Invalid service API key in TOML"));
    }
    Ok(())
}

fn validate_url(value: &str) -> Result<(), AppError> {
    let address =
        url::Url::parse(value).map_err(|_| AppError::Invalid("Invalid OCR service URL"))?;
    if !["http", "https"].contains(&address.scheme())
        || address.host_str().is_none()
        || !address.username().is_empty()
        || address.password().is_some()
        || address.query().is_some()
        || address.fragment().is_some()
    {
        return Err(AppError::Invalid(
            "OCR URL must be HTTP(S) without credentials, query or fragment",
        ));
    }
    Ok(())
}
