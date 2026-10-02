use crate::{
    app::AppState,
    authentication::now,
    database::{Database, queries},
    error::AppError,
};
use futures_util::TryStreamExt;
use serde_json::{Value, json};
use sqlx::{Column, Row, TypeInfo, ValueRef};
use std::{
    io::{BufRead, Write},
    path::Path,
};
use tokio::io::AsyncWriteExt;

impl Database {
    pub async fn data_revision(&self, user_id: &str) -> Result<i64, AppError> {
        sqlx::query_scalar(queries::USER_DATA_REVISION)
            .bind(user_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)
    }
    pub async fn delete_report(
        &self,
        user_id: &str,
        report_id: &str,
        revision: i64,
    ) -> Result<(), AppError> {
        let report = self.report_summary(user_id, report_id).await?;
        if report.revision != revision {
            return Err(AppError::Conflict("Report changed; reload before deleting"));
        }
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let updated = sqlx::query(queries::UPDATE_REPORT)
            .bind(Option::<String>::None)
            .bind(user_id)
            .bind(report_id)
            .bind(revision)
            .execute(&mut *transaction)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::Conflict("Report changed"));
        }
        let original = self.file(user_id, report_id).await?;
        for path in [
            original.relative_path,
            format!("derived/{user_id}/{report_id}"),
        ] {
            sqlx::query(queries::INSERT_CLEANUP)
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(path)
                .bind(now()?)
                .execute(&mut *transaction)
                .await?;
        }
        // Queue only exports that existed at deletion time. A later export must survive this cleanup.
        for row in sqlx::query(queries::ALL_EXPORT_IDS)
            .bind(user_id)
            .fetch_all(&mut *transaction)
            .await?
        {
            let export_id: String = row.get("export_id");
            sqlx::query(queries::INSERT_CLEANUP)
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(format!("exports/{user_id}/{export_id}.zip"))
                .bind(now()?)
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query(queries::TOMBSTONE_UPLOAD)
            .bind(user_id)
            .bind(report_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(queries::DELETE_FILE)
            .bind(user_id)
            .bind(report_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(queries::TOUCH_USER_DATA)
            .bind(user_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(queries::INVALIDATE_ANALYSES)
            .bind(user_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(queries::PURGE_FEEDBACK)
            .bind(user_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(queries::INVALIDATE_EXPORTS)
            .bind(user_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }
    pub async fn delete_account(&self, user_id: &str) -> Result<(), AppError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        for folder in [
            "raw/apple_health",
            "raw/google_health",
            "raw/photos",
            "raw/documents",
            "derived",
            "tmp",
            "exports",
        ] {
            sqlx::query(queries::INSERT_CLEANUP)
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(format!("{folder}/{user_id}"))
                .bind(now()?)
                .execute(&mut *transaction)
                .await?;
        }
        sqlx::query(queries::DELETE_USER)
            .bind(user_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }
    pub async fn request_export(&self, user_id: &str) -> Result<String, AppError> {
        let export_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(queries::INSERT_EXPORT)
            .bind(&export_id)
            .bind(user_id)
            .bind(now()?)
            .execute(&self.pool)
            .await?;
        Ok(export_id)
    }
    pub async fn exports(&self, user_id: &str) -> Result<Value, AppError> {
        let exports=sqlx::query(queries::EXPORT_LIST).bind(user_id).fetch_all(&self.pool).await?.into_iter().map(|row|json!({"export_id":row.get::<String,_>("export_id"),"status":row.get::<String,_>("status"),"error_code":row.get::<Option<String>,_>("error_code"),"created_at":row.get::<i64,_>("created_at")})).collect::<Vec<_>>();
        Ok(json!({"exports":exports}))
    }
    pub async fn export_ready(&self, user_id: &str, export_id: &str) -> Result<(), AppError> {
        let status: String = sqlx::query_scalar(queries::EXPORT_GET)
            .bind(user_id)
            .bind(export_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound)?;
        if status != "ready" {
            return Err(AppError::Conflict("Export is not current and ready"));
        }
        Ok(())
    }
    pub async fn delete_export(&self, user_id: &str, export_id: &str) -> Result<(), AppError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let deleted = sqlx::query(queries::DELETE_EXPORT)
            .bind(user_id)
            .bind(export_id)
            .execute(&mut *transaction)
            .await?;
        if deleted.rows_affected() != 1 {
            return Err(AppError::NotFound);
        }
        sqlx::query(queries::INSERT_CLEANUP)
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(format!("exports/{user_id}/{export_id}.zip"))
            .bind(now()?)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(())
    }
}

pub async fn cleanup(state: &AppState) -> Result<(), AppError> {
    state.database.temporary_files.cleanup().await?;
    let _guard = state.file_mutation.lock().await;
    let rows = sqlx::query(queries::CLEANUP_LIST)
        .fetch_all(&state.database.pool)
        .await?;
    for row in rows {
        let path: String = row.get("relative_path");
        let components = path.split('/').collect::<Vec<_>>();
        if components.len() < 2
            || !["raw", "derived", "tmp", "exports"].contains(&components[0])
            || if components[0] == "raw" {
                !crate::raw::valid_relative_path(&path)
            } else {
                uuid::Uuid::parse_str(components[1]).is_err()
            }
            || components
                .iter()
                .any(|part| part.is_empty() || *part == ".." || *part == "." || part.contains('\\'))
        {
            return Err(AppError::Internal);
        }
        let absolute = state.config.server.data_dir.join(path);
        match tokio::fs::symlink_metadata(&absolute).await {
            Ok(metadata) if metadata.is_dir() => tokio::fs::remove_dir_all(&absolute).await?,
            Ok(_) => tokio::fs::remove_file(&absolute).await?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        sqlx::query(queries::CLEANUP_DONE)
            .bind(row.get::<String, _>("cleanup_id"))
            .execute(&state.database.pool)
            .await?;
    }
    Ok(())
}

pub async fn export_next(state: &AppState) -> Result<bool, AppError> {
    let Some(row) = sqlx::query(queries::CLAIM_EXPORT)
        .fetch_optional(&state.database.pool)
        .await?
    else {
        return Ok(false);
    };
    let user_id: String = row.get("user_id");
    let export_id: String = row.get("export_id");
    let result = build_export(state, &user_id, &export_id).await;
    let (status, error) = if result.is_ok() {
        ("ready", None)
    } else {
        ("failed", Some("export_failed"))
    };
    sqlx::query(queries::FINISH_EXPORT)
        .bind(status)
        .bind(error)
        .bind(&user_id)
        .bind(&export_id)
        .execute(&state.database.pool)
        .await?;
    Ok(true)
}

async fn build_export(state: &AppState, user_id: &str, export_id: &str) -> Result<(), AppError> {
    let guard = std::sync::Arc::new(state.file_mutation.clone().lock_owned().await);
    let scratch = state.database.temporary_files.reserve(
        state
            .config
            .server
            .data_dir
            .join("tmp")
            .join(user_id)
            .join(export_id),
    )?;
    tokio::fs::create_dir_all(scratch.path()).await?;
    let mut transaction = state.database.pool.begin().await?;
    let revision: i64 = sqlx::query_scalar(queries::USER_DATA_REVISION)
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or(AppError::NotFound)?;
    for (table, query) in queries::EXPORT_TABLES {
        let mut output =
            tokio::fs::File::create(scratch.path().join(format!("{table}.jsonl"))).await?;
        let mut rows = sqlx::query(query).bind(user_id).fetch(&mut *transaction);
        while let Some(row) = rows.try_next().await? {
            let bytes = state
                .database
                .cpu
                .run_background(move || {
                    let mut object = serde_json::Map::new();
                    for column in row.columns() {
                        let name = column.name();
                        let raw = row.try_get_raw(name)?;
                        let value = if raw.is_null() {
                            Value::Null
                        } else {
                            match raw.type_info().name() {
                                "INTEGER" => json!(row.try_get::<i64, _>(name)?),
                                "REAL" => json!(row.try_get::<f64, _>(name)?),
                                _ => json!(row.try_get::<String, _>(name)?),
                            }
                        };
                        object.insert(name.into(), value);
                    }
                    Ok(serde_json::to_vec(&object)?)
                })
                .await?;
            output.write_all(&bytes).await?;
            output.write_all(b"\n").await?;
        }
        output.sync_all().await?;
    }
    transaction.commit().await?;
    let manifest = json!({"format":"helpyourself-export","version":3,"data_revision":revision,"user_id":user_id,"created_at":now()?,
        "conversion_version":crate::laboratory::CONVERSION_VERSION,"metrics":crate::laboratory::metrics(),"tables":queries::EXPORT_TABLES.iter().map(|(name,_)|format!("{name}.jsonl")).collect::<Vec<_>>(),"raw_directory":"raw/","csv":"observations.csv",
        "notes":"JSONL is lossless. CSV formula-leading text is prefixed with an apostrophe. No passwords, sessions or provider secrets are exported."});
    tokio::fs::write(
        scratch.path().join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )
    .await?;
    let archive_lease = scratch.clone();
    let archive_guard = guard.clone();
    let raw = state.config.server.data_dir.clone();
    state
        .database
        .cpu
        .run_background(move || {
            let _guard = archive_guard;
            write_zip(archive_lease.path(), &raw)
        })
        .await?;
    let mut transaction = state.database.pool.begin_with("BEGIN IMMEDIATE").await?;
    let current: Option<i64> = sqlx::query_scalar(queries::USER_DATA_REVISION)
        .bind(user_id)
        .fetch_optional(&mut *transaction)
        .await?;
    let status: Option<String> = sqlx::query_scalar(queries::EXPORT_GET)
        .bind(user_id)
        .bind(export_id)
        .fetch_optional(&mut *transaction)
        .await?;
    if current != Some(revision) || status.as_deref() != Some("running") {
        return Err(AppError::Conflict("Archive changed while exporting"));
    }
    let target = state.config.server.data_dir.join("exports").join(user_id);
    tokio::fs::create_dir_all(&target).await?;
    tokio::fs::rename(
        scratch.path().join("archive.zip"),
        target.join(format!("{export_id}.zip")),
    )
    .await?;
    sqlx::query(queries::FINISH_EXPORT)
        .bind("ready")
        .bind(Option::<String>::None)
        .bind(user_id)
        .bind(export_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(())
}

fn write_zip(directory: &Path, raw: &Path) -> Result<(), AppError> {
    let file = std::fs::File::create(directory.join("archive.zip"))?;
    let mut archive = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for name in std::iter::once("manifest.json".to_string()).chain(
        queries::EXPORT_TABLES
            .iter()
            .map(|(table, _)| format!("{table}.jsonl")),
    ) {
        archive
            .start_file(&name, options)
            .map_err(|_| AppError::Internal)?;
        std::io::copy(
            &mut std::fs::File::open(directory.join(&name))?,
            &mut archive,
        )?;
    }
    archive
        .start_file("observations.csv", options)
        .map_err(|_| AppError::Internal)?;
    archive.write_all(b"observation_id,revision,status,name,result,unit,sampled_at,standardized_result,standard_unit,result_status,original_reference,standardized_reference,reference_status,conversion_version\n")?;
    for line in std::io::BufReader::new(std::fs::File::open(
        directory.join("observation_revisions.jsonl"),
    )?)
    .lines()
    {
        let row: Value = serde_json::from_str(&line?)?;
        let payload: Value =
            serde_json::from_str(row["payload_json"].as_str().ok_or(AppError::Internal)?)?;
        let typed: crate::reports::ObservationPayload = serde_json::from_value(payload.clone())?;
        let interpreted = crate::laboratory::interpret(&typed);
        let fields = [
            row["observation_id"].as_str().unwrap_or("").to_string(),
            row["revision"].to_string(),
            row["status"].as_str().unwrap_or("").to_string(),
            payload["raw_name"].as_str().unwrap_or("").to_string(),
            payload["raw_result"].as_str().unwrap_or("").to_string(),
            payload["raw_unit"].as_str().unwrap_or("").to_string(),
            payload["sampled_at"].as_str().unwrap_or("").to_string(),
            interpreted.result.display.unwrap_or_default(),
            interpreted.result.unit.unwrap_or("").into(),
            interpreted.result.status.into(),
            payload["reference_range"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            interpreted.reference.display.unwrap_or_default(),
            interpreted.reference.status.into(),
            crate::laboratory::CONVERSION_VERSION.into(),
        ];
        archive.write_all(
            format!(
                "{}\n",
                fields
                    .iter()
                    .map(|field| csv_cell(field))
                    .collect::<Vec<_>>()
                    .join(",")
            )
            .as_bytes(),
        )?;
    }
    for (table, path_key) in [
        ("raw_files", "relative_path"),
        ("health_revisions", "raw_path"),
    ] {
        for line in std::io::BufReader::new(std::fs::File::open(
            directory.join(format!("{table}.jsonl")),
        )?)
        .lines()
        {
            let row: Value = serde_json::from_str(&line?)?;
            let path = row[path_key].as_str().ok_or(AppError::Internal)?;
            if !crate::raw::valid_relative_path(path) {
                return Err(AppError::Internal);
            }
            archive
                .start_file(path, options)
                .map_err(|_| AppError::Internal)?;
            std::io::copy(&mut std::fs::File::open(raw.join(path))?, &mut archive)?;
        }
    }
    archive
        .finish()
        .map_err(|_| AppError::Internal)?
        .sync_all()?;
    Ok(())
}

pub fn csv_cell(text: &str) -> String {
    let protected = if text.trim_start().starts_with(['=', '+', '-', '@']) {
        format!("'{text}")
    } else {
        text.to_string()
    };
    format!("\"{}\"", protected.replace('"', "\"\""))
}
