use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Invalid(&'static str),
    #[error("Authentication required or credentials invalid")]
    Unauthorized,
    #[error("Resource not found")]
    NotFound,
    #[error("{0}")]
    Conflict(&'static str),
    #[error("Request exceeds configured limits")]
    TooLarge,
    #[error("Try again later")]
    RateLimited,
    #[error("Provider temporarily unavailable")]
    ProviderUnavailable,
    #[error("Storage operation failed")]
    Database(#[from] sqlx::Error),
    #[error("File operation failed")]
    Io(#[from] std::io::Error),
    #[error("Internal operation failed")]
    Internal,
}

impl From<serde_json::Error> for AppError {
    fn from(_: serde_json::Error) -> Self {
        Self::Internal
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    code: &'static str,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "conflict"),
            Self::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "too_large"),
            Self::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            Self::ProviderUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "provider_unavailable"),
            Self::Database(_) | Self::Io(_) | Self::Internal => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };
        // Internal errors are deliberately not formatted: driver details can contain private values.
        let message = if status.is_server_error() {
            "Internal operation failed".to_owned()
        } else {
            self.to_string()
        };
        (
            status,
            Json(ErrorBody {
                error: ErrorDetail { code, message },
            }),
        )
            .into_response()
    }
}
