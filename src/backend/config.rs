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
    pub server: ServerConfig,
    pub security: SecurityConfig,
    pub storage: StorageConfig,
    pub jobs: JobConfig,
    pub providers: ProviderConfigs,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub listen: SocketAddr,
    pub data_dir: PathBuf,
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
pub struct ProviderConfigs {
    pub ocr: ProviderConfig,
    pub analysis: ProviderConfig,
    pub document_parser: Option<ProviderConfig>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub adapter: String,
    pub timeout_seconds: u64,
    pub api_key_file: Option<PathBuf>,
    pub extra_body: Option<serde_json::Map<String, serde_json::Value>>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, AppError> {
        let path = path.canonicalize()?;
        let content = std::fs::read_to_string(&path)?;
        let mut configuration: Self = toml::from_str(&content).map_err(|_| {
            AppError::Invalid("Invalid TOML configuration; check schema and field types")
        })?;
        let directory = path
            .parent()
            .ok_or(AppError::Invalid("Invalid configuration path"))?;
        configuration.server.data_dir = resolve_path(directory, &configuration.server.data_dir)?;
        for provider in [
            &mut configuration.providers.ocr,
            &mut configuration.providers.analysis,
        ] {
            if let Some(path) = &provider.api_key_file {
                provider.api_key_file = Some(resolve_path(directory, path)?);
            }
        }
        if let Some(provider) = &mut configuration.providers.document_parser
            && let Some(path) = &provider.api_key_file
        {
            provider.api_key_file = Some(resolve_path(directory, path)?);
        }
        configuration.validate()?;
        Ok(configuration)
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if self.server.data_dir.as_os_str().is_empty() {
            return Err(AppError::Invalid("server.data_dir must not be empty"));
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
        for provider in [&self.providers.ocr, &self.providers.analysis]
            .into_iter()
            .chain(self.providers.document_parser.iter())
        {
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
                || !["unlimited_ocr", "openai_chat"].contains(&provider.adapter.as_str())
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
        if self.providers.ocr.enabled
            && self.providers.ocr.adapter == "unlimited_ocr"
            && !self
                .providers
                .document_parser
                .as_ref()
                .is_some_and(|provider| provider.enabled && provider.adapter == "openai_chat")
        {
            return Err(AppError::Invalid(
                "Unlimited-OCR requires an enabled OpenAI-compatible document_parser for structured extraction",
            ));
        }
        if self.providers.analysis.enabled && self.providers.analysis.adapter != "openai_chat" {
            return Err(AppError::Invalid("Analysis requires openai_chat adapter"));
        }
        Ok(())
    }
}

fn resolve_path(directory: &Path, path: &Path) -> Result<PathBuf, AppError> {
    if path.as_os_str().is_empty() {
        return Err(AppError::Invalid("Configuration paths must not be empty"));
    }
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        directory.join(path)
    })
}
