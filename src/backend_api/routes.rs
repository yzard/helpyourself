use crate::{
    app::AppState,
    authentication::{self, CurrentUser},
    error::AppError,
    files,
    models::{FileRequest, JobRequest, ListRequest, LoginRequest},
};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::json;

pub fn router(upload_limit: usize) -> Router<AppState> {
    Router::new()
        .route("/api/v1/reports/delete", post(delete_report))
        .route("/api/v1/user/delete", post(delete_account))
        .route("/api/v1/exports/create", post(create_export))
        .route("/api/v1/exports/list", post(list_exports))
        .route("/api/v1/exports/delete", post(delete_export))
        .route("/api/v1/exports/{export_id}/download", get(download_export))
        .route("/api/v1/analysis/create", post(create_analysis))
        .route("/api/v1/analysis/list", post(list_analyses))
        .route("/api/v1/analysis/get", post(get_analysis))
        .route("/api/v1/analysis/retry", post(retry_analysis))
        .route("/api/v1/analysis/feedback", post(analysis_feedback))
        .route("/api/v1/reports/list", post(list_reports))
        .route("/api/v1/reports/get", post(get_report))
        .route("/api/v1/reports/input/get", post(get_extraction_input))
        .route(
            "/api/v1/reports/extraction/get",
            post(get_extraction_output),
        )
        .route(
            "/api/v1/reports/review",
            post(review_report).layer(DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route("/api/v1/reports/relate", post(relate_reports))
        .route("/api/v1/observations/history", post(observation_history))
        .route("/api/v1/metrics/list", post(list_metrics))
        .route("/api/v1/trends/get", post(get_trends))
        .route("/api/v1/health/connect", post(connect_health))
        .route(
            "/api/v1/health/sync",
            post(sync_health).layer(DefaultBodyLimit::max(40 * 1024 * 1024)),
        )
        .route("/api/v1/health/coverage", post(health_coverage))
        .route("/api/v1/health/list", post(list_health))
        .route("/api/v1/health/raw/list", post(list_raw_health))
        .route(
            "/api/v1/health/raw/{raw_id}/download",
            get(download_raw_health),
        )
        .route("/api/v1/health/aggregate", post(aggregate_health))
        .route(
            "/api/v1/wellness/entries/save",
            post(wellness_entry_save).layer(DefaultBodyLimit::max(256 * 1024)),
        )
        .route("/api/v1/wellness/entries/list", post(wellness_entry_list))
        .route(
            "/api/v1/wellness/entries/delete",
            post(wellness_entry_delete),
        )
        .route(
            "/api/v1/wellness/import/gpx",
            post(wellness_import_gpx).layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
        )
        .route(
            "/api/v1/wellness/import/tcx",
            post(wellness_import_tcx).layer(DefaultBodyLimit::max(5 * 1024 * 1024)),
        )
        .route("/api/v1/wellness/import/list", post(wellness_import_list))
        .route(
            "/api/v1/wellness/import/delete",
            post(wellness_import_delete),
        )
        .route(
            "/api/v1/wellness/preferences/get",
            post(wellness_preferences),
        )
        .route(
            "/api/v1/wellness/preferences/save",
            post(wellness_preferences_save),
        )
        .route("/api/v1/wellness/sleep", post(wellness_sleep))
        .route(
            "/api/v1/wellness/sleep/regularity",
            post(wellness_regularity),
        )
        .route("/api/v1/wellness/clinical-age", post(wellness_clinical_age))
        .route("/api/v1/wellness/day", post(wellness_day))
        .route("/api/v1/wellness/series", post(wellness_series))
        .route("/api/v1/wellness/hrv", post(wellness_hrv))
        .route("/api/v1/wellness/library", post(wellness_library))
        .route("/api/v1/wellness/import/fit", post(import_fit))
        .route("/api/v1/wellness/coach", post(request_coach))
        .route("/api/v1/wellness/reminders", post(reminder_occurrences))
        .route("/api/v1/wellness/records/list", post(specialty_records))
        .route("/api/v1/wellness/records/get", post(specialty_record))
        .route("/api/v1/wellness/food/portion", post(food_portion))
        .route("/api/v1/wellness/food/lookup", post(food_lookup))
        .route("/api/v1/wellness/nutrition/day", post(nutrition_day))
        .route("/api/v1/wellness/timeline", post(wellness_timeline))
        .route("/api/v1/wellness/review", post(wellness_review))
        .route("/api/v1/wellness/report", post(wellness_report))
        .route("/api/v1/wellness/diet", post(wellness_diet))
        .route("/api/v1/wellness/meals", post(wellness_meals))
        .route("/api/v1/wellness/meal-glucose", post(wellness_meal_glucose))
        .route("/api/v1/wellness/associations/run", post(associations_run))
        .route(
            "/api/v1/wellness/associations/list",
            post(associations_list),
        )
        .route("/api/v1/wellness/associations/get", post(associations_get))
        .route("/api/v1/wellness/sources", post(wellness_sources))
        .route("/api/v1/server/status", post(status))
        .route("/api/v1/session/login", post(login))
        .route("/api/v1/session/logout", post(logout))
        .route("/api/v1/user/get", post(current_user))
        .route(
            "/api/v1/files/upload",
            post(upload).layer(DefaultBodyLimit::max(upload_limit)),
        )
        .route("/api/v1/files/list", post(list_files))
        .route("/api/v1/files/get", post(get_file))
        .route("/api/v1/files/{file_id}/download", get(download))
        .route("/api/v1/jobs/list", post(list_jobs))
        .route("/api/v1/jobs/get", post(get_job))
        .route("/api/v1/jobs/retry", post(retry_job))
        .fallback(|| async { AppError::NotFound })
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportRequest {
    report_id: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationRequest {
    observation_id: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RelationRequest {
    report_id: String,
    preferred_report_id: Option<String>,
    kind: String,
    expected_revision: i64,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HealthListRequest {
    limit: i64,
    offset: i64,
}
async fn list_reports(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<ListRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        json!({"reports":state.database.reports(&current.user.user_id,&request).await?}),
    ))
}
async fn get_report(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<ReportRequest>,
) -> Result<Response, AppError> {
    crate::transport::json_response(
        &state,
        state
            .database
            .report(&current.user.user_id, &request.report_id)
            .await?,
    )
    .await
}
async fn get_extraction_output(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::reports::ExtractionOutputRequest>,
) -> Result<Response, AppError> {
    crate::transport::json_response(
        &state,
        state
            .database
            .extraction_output(&current.user.user_id, request)
            .await?,
    )
    .await
}

async fn review_report(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::reports::ReviewRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let confirmed = request
        .observations
        .iter()
        .any(|item| item.status == "confirmed");
    let mut response = state
        .database
        .review(&current.user.user_id, request)
        .await?;
    if confirmed && state.config.providers.analysis.enabled {
        let end = chrono::Utc::now().date_naive();
        let scope = crate::analysis::AnalysisRequest {
            start_date: (end - chrono::Duration::days(90)).to_string(),
            end_date: end.to_string(),
            timezone: "UTC".into(),
        };
        response["analysis_trigger"] = match crate::analysis::request_analysis(
            &state,
            &current.user.user_id,
            scope,
        )
        .await
        {
            Ok(run_id) => json!({"run_id":run_id}),
            Err(_) => {
                json!({"status":"not_queued","reason":"Analysis requires mapped lipid data and a stable snapshot"})
            }
        };
    }
    Ok(Json(response))
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteReportRequest {
    report_id: String,
    expected_revision: i64,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteAccountRequest {
    confirmation: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportRequest {
    export_id: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AnalysisId {
    run_id: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FeedbackRequest {
    run_id: String,
    note: String,
}
async fn delete_report(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<DeleteReportRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _guard = state.file_mutation.lock().await;
    state
        .database
        .delete_report(
            &current.user.user_id,
            &request.report_id,
            request.expected_revision,
        )
        .await?;
    Ok(Json(json!({"status":"cleanup_queued"})))
}
async fn delete_account(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<DeleteAccountRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if request.confirmation != current.user.username {
        return Err(AppError::Invalid(
            "Confirm the account username before deleting",
        ));
    }
    let _guard = state.file_mutation.lock().await;
    state.database.delete_account(&current.user.user_id).await?;
    Ok(Json(
        json!({"status":"cleanup_queued","sessions_revoked":true}),
    ))
}
async fn create_export(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        json!({"export_id":state.database.request_export(&current.user.user_id).await?}),
    ))
}
async fn list_exports(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(state.database.exports(&current.user.user_id).await?))
}
async fn delete_export(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<ExportRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _guard = state.file_mutation.lock().await;
    state
        .database
        .delete_export(&current.user.user_id, &request.export_id)
        .await?;
    Ok(Json(json!({"status":"cleanup_queued"})))
}
async fn download_export(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(export_id): Path<String>,
) -> Result<Response, AppError> {
    let _guard = state.file_mutation.lock().await;
    state
        .database
        .export_ready(&current.user.user_id, &export_id)
        .await?;
    let file = tokio::fs::File::open(
        state
            .config
            .data_dir
            .join("exports")
            .join(&current.user.user_id)
            .join(format!("{export_id}.zip")),
    )
    .await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=helpyourself-export.zip",
            ),
        ],
        Body::from_stream(tokio_util::io::ReaderStream::new(file)),
    )
        .into_response())
}
async fn create_analysis(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::analysis::AnalysisRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        json!({"run_id":crate::analysis::request_analysis(&state,&current.user.user_id,request).await?}),
    ))
}
async fn list_analyses(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(state.database.analyses(&current.user.user_id).await?))
}
async fn get_analysis(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<AnalysisId>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .analysis(&current.user.user_id, &request.run_id)
            .await?,
    ))
}
async fn retry_analysis(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<AnalysisId>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .database
        .retry_analysis(&current.user.user_id, &request.run_id)
        .await?;
    Ok(Json(json!({"queued":true})))
}
async fn analysis_feedback(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<FeedbackRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .database
        .analysis_feedback(&current.user.user_id, &request.run_id, &request.note)
        .await?;
    Ok(Json(
        json!({"saved":true,"verification_status":"unverified"}),
    ))
}
async fn relate_reports(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<RelationRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    state
        .database
        .relate_reports(
            &current.user.user_id,
            &request.report_id,
            request.preferred_report_id.as_deref(),
            &request.kind,
            request.expected_revision,
        )
        .await?;
    Ok(Json(json!({"updated":true})))
}
async fn observation_history(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<ObservationRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .observation_history(&current.user.user_id, &request.observation_id)
            .await?,
    ))
}
async fn list_metrics(_current: CurrentUser) -> Json<serde_json::Value> {
    Json(json!({"metrics":crate::laboratory::metrics()}))
}
async fn get_trends(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::reports::TrendRequest>,
) -> Result<Response, AppError> {
    crate::transport::json_response(
        &state,
        crate::reports::trends(&state.database, &current.user.user_id, request).await?,
    )
    .await
}
async fn connect_health(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::health::ConnectionRequest>,
) -> Result<Json<crate::health::Connection>, AppError> {
    if !["apple_health", "health_connect"].contains(&request.platform.as_str()) {
        return Err(AppError::Invalid(
            "Use the domain import endpoint for this source",
        ));
    }
    Ok(Json(
        state
            .database
            .create_connection(&current.user.user_id, request)
            .await?,
    ))
}
async fn sync_health(
    State(state): State<AppState>,
    current: CurrentUser,
    body: crate::transport::HealthSyncBody,
) -> Result<Json<serde_json::Value>, AppError> {
    let _guard = state.file_mutation.lock().await;
    state
        .database
        .require_device_connection(&current.user.user_id, &body.batch.connection_id)
        .await?;
    Ok(Json(
        state
            .database
            .apply_health_batch(&current.user.user_id, body.batch)
            .await?,
    ))
}
async fn list_raw_health(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<ListRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .health_raw_files(&current.user.user_id, &request)
            .await?,
    ))
}
async fn download_raw_health(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(raw_id): Path<String>,
) -> Result<Response, AppError> {
    let relative = state
        .database
        .health_raw_path(&current.user.user_id, &raw_id)
        .await?;
    let file = tokio::fs::File::open(state.config.data_dir.join(relative)).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CONTENT_DISPOSITION, "attachment"),
        ],
        Body::from_stream(tokio_util::io::ReaderStream::new(file)),
    )
        .into_response())
}

