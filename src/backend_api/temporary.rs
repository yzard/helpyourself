//! Temporary leases survive cancellation while a blocking operation still uses the path.
use crate::error::AppError;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, Weak},
};

#[derive(Clone)]
pub struct TemporaryFiles {
    root: PathBuf,
    leases: Arc<Mutex<HashMap<PathBuf, Weak<()>>>>,
}
#[derive(Clone)]
pub struct TemporaryFile {
    path: PathBuf,
    _lease: Arc<()>,
}
impl TemporaryFile {
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl TemporaryFiles {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            leases: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub fn reserve(&self, path: PathBuf) -> Result<TemporaryFile, AppError> {
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|_| AppError::Internal)?;
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err(AppError::Internal);
        }
        let mut leases = self.leases.lock().map_err(|_| AppError::Internal)?;
        if leases.contains_key(&path) {
            return Err(AppError::Conflict("Temporary path is already reserved"));
        }
        let lease = Arc::new(());
        leases.insert(path.clone(), Arc::downgrade(&lease));
        Ok(TemporaryFile {
            path,
            _lease: lease,
        })
    }
    pub async fn cleanup(&self) -> Result<(), AppError> {
        let inactive = self
            .leases
            .lock()
            .map_err(|_| AppError::Internal)?
            .iter()
            .filter(|(_, lease)| lease.strong_count() == 0)
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>();
        for path in inactive {
            remove(&path).await?;
            let mut parent = path.parent();
            while let Some(directory) = parent.filter(|directory| *directory != self.root) {
                match tokio::fs::remove_dir(directory).await {
                    Ok(()) => parent = directory.parent(),
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                        ) =>
                    {
                        break;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            self.leases
                .lock()
                .map_err(|_| AppError::Internal)?
                .remove(&path);
        }
        Ok(())
    }
    /// Only called under the exclusive data-directory lock, before any work can reserve paths.
    pub async fn recover(&self) -> Result<(), AppError> {
        tokio::fs::create_dir_all(&self.root).await?;
        let mut entries = tokio::fs::read_dir(&self.root).await?;
        while let Some(entry) = entries.next_entry().await? {
            remove(&entry.path()).await?;
        }
        Ok(())
    }
}
async fn remove(path: &Path) -> Result<(), AppError> {
    let result = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) if metadata.is_dir() => tokio::fs::remove_dir_all(path).await,
        Ok(_) => tokio::fs::remove_file(path).await,
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
