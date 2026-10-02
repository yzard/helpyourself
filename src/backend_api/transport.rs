//! Bounded Health ingress and large JSON serialization off the network runtime.
use crate::{app::AppState, error::AppError, health::SyncRequest};
use axum::{
    body::{Body, Bytes},
    extract::{FromRequest, Request},
    http::header,
    response::{IntoResponse, Response},
};
use tokio::sync::OwnedSemaphorePermit;

pub struct HealthSyncBody {
    pub(crate) batch: crate::health_batch::PreparedBatch,
    _admission: OwnedSemaphorePermit,
}
impl FromRequest<AppState> for HealthSyncBody {
    type Rejection = AppError;
    async fn from_request(request: Request, state: &AppState) -> Result<Self, AppError> {
        let admission = state
            .health_slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| AppError::RateLimited)?;
        let content_type = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if content_type != "application/json"
            && !(content_type.starts_with("application/") && content_type.ends_with("+json"))
        {
            return Err(AppError::Invalid("JSON content type required"));
        }
        let bytes = Bytes::from_request(request, state).await.map_err(|error| {
            if error.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
                AppError::TooLarge
            } else {
                AppError::Invalid("Invalid request body")
            }
        })?;
        let batch = state
            .database
            .cpu
            .run(move || {
                let request: SyncRequest = serde_json::from_slice(&bytes)
                    .map_err(|_| AppError::Invalid("Invalid health JSON"))?;
                crate::health_batch::prepare(request)
            })
            .await?;
        Ok(Self {
            batch,
            _admission: admission,
        })
    }
}
pub async fn json_response(
    state: &AppState,
    value: serde_json::Value,
) -> Result<Response, AppError> {
    let bytes = state
        .database
        .cpu
        .run(move || Ok(serde_json::to_vec(&value)?))
        .await?;
    Ok((
        [(header::CONTENT_TYPE, "application/json")],
        Body::from(bytes),
    )
        .into_response())
}
