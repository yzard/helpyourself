use crate::{authentication, config::Config, database::Database, error::AppError, routes};
use axum::{Router, extract::DefaultBodyLimit, middleware};
use fs2::FileExt;
use std::{fs::File, sync::Arc};
use tokio::sync::{Mutex, Semaphore};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub database: Database,
    pub password_checks: Arc<Semaphore>,
    pub dummy_password_hash: String,
    pub(crate) file_mutation: Arc<Mutex<()>>,
    pub(crate) upload_slots: Arc<Semaphore>,
    _service_lock: Arc<File>,
}

impl AppState {
    pub async fn open(config: Config) -> Result<Self, AppError> {
        config.validate()?;
        tokio::fs::create_dir_all(&config.server.data_dir).await?;
        let service_lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(config.server.data_dir.join("server.lock"))?;
        service_lock
            .try_lock_exclusive()
            .map_err(|_| AppError::Conflict("Another server is using this data directory"))?;
        let database = Database::open(&config.server.data_dir).await?;
        sqlx::query(crate::database::queries::RECOVER_ANALYSIS)
            .execute(&database.pool)
            .await?;
        sqlx::query(crate::database::queries::RECOVER_EXPORTS)
            .execute(&database.pool)
            .await?;
        for folder in ["raw", "derived", "tmp"] {
            tokio::fs::create_dir_all(config.server.data_dir.join(folder)).await?;
        }
        // Only the lock owner can remove interrupted uploads on startup.
        let mut temporary_files = tokio::fs::read_dir(config.server.data_dir.join("tmp")).await?;
        while let Some(entry) = temporary_files.next_entry().await? {
            if entry.file_type().await?.is_file() {
                tokio::fs::remove_file(entry.path()).await?;
            } else if entry.file_type().await?.is_dir() {
                tokio::fs::remove_dir_all(entry.path()).await?;
            }
        }
        crate::raw::remove_orphans(&database, &config.server.data_dir).await?;
        let dummy_password_hash =
            authentication::hash_password(uuid::Uuid::new_v4().to_string()).await?;
        Ok(Self {
            password_checks: authentication::password_semaphore(
                config.security.maximum_password_checks,
            ),
            config: Arc::new(config),
            database,
            dummy_password_hash,
            file_mutation: Arc::new(Mutex::new(())),
            upload_slots: Arc::new(Semaphore::new(2)),
            _service_lock: Arc::new(service_lock),
        })
    }
}

pub fn create_application(state: AppState) -> Router {
    let upload_limit = state.config.storage.maximum_upload_bytes * 2 + 65_536;
    routes::router(upload_limit)
        .layer(DefaultBodyLimit::max(16_384))
        .layer(middleware::from_fn(crate::middleware::request_logging))
        .with_state(state)
}
