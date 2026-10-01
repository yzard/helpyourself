use crate::{
    app::AppState,
    error::AppError,
    models::{LoginRequest, LoginResponse, User},
};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::{extract::FromRequestParts, http::request::Parts};
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Semaphore;

pub fn now() -> Result<i64, AppError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AppError::Internal)?
        .as_secs()
        .try_into()
        .map_err(|_| AppError::Internal)
}

pub fn digest(secret: &[u8]) -> String {
    hex::encode(Sha256::digest(secret))
}

pub fn normalize_username(username: &str) -> Result<String, AppError> {
    let username = username.trim().to_ascii_lowercase();
    if !(3..=64).contains(&username.len())
        || !username
            .bytes()
            .all(|character| character.is_ascii_alphanumeric() || b"._-@".contains(&character))
    {
        return Err(AppError::Invalid(
            "Username must be 3-64 ASCII letters, digits or . _ - @",
        ));
    }
    Ok(username)
}

pub async fn hash_password(password: String) -> Result<String, AppError> {
    if !(12..=1024).contains(&password.len()) {
        return Err(AppError::Invalid("Password must be 12-1024 bytes"));
    }
    tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
            .map(|hashed| hashed.to_string())
            .map_err(|_| AppError::Internal)
    })
    .await
    .map_err(|_| AppError::Internal)?
}

pub async fn login(state: &AppState, request: LoginRequest) -> Result<LoginResponse, AppError> {
    let username = normalize_username(&request.username).map_err(|_| AppError::Unauthorized)?;
    if request.password.len() > 1024 {
        return Err(AppError::Unauthorized);
    }
    let permit = Arc::clone(&state.password_checks)
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    let current_time = now()?;
    state
        .database
        .reserve_login(
            &digest(username.as_bytes()),
            current_time,
            state.config.security.login_window_seconds,
            state.config.security.login_attempts_per_window,
        )
        .await?;
    let credentials = state.database.credentials(&username).await?;
    let password_hash = credentials
        .as_ref()
        .map(|record| record.password_hash.clone())
        .unwrap_or_else(|| state.dummy_password_hash.clone());
    let matches = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let parsed = PasswordHash::new(&password_hash).map_err(|_| AppError::Internal)?;
        Ok::<bool, AppError>(
            Argon2::default()
                .verify_password(request.password.as_bytes(), &parsed)
                .is_ok(),
        )
    })
    .await
    .map_err(|_| AppError::Internal)??;
    if !matches {
        return Err(AppError::Unauthorized);
    }
    let credentials = credentials.ok_or(AppError::Unauthorized)?;
    let mut random = [0u8; 32];
    OsRng.fill_bytes(&mut random);
    let token = hex::encode(random);
    let expires_at = current_time + state.config.security.session_ttl_seconds;
    state
        .database
        .create_session(
            &digest(token.as_bytes()),
            expires_at,
            &credentials,
            current_time,
        )
        .await?;
    Ok(LoginResponse {
        token,
        expires_at,
        user: User {
            user_id: credentials.user_id,
            username: credentials.username,
        },
    })
}

pub struct CurrentUser {
    pub user: User,
    pub token_hash: String,
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|header| header.to_str().ok())
            .and_then(|header| header.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;
        if token.len() != 64 || !token.bytes().all(|character| character.is_ascii_hexdigit()) {
            return Err(AppError::Unauthorized);
        }
        let token_hash = digest(token.as_bytes());
        let user = state.database.session_user(&token_hash, now()?).await?;
        Ok(Self { user, token_hash })
    }
}

pub fn password_semaphore(maximum: usize) -> Arc<Semaphore> {
    Arc::new(Semaphore::new(maximum))
}
