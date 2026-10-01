use serde::{Deserialize, Serialize};

#[derive(Serialize, sqlx::FromRow)]
pub struct User {
    pub user_id: String,
    pub username: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_at: i64,
    pub user: User,
}

#[derive(Clone, Serialize, sqlx::FromRow)]
pub struct ArchivedFile {
    pub file_id: String,
    pub upload_id: String,
    pub original_name: String,
    pub relative_path: String,
    pub processing_path: Option<String>,
    pub processing_sha256: Option<String>,
    pub content_type: String,
    pub sha256: String,
    pub byte_count: i64,
    pub page_count: i64,
    pub created_at: i64,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct Job {
    pub job_id: String,
    pub file_id: String,
    pub kind: String,
    pub status: String,
    pub attempt_count: i64,
    pub error_code: Option<String>,
    pub created_at: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRequest {
    pub file_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRequest {
    pub job_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListRequest {
    pub after_id: Option<String>,
    pub limit: i64,
}

impl ListRequest {
    pub fn validate(&self) -> Result<(), crate::error::AppError> {
        if !(1..=100).contains(&self.limit) {
            return Err(crate::error::AppError::Invalid(
                "limit must be between 1 and 100",
            ));
        }
        Ok(())
    }
}