async fn health_coverage(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .health_coverage(&current.user.user_id)
            .await?,
    ))
}
async fn list_health(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<HealthListRequest>,
) -> Result<Response, AppError> {
    let _admission = state
        .health_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    crate::transport::json_response(
        &state,
        state
            .database
            .health_records(&current.user.user_id, request.limit, request.offset)
            .await?,
    )
    .await
}
async fn aggregate_health(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::health::AggregateRequest>,
) -> Result<Response, AppError> {
    let _admission = state
        .health_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    crate::transport::json_response(
        &state,
        state
            .database
            .aggregate_health(&current.user.user_id, request)
            .await?,
    )
    .await
}

async fn status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(
        json!({"status":"ok", "api_version":1, "capabilities":{"file_archive":true,"document_extraction":true,
        "review":true,"trends":true,"health_sync":true,"wellness_day":true,"manual_entries":true,"export":true,"delete":true,"analysis":state.config.providers.analysis.enabled}}),
    )
}

async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<crate::models::LoginResponse>, AppError> {
    Ok(Json(authentication::login(&state, request).await?))
}
async fn logout(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<serde_json::Value>, AppError> {
    state.database.revoke_session(&current.token_hash).await?;
    Ok(Json(json!({"revoked":true})))
}
async fn current_user(current: CurrentUser) -> Json<crate::models::User> {
    Json(current.user)
}

async fn upload(
    State(state): State<AppState>,
    current: CurrentUser,
    headers: HeaderMap,
    multipart: Multipart,
) -> Result<Json<files::UploadResponse>, AppError> {
    let upload_id = headers
        .get("x-upload-id")
        .and_then(|header| header.to_str().ok())
        .ok_or(AppError::Invalid("X-Upload-Id is required"))?;
    Ok(Json(
        files::receive_upload(&state, &current.user.user_id, upload_id, multipart).await?,
    ))
}
async fn list_files(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<ListRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        json!({"files":state.database.files(&current.user.user_id, &request).await?}),
    ))
}
async fn get_file(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<FileRequest>,
) -> Result<Json<crate::models::ArchivedFile>, AppError> {
    Ok(Json(
        state
            .database
            .file(&current.user.user_id, &request.file_id)
            .await?,
    ))
}
async fn download(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(file_id): Path<String>,
) -> Result<Response, AppError> {
    let file = state.database.file(&current.user.user_id, &file_id).await?;
    let path = state.config.data_dir.join(&file.relative_path);
    let reader = tokio::fs::File::open(path).await?;
    let body = Body::from_stream(tokio_util::io::ReaderStream::new(reader));
    Ok((
        [
            (header::CONTENT_TYPE, file.content_type),
            (header::CONTENT_DISPOSITION, "attachment".into()),
        ],
        body,
    )
        .into_response())
}
async fn list_jobs(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<ListRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        json!({"jobs":state.database.jobs(&current.user.user_id, &request).await?}),
    ))
}
async fn get_job(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<JobRequest>,
) -> Result<Json<crate::models::Job>, AppError> {
    Ok(Json(
        state
            .database
            .job(&current.user.user_id, &request.job_id)
            .await?,
    ))
}
async fn retry_job(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<JobRequest>,
) -> Result<Json<crate::models::Job>, AppError> {
    Ok(Json(
        state
            .database
            .retry_job(&current.user.user_id, &request.job_id)
            .await?,
    ))
}

