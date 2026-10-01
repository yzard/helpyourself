pub(crate) mod queries;

use crate::{
    error::AppError,
    models::{ArchivedFile, Job, ListRequest, User},
};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone)]
pub struct Database {
    pub(crate) pool: SqlitePool,
    pub(crate) data_dir: PathBuf,
}

#[derive(sqlx::FromRow)]
pub struct Credentials {
    pub user_id: String,
    pub username: String,
    pub password_hash: String,
    pub credential_version: i64,
}

impl Database {
    pub async fn open(directory: &Path) -> Result<Self, AppError> {
        tokio::fs::create_dir_all(directory).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            tokio::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700)).await?;
        }
        let options = SqliteConnectOptions::new()
            .filename(directory.join("database.sqlite"))
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await?;
        let version: i64 = sqlx::query_scalar(queries::SCHEMA_VERSION)
            .fetch_one(&mut *transaction)
            .await?;
        if version != 0 && version != 6 {
            return Err(AppError::Conflict(
                "Unsupported database schema; use a fresh data directory",
            ));
        }
        if version == 0 {
            sqlx::raw_sql(include_str!("schema.sql"))
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query(queries::SET_SCHEMA_VERSION)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        for folder in [
            "tmp",
            "raw/apple_health",
            "raw/google_health",
            "raw/photos",
            "raw/documents",
        ] {
            tokio::fs::create_dir_all(directory.join(folder)).await?;
        }
        Ok(Self {
            pool,
            data_dir: directory.to_owned(),
        })
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }

    pub async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        now: i64,
    ) -> Result<User, AppError> {
        let user = User {
            user_id: uuid::Uuid::new_v4().to_string(),
            username: username.to_owned(),
        };
        let inserted = sqlx::query(queries::CREATE_USER)
            .bind(&user.user_id)
            .bind(username)
            .bind(password_hash)
            .bind(now)
            .execute(&self.pool)
            .await;
        match inserted {
            Err(sqlx::Error::Database(error)) if error.is_unique_violation() => {
                return Err(AppError::Conflict("Username already exists"));
            }
            outcome => {
                outcome?;
            }
        }
        Ok(user)
    }

    pub async fn credentials(&self, username: &str) -> Result<Option<Credentials>, AppError> {
        Ok(sqlx::query_as(queries::CREDENTIALS)
            .bind(username)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn change_credentials(
        &self,
        username: &str,
        password_hash: Option<&str>,
    ) -> Result<(), AppError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let user_id: Option<String> = if let Some(password_hash) = password_hash {
            sqlx::query_scalar(queries::RESET_PASSWORD)
                .bind(password_hash)
                .bind(username)
                .fetch_optional(&mut *transaction)
                .await?
        } else {
            sqlx::query_scalar(queries::DISABLE_USER)
                .bind(username)
                .fetch_optional(&mut *transaction)
                .await?
        };
        let user_id = user_id.ok_or(AppError::NotFound)?;
        sqlx::query(queries::REVOKE_USER_SESSIONS)
            .bind(user_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn reserve_login(
        &self,
        username_hash: &str,
        now: i64,
        window: i64,
        maximum: i64,
    ) -> Result<(), AppError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query(queries::EXPIRE_LOGIN_WINDOWS)
            .bind(now - window)
            .execute(&mut *transaction)
            .await?;
        let attempts: i64 = sqlx::query_scalar(queries::RESERVE_LOGIN)
            .bind(username_hash)
            .bind(now)
            .fetch_one(&mut *transaction)
            .await?;
        transaction.commit().await?;
        if attempts > maximum {
            return Err(AppError::RateLimited);
        }
        Ok(())
    }

    pub async fn create_session(
        &self,
        token_hash: &str,
        expires_at: i64,
        credentials: &Credentials,
        now: i64,
    ) -> Result<(), AppError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query(queries::EXPIRE_SESSIONS)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        let created = sqlx::query(queries::CREATE_SESSION)
            .bind(token_hash)
            .bind(expires_at)
            .bind(&credentials.user_id)
            .bind(credentials.credential_version)
            .execute(&mut *transaction)
            .await?;
        if created.rows_affected() != 1 {
            return Err(AppError::Unauthorized);
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn session_user(&self, token_hash: &str, now: i64) -> Result<User, AppError> {
        sqlx::query_as(queries::SESSION_USER)
            .bind(token_hash)
            .bind(now)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::Unauthorized)
    }

    pub async fn revoke_session(&self, token_hash: &str) -> Result<(), AppError> {
        sqlx::query(queries::REVOKE_SESSION)
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn file_by_upload(
        &self,
        user_id: &str,
        upload_id: &str,
    ) -> Result<Option<ArchivedFile>, AppError> {
        Ok(sqlx::query_as(queries::FILE_BY_UPLOAD)
            .bind(user_id)
            .bind(upload_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn file(&self, user_id: &str, file_id: &str) -> Result<ArchivedFile, AppError> {
        sqlx::query_as(queries::FILE_BY_ID)
            .bind(user_id)
            .bind(file_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)
    }

    pub async fn archive_file(&self, user_id: &str, file: &ArchivedFile) -> Result<Job, AppError> {
        let job_id = uuid::Uuid::new_v4().to_string();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let deleted: i64 = sqlx::query_scalar(queries::UPLOAD_DELETED)
            .bind(user_id)
            .bind(&file.upload_id)
            .fetch_one(&mut *transaction)
            .await?;
        if deleted != 0 {
            return Err(AppError::Conflict(
                "This upload was deleted; import explicitly as a new document",
            ));
        }
        sqlx::query(queries::INSERT_FILE)
            .bind(&file.file_id)
            .bind(user_id)
            .bind(&file.upload_id)
            .bind(&file.relative_path)
            .bind(&file.processing_path)
            .bind(&file.processing_sha256)
            .bind(&file.original_name)
            .bind(&file.content_type)
            .bind(&file.sha256)
            .bind(file.byte_count)
            .bind(file.page_count)
            .bind(file.created_at)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(queries::INSERT_JOB)
            .bind(&job_id)
            .bind(user_id)
            .bind(&file.file_id)
            .bind(file.created_at)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(queries::INSERT_REPORT)
            .bind(&file.file_id)
            .bind(user_id)
            .bind(&file.file_id)
            .bind(file.created_at)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(queries::TOUCH_USER_DATA)
            .bind(user_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(queries::INVALIDATE_EXPORTS)
            .bind(user_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(Job {
            job_id,
            file_id: file.file_id.clone(),
            kind: "document_extract".into(),
            status: "blocked".into(),
            attempt_count: 0,
            error_code: Some("provider_disabled".into()),
            created_at: file.created_at,
        })
    }

    pub async fn files(
        &self,
        user_id: &str,
        request: &ListRequest,
    ) -> Result<Vec<ArchivedFile>, AppError> {
        request.validate()?;
        Ok(sqlx::query_as(queries::LIST_FILES)
            .bind(user_id)
            .bind(&request.after_id)
            .bind(&request.after_id)
            .bind(request.limit)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn job(&self, user_id: &str, job_id: &str) -> Result<Job, AppError> {
        sqlx::query_as(queries::JOB_BY_ID)
            .bind(user_id)
            .bind(job_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)
    }

    pub async fn job_by_file(&self, user_id: &str, file_id: &str) -> Result<Job, AppError> {
        sqlx::query_as(queries::JOB_BY_FILE)
            .bind(user_id)
            .bind(file_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)
    }

    pub async fn jobs(&self, user_id: &str, request: &ListRequest) -> Result<Vec<Job>, AppError> {
        request.validate()?;
        Ok(sqlx::query_as(queries::LIST_JOBS)
            .bind(user_id)
            .bind(&request.after_id)
            .bind(&request.after_id)
            .bind(request.limit)
            .fetch_all(&self.pool)
            .await?)
    }
}
