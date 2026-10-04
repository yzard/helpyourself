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
    pub(crate) health_slots: Arc<Semaphore>,
    _service_lock: Arc<File>,
}

impl AppState {
    pub async fn open(config: Config) -> Result<Self, AppError> {
        config.validate()?;
        tokio::fs::create_dir_all(&config.data_dir).await?;
        let lock_path = config.data_dir.join("server.lock");
        let service_lock = tokio::task::spawn_blocking(move || {
            let lock = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(lock_path)?;
            lock.try_lock_exclusive()
                .map_err(|_| AppError::Conflict("Another server is using this data directory"))?;
            Ok::<_, AppError>(lock)
        })
        .await
        .map_err(|_| AppError::Internal)??;
        let database = Database::open(&config.data_dir).await?;
        sqlx::query(crate::database::queries::RECOVER_ANALYSIS)
            .execute(&database.pool)
            .await?;
        sqlx::query(crate::database::queries::RECOVER_EXPORTS)
            .execute(&database.pool)
            .await?;
        for folder in ["raw", "derived", "tmp"] {
            tokio::fs::create_dir_all(config.data_dir.join(folder)).await?;
        }
        database.temporary_files.recover().await?;
        crate::raw::remove_orphans(&database, &config.data_dir).await?;
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
            health_slots: Arc::new(Semaphore::new(2)),
            _service_lock: Arc::new(service_lock),
        })
    }
    pub async fn shutdown(&self) -> Result<(), AppError> {
        self.database.cpu.shutdown().await;
        let cleanup = crate::maintenance::cleanup(self).await;
        self.database.close().await;
        cleanup
    }
}

pub fn create_application(state: AppState) -> Router {
    let upload_limit = state.config.storage.maximum_upload_bytes * 2 + 65_536;
    routes::router(upload_limit)
        .merge(crate::webgui::router())
        .layer(DefaultBodyLimit::max(16_384))
        .layer(middleware::from_fn(crate::middleware::request_logging))
        .with_state(state)
}
