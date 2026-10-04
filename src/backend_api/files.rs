use crate::{
    app::AppState,
    authentication::now,
    config::StorageConfig,
    error::AppError,
    models::{ArchivedFile, Job},
};
use axum::extract::Multipart;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};
use tokio::io::AsyncWriteExt;

#[derive(Serialize)]
pub struct UploadResponse {
    pub file: ArchivedFile,
    pub job: Job,
    pub replayed: bool,
}

pub async fn receive_upload(
    state: &AppState,
    user_id: &str,
    upload_id: &str,
    mut multipart: Multipart,
) -> Result<UploadResponse, AppError> {
    let upload_id = uuid::Uuid::parse_str(upload_id)
        .map_err(|_| AppError::Invalid("X-Upload-Id must be a UUID"))?
        .to_string();
    let _slot = Arc::clone(&state.upload_slots)
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    let pending = state.database.temporary_files.reserve(
        state
            .config
            .data_dir
            .join("tmp")
            .join(uuid::Uuid::new_v4().to_string()),
    )?;
    let mut output = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(pending.path())
        .await?;
    let field = multipart
        .next_field()
        .await
        .map_err(|error| multipart_error(error.status().as_u16()))?
        .ok_or(AppError::Invalid("A file field is required"))?;
    if field.name() != Some("file") {
        return Err(AppError::Invalid("Expected a field named file"));
    }
    let first = receive_part(
        field,
        &mut output,
        state.config.storage.maximum_upload_bytes,
        &state.database.cpu,
        &pending,
    )
    .await?;
    output.sync_all().await?;
    drop(output);
    let inspected = pending.clone();
    let limits = state.config.storage.clone();
    let (processing_type, page_count) = state
        .database
        .cpu
        .run(move || inspect_document(inspected.path(), &limits))
        .await?;
    let original_pending = state.database.temporary_files.reserve(
        state
            .config
            .data_dir
            .join("tmp")
            .join(uuid::Uuid::new_v4().to_string()),
    )?;
    let original = if let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| multipart_error(error.status().as_u16()))?
    {
        if field.name() != Some("original") || processing_type != "image/jpeg" {
            return Err(AppError::Invalid(
                "An original is allowed only with a JPEG processing image",
            ));
        }
        let mut output = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(original_pending.path())
            .await?;
        let original = receive_part(
            field,
            &mut output,
            state.config.storage.maximum_upload_bytes,
            &state.database.cpu,
            &original_pending,
        )
        .await?;
        output.sync_all().await?;
        drop(output);
        let mut input = tokio::fs::File::open(original_pending.path()).await?;
        let mut header = [0u8; 64];
        let size = tokio::io::AsyncReadExt::read(&mut input, &mut header).await?;
        if !matches!(original.content_type.as_str(), "image/heic" | "image/heif")
            || size < 16
            || &header[4..8] != b"ftyp"
            || !header[8..size].windows(4).any(|brand| {
                [
                    b"heic".as_slice(),
                    b"heix".as_slice(),
                    b"hevc".as_slice(),
                    b"hevx".as_slice(),
                    b"mif1".as_slice(),
                ]
                .contains(&brand)
            })
        {
            return Err(AppError::Invalid("Expected a HEIC or HEIF original"));
        }
        Some(original)
    } else {
        None
    };
    if multipart
        .next_field()
        .await
        .map_err(|error| multipart_error(error.status().as_u16()))?
        .is_some()
    {
        return Err(AppError::Invalid("Unexpected upload field"));
    }
    let processing_sha256 = original.as_ref().map(|_| first.sha256.clone());
    let original_name = original
        .as_ref()
        .map(|value| value.name.clone())
        .unwrap_or(first.name);
    let content_type = original
        .as_ref()
        .map(|value| value.content_type.clone())
        .unwrap_or(processing_type);
    let byte_count = original
        .as_ref()
        .map(|value| value.byte_count)
        .unwrap_or(first.byte_count);
    let sha256 = original
        .as_ref()
        .map(|value| value.sha256.clone())
        .unwrap_or(first.sha256);
    let _mutation = state.file_mutation.lock().await;
    if let Some(file) = state.database.file_by_upload(user_id, &upload_id).await? {
        if file.sha256 != sha256
            || file.original_name != original_name
            || file.processing_sha256 != processing_sha256
        {
            return Err(AppError::Conflict(
                "Upload ID already belongs to different content or filename",
            ));
        }
        let job = state.database.job_by_file(user_id, &file.file_id).await?;
        return Ok(UploadResponse {
            file,
            job,
            replayed: true,
        });
    }
    let file_id = uuid::Uuid::new_v4().to_string();
    let relative_path = crate::raw::document_path(user_id, &file_id, &content_type);
    let processing_path = original
        .as_ref()
        .map(|_| format!("derived/{user_id}/{file_id}/input.jpg"));
    let file = ArchivedFile {
        file_id,
        relative_path,
        processing_path,
        processing_sha256,
        upload_id,
        original_name,
        content_type,
        sha256,
        byte_count: byte_count as i64,
        page_count,
        created_at: now()?,
    };
    let final_path = state.config.data_dir.join(&file.relative_path);
    let directory = final_path.parent().ok_or(AppError::Internal)?;
    tokio::fs::create_dir_all(directory).await?;
    if let Some(processing_path) = &file.processing_path {
        let processing = state.config.data_dir.join(processing_path);
        let directory = processing.parent().ok_or(AppError::Internal)?;
        tokio::fs::create_dir_all(directory).await?;
        tokio::fs::rename(pending.path(), &processing).await?;
        crate::raw::sync_directory(directory).await?;
        tokio::fs::rename(original_pending.path(), &final_path).await?;
    } else {
        tokio::fs::rename(pending.path(), &final_path).await?;
    }
    crate::raw::sync_directory(directory).await?;
    crate::raw::sync_directory(directory.parent().ok_or(AppError::Internal)?).await?;
    // A crash before the database commit can leave an orphan; never leave a row pointing at an unwritten file.
    let saved = state.database.archive_file(user_id, &file).await;
    let job = match saved {
        Ok(job) => job,
        Err(error) => {
            tokio::fs::remove_file(&final_path).await?;
            if let Some(path) = &file.processing_path {
                tokio::fs::remove_file(state.config.data_dir.join(path)).await?;
            }
            return Err(error);
        }
    };
    Ok(UploadResponse {
        file,
        job,
        replayed: false,
    })
}