async fn get_extraction_input(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::reports::ExtractionInputRequest>,
) -> Result<Response, AppError> {
    crate::transport::json_response(
        &state,
        state
            .database
            .extraction_input(&current.user.user_id, request)
            .await?,
    )
    .await
}

async fn wellness_day(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::DayRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .wellness_day(&current.user.user_id, request)
            .await?,
    ))
}
async fn wellness_sources(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .wellness_sources(&current.user.user_id)
            .await?,
    ))
}

async fn wellness_entry_save(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::entries::SaveRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _guard = state.file_mutation.lock().await;
    Ok(Json(
        state
            .database
            .save_wellness_entry(&current.user.user_id, request)
            .await?,
    ))
}
async fn wellness_entry_list(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::entries::ListRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .list_wellness_entries(&current.user.user_id, request)
            .await?,
    ))
}
async fn wellness_entry_delete(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::entries::DeleteRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _guard = state.file_mutation.lock().await;
    Ok(Json(
        state
            .database
            .delete_wellness_entry(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_import_gpx(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::import::XmlImportRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _slot = state
        .health_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    let _guard = state.file_mutation.lock().await;
    Ok(Json(
        state
            .database
            .import_gpx(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_sleep(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::sleep::SleepRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .sleep_sessions(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_clinical_age(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::clinical::ClinicalRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .clinical_age(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_import_list(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::import::ImportListRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .list_training_imports(&current.user.user_id, request)
            .await?,
    ))
}
async fn wellness_import_delete(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::import::ImportDeleteRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _guard = state.file_mutation.lock().await;
    Ok(Json(
        state
            .database
            .delete_training_import(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_preferences(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .wellness_preferences(&current.user.user_id)
            .await?,
    ))
}
async fn wellness_preferences_save(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::preferences::SavePreferences>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _guard = state.file_mutation.lock().await;
    Ok(Json(
        state
            .database
            .save_wellness_preferences(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_series(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::series::SeriesRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _permit = state
        .health_slots
        .acquire()
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(
        state
            .database
            .wellness_series(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_timeline(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::timeline::TimelineRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _permit = state
        .health_slots
        .acquire()
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(
        state
            .database
            .wellness_timeline(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_review(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::review::ReviewRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _permit = state
        .health_slots
        .acquire()
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(
        state
            .database
            .wellness_review(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_import_tcx(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::import::XmlImportRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _slot = state
        .health_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    let _guard = state.file_mutation.lock().await;
    Ok(Json(
        state
            .database
            .import_tcx(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_regularity(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::sleep::SleepRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _slot = state
        .health_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    Ok(Json(
        state
            .database
            .sleep_regularity_view(&current.user.user_id, request)
            .await?,
    ))
}

async fn associations_run(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::associations::Request>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _slot = state
        .health_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| AppError::RateLimited)?;
    Ok(Json(
        state
            .database
            .behavior_associations(&current.user.user_id, request)
            .await?,
    ))
}
async fn associations_list(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .association_results(&current.user.user_id)
            .await?,
    ))
}
async fn associations_get(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::associations::GetRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .association_result(&current.user.user_id, &request.result_id)
            .await?,
    ))
}

async fn wellness_hrv(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::hrv::Request>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _permit = state
        .health_slots
        .acquire()
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(
        state
            .database
            .hrv_windows(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_library(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::planning::LibraryRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .wellness_library(&current.user.user_id, request)
            .await?,
    ))
}

async fn food_portion(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::food::PortionRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .food_portion(&current.user.user_id, request)
            .await?,
    ))
}

async fn nutrition_day(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::food::DayRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .nutrition_day(&current.user.user_id, request)
            .await?,
    ))
}

async fn specialty_records(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::records::ListRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .specialty_records(&current.user.user_id, request)
            .await?,
    ))
}
async fn specialty_record(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::records::GetRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _permit = state
        .health_slots
        .acquire()
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(
        state
            .database
            .specialty_record(&current.user.user_id, request)
            .await?,
    ))
}

async fn reminder_occurrences(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::reminders::Request>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(
        state
            .database
            .reminder_occurrences(&current.user.user_id, request)
            .await?,
    ))
}

async fn request_coach(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::coach::Request>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _permit = state
        .health_slots
        .acquire()
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(
        serde_json::json!({"run_id":crate::wellness::coach::request(&state,&current.user.user_id,request).await?}),
    ))
}

async fn import_fit(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::fit::Request>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _permit = state
        .health_slots
        .acquire()
        .await
        .map_err(|_| AppError::Internal)?;
    let _guard = state.file_mutation.lock().await;
    Ok(Json(
        state
            .database
            .import_fit(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_report(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::report::Request>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _permit = state
        .health_slots
        .acquire()
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(
        state
            .database
            .wellness_report(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_diet(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::diet::Request>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _permit = state
        .health_slots
        .acquire()
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(
        state
            .database
            .diet_quality(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_meal_glucose(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::meal_glucose::Request>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _permit = state
        .health_slots
        .acquire()
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(
        state
            .database
            .meal_glucose(&current.user.user_id, request)
            .await?,
    ))
}

async fn wellness_meals(
    State(state): State<AppState>,
    current: CurrentUser,
    Json(request): Json<crate::wellness::meals::Request>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _permit = state
        .health_slots
        .acquire()
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(
        state
            .database
            .meal_plans(&current.user.user_id, request)
            .await?,
    ))
}

async fn food_lookup(
    State(state): State<AppState>,
    _current: CurrentUser,
    Json(request): Json<crate::wellness::food_lookup::Request>,
) -> Result<Json<serde_json::Value>, AppError> {
    let _permit = state
        .health_slots
        .acquire()
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(crate::wellness::food_lookup::lookup(request).await?))
}
