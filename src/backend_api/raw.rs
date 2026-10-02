use crate::{
    database::{Database, queries},
    error::AppError,
};
use sqlx::Row;
use std::{collections::HashSet, path::Path};
use tokio::io::AsyncWriteExt;

pub const SOURCES: &[&str] = &["apple_health", "google_health", "photos", "documents"];

pub fn document_path(user_id: &str, file_id: &str, content_type: &str) -> String {
    let source = if content_type.starts_with("image/") {
        "photos"
    } else {
        "documents"
    };
    format!("raw/{source}/{user_id}/{file_id}")
}

pub fn health_path(user_id: &str, platform: &str, revision_id: &str) -> Result<String, AppError> {
    let source = match platform {
        "apple_health" => "apple_health",
        "health_connect" => "google_health",
        _ => return Err(AppError::Invalid("Unknown health platform")),
    };
    Ok(format!("raw/{source}/{user_id}/{revision_id}.json"))
}

pub fn valid_relative_path(path: &str) -> bool {
    let parts = path.split('/').collect::<Vec<_>>();
    parts.len() >= 3
        && parts[0] == "raw"
        && SOURCES.contains(&parts[1])
        && uuid::Uuid::parse_str(parts[2]).is_ok()
        && parts
            .iter()
            .all(|part| !part.is_empty() && *part != "." && *part != ".." && !part.contains('\\'))
}

pub async fn sync_directory(path: &Path) -> Result<(), AppError> {
    #[cfg(unix)]
    tokio::fs::File::open(path).await?.sync_all().await?;
    Ok(())
}

/// Write and flush a new original before the transaction publishes its index.
pub async fn archive(
    data_dir: &Path,
    temporary_files: &crate::temporary::TemporaryFiles,
    relative_path: &str,
    bytes: &[u8],
) -> Result<(), AppError> {
    if !valid_relative_path(relative_path) {
        return Err(AppError::Internal);
    }
    let target = data_dir.join(relative_path);
    let directory = target.parent().ok_or(AppError::Internal)?;
    tokio::fs::create_dir_all(directory).await?;
    let temporary =
        temporary_files.reserve(data_dir.join("tmp").join(uuid::Uuid::new_v4().to_string()))?;
    let mut output = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(temporary.path())
        .await?;
    output.write_all(bytes).await?;
    output.sync_all().await?;
    drop(output);
    tokio::fs::rename(temporary.path(), &target).await?;
    sync_directory(directory).await?;
    sync_directory(directory.parent().ok_or(AppError::Internal)?).await?;
    Ok(())
}

pub async fn remove_orphans(database: &Database, data_dir: &Path) -> Result<(), AppError> {
    let paths: HashSet<String> = sqlx::query(queries::RAW_PATHS)
        .fetch_all(&database.pool)
        .await?
        .into_iter()
        .map(|row| row.get("relative_path"))
        .collect();
    for source in SOURCES {
        let mut users = tokio::fs::read_dir(data_dir.join("raw").join(source)).await?;
        while let Some(user) = users.next_entry().await? {
            if !user.file_type().await?.is_dir() {
                continue;
            }
            let user_id = user.file_name().to_string_lossy().into_owned();
            if uuid::Uuid::parse_str(&user_id).is_err() {
                continue;
            }
            let mut files = tokio::fs::read_dir(user.path()).await?;
            while let Some(file) = files.next_entry().await? {
                if !file.file_type().await?.is_file() {
                    continue;
                }
                let relative = format!(
                    "raw/{source}/{user_id}/{}",
                    file.file_name().to_string_lossy()
                );
                if !paths.contains(&relative) {
                    tokio::fs::remove_file(file.path()).await?;
                }
            }
        }
    }
    Ok(())
}