struct UploadPart {
    name: String,
    content_type: String,
    sha256: String,
    byte_count: usize,
}
async fn receive_part(
    mut field: axum::extract::multipart::Field<'_>,
    output: &mut tokio::fs::File,
    maximum_bytes: usize,
    cpu: &crate::execution::CpuExecutor,
    temporary: &crate::temporary::TemporaryFile,
) -> Result<UploadPart, AppError> {
    let name = field
        .file_name()
        .ok_or(AppError::Invalid("Filename is required"))?
        .to_owned();
    if name.is_empty() || name.len() > 255 || name.chars().any(char::is_control) {
        return Err(AppError::Invalid("Invalid filename"));
    }
    let content_type = field
        .content_type()
        .unwrap_or("application/octet-stream")
        .to_owned();
    let mut byte_count = 0usize;
    while let Some(chunk) = field
        .chunk()
        .await
        .map_err(|error| multipart_error(error.status().as_u16()))?
    {
        byte_count = byte_count
            .checked_add(chunk.len())
            .ok_or(AppError::TooLarge)?;
        if byte_count > maximum_bytes {
            return Err(AppError::TooLarge);
        }
        output.write_all(&chunk).await?;
    }
    if byte_count == 0 {
        return Err(AppError::Invalid("Empty file"));
    }
    Ok(UploadPart {
        name,
        content_type,
        sha256: {
            output.flush().await?;
            let temporary = temporary.clone();
            cpu.run(move || {
                use std::io::Read;
                let mut input = std::fs::File::open(temporary.path())?;
                let mut hasher = Sha256::new();
                let mut buffer = [0u8; 65536];
                loop {
                    let count = input.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    hasher.update(&buffer[..count]);
                }
                Ok(hex::encode(hasher.finalize()))
            })
            .await?
        },
        byte_count,
    })
}

fn multipart_error(status: u16) -> AppError {
    if status == 413 {
        return AppError::TooLarge;
    }
    AppError::Invalid("Invalid multipart upload")
}

pub fn inspect_document(path: &Path, limits: &StorageConfig) -> Result<(String, i64), AppError> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > limits.maximum_upload_bytes {
        return Err(AppError::TooLarge);
    }
    if bytes.starts_with(b"%PDF-") {
        let document = lopdf::Document::load_mem(&bytes)
            .map_err(|_| AppError::Invalid("PDF could not be parsed"))?;
        if document.is_encrypted() {
            return Err(AppError::Invalid(
                "Encrypted PDFs are not supported; export an unlocked copy",
            ));
        }
        let pages = document.get_pages().len();
        if pages == 0 {
            return Err(AppError::Invalid("PDF has no pages"));
        }
        if pages > limits.maximum_pdf_pages {
            return Err(AppError::TooLarge);
        }
        return Ok(("application/pdf".into(), pages as i64));
    }
    let format = image::guess_format(&bytes)
        .map_err(|_| AppError::Invalid("Only PDF, JPEG and PNG are supported"))?;
    let content_type = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        _ => return Err(AppError::Invalid("Only PDF, JPEG and PNG are supported")),
    };
    let dimensions = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|_| AppError::Invalid("Image could not be parsed"))?;
    if u64::from(dimensions.0) * u64::from(dimensions.1) > limits.maximum_image_pixels {
        return Err(AppError::TooLarge);
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format);
    let mut decoder_limits = image::Limits::default();
    decoder_limits.max_alloc = Some(limits.maximum_image_pixels.saturating_mul(16));
    reader.limits(decoder_limits);
    reader
        .decode()
        .map_err(|_| AppError::Invalid("Image could not be decoded"))?;
    Ok((content_type.into(), 1))
}
