use crate::{add_user, archive, fixture};

#[tokio::test]
async fn actual_http_extraction_creates_pending_candidates_with_provenance() {
    let (_directory, mut state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let (address, server) = super::provider::mock().await;
    let mut config = (*state.config).clone();
    config.ocr.enabled = true;
    config.ocr.url = address.trim_end_matches("/v1").into();
    state.config = std::sync::Arc::new(config);
    let report = archive(&state, "alice").await;
    assert!(helpyourself::worker::extract_next(&state).await.unwrap());
    let rows = state
        .database
        .observations(&user.user_id, &report)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "pending");
    assert_eq!(rows[0].payload.source.page, 1);
    assert!(rows[0].payload.metric_id.is_none());
    assert!(
        state
            .database
            .confirmed(&user.user_id)
            .await
            .unwrap()
            .is_empty()
    );
    let archived = state.database.report(&user.user_id, &report).await.unwrap();
    assert_eq!(archived["extraction_outputs"].as_array().unwrap().len(), 1);
    assert_eq!(
        state
            .database
            .job_by_file(&user.user_id, &report)
            .await
            .unwrap()
            .status,
        "succeeded"
    );
    server.abort();
}

#[tokio::test]
async fn pdf_rendering_and_independent_ocr_service_create_reviewable_rows() {
    let (_directory, mut state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let (address, server) = super::provider::mock().await;
    let mut config = (*state.config).clone();
    config.ocr.enabled = true;
    config.ocr.url = address.trim_end_matches("/v1").into();
    state.config = std::sync::Arc::new(config);
    let app = helpyourself::app::create_application(state.clone());
    let token = crate::token(&state, "alice").await;
    let (status, result) = crate::upload(
        &app,
        &token,
        &uuid::Uuid::new_v4().to_string(),
        &super::files::synthetic_pdf(),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(result["file"]["content_type"], "application/pdf");
    helpyourself::worker::extract_next(&state).await.unwrap();
    let report = result["file"]["file_id"].as_str().unwrap();
    assert_eq!(
        state
            .database
            .observations(&user.user_id, report)
            .await
            .unwrap()
            .len(),
        1
    );
    let archived = state.database.report(&user.user_id, report).await.unwrap();
    assert_eq!(archived["extraction_outputs"].as_array().unwrap().len(), 1);
    server.abort();
}

#[tokio::test]
async fn malformed_ocr_is_archived_before_validation_and_obeys_owner_and_deletion() {
    use helpyourself::reports::ExtractionOutputRequest;
    use serde_json::json;
    let (_directory, mut state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let other = add_user(&state, "bob").await;
    let body = json!({"model":"qwen3.8-27b-ninfer-nvfp4","engine":"ninfer","content":"An original row with <5, but not JSON","raw_response_body":"Complete native failure reply","error":{"code":"invalid_structured_output"}}).to_string();
    let response = body.clone();
    let router = axum::Router::new().route(
        "/api/v1/documents/extract",
        axum::routing::post(move || {
            let response = response.clone();
            async move { (axum::http::StatusCode::UNPROCESSABLE_ENTITY, response) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut config = (*state.config).clone();
    config.ocr.enabled = true;
    config.ocr.url = format!("http://{address}");
    state.config = std::sync::Arc::new(config);
    let report_id = archive(&state, "alice").await;
    helpyourself::worker::extract_next(&state).await.unwrap();
    assert_eq!(
        state
            .database
            .job_by_file(&user.user_id, &report_id)
            .await
            .unwrap()
            .status,
        "failed"
    );
    let report = state
        .database
        .report(&user.user_id, &report_id)
        .await
        .unwrap();
    let output = &report["extraction_outputs"][0];
    let request = || ExtractionOutputRequest {
        report_id: report_id.clone(),
        run_id: output["run_id"].as_str().unwrap().into(),
        page: 1,
        stage: "ocr".into(),
    };
    let original = state
        .database
        .extraction_output(&user.user_id, request())
        .await
        .unwrap();
    assert_eq!(original["response_body"], body);
    assert_eq!(original["status_code"], 422);
    assert_eq!(original["content"], "An original row with <5, but not JSON");
    assert!(
        state
            .database
            .extraction_output(&other.user_id, request())
            .await
            .is_err()
    );
    state
        .database
        .delete_report(
            &user.user_id,
            &report_id,
            report["report"]["revision"].as_i64().unwrap(),
        )
        .await
        .unwrap();
    assert!(
        state
            .database
            .extraction_output(&user.user_id, request())
            .await
            .is_err()
    );
    server.abort();
}

#[tokio::test]
async fn deleted_report_rejects_late_ocr_and_clears_temporary_images() {
    use std::sync::Arc;
    let (_directory, mut state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let started = Arc::new(tokio::sync::Notify::new());
    let released = Arc::new(tokio::sync::Notify::new());
    let started_in = started.clone();
    let released_in = released.clone();
    let app = axum::Router::new().route("/api/v1/documents/extract",axum::routing::post(move || {
        let started = started_in.clone(); let released = released_in.clone();
        async move {
            started.notify_one(); released.notified().await;
            axum::Json(serde_json::json!({"choices":[{"message":{"content":serde_json::json!({"observations":[crate::observation_payload()],"warnings":[]}).to_string()}}]}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let mut config = (*state.config).clone();
    config.ocr.enabled = true;
    config.ocr.url = format!("http://{address}");
    state.config = Arc::new(config);
    let report = archive(&state, "alice").await;
    let worker_state = state.clone();
    let worker =
        tokio::spawn(async move { helpyourself::worker::extract_next(&worker_state).await });
    tokio::time::timeout(std::time::Duration::from_secs(10), started.notified())
        .await
        .unwrap();
    state
        .database
        .delete_report(&user.user_id, &report, 1)
        .await
        .unwrap();
    released.notify_one();
    assert!(worker.await.unwrap().is_err());
    helpyourself::maintenance::cleanup(&state).await.unwrap();
    assert!(
        state
            .database
            .observations(&user.user_id, &report)
            .await
            .is_err()
    );
    server.abort();
}

#[tokio::test]
async fn long_extraction_renews_lease_and_releases_renewal_on_completion() {
    use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};
    use std::{sync::Arc, time::Duration};
    const READ_LEASE: &str = "SELECT lease_until FROM jobs WHERE job_id = ?";
    const WRITE_REVISION: &str =
        "UPDATE users SET data_revision = data_revision + 1 WHERE user_id = ?";
    let (_directory, mut state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let started = Arc::new(tokio::sync::Notify::new());
    let released = Arc::new(tokio::sync::Notify::new());
    let start_in = started.clone();
    let release_in = released.clone();
    let router = axum::Router::new().route(
        "/api/v1/documents/extract",
        axum::routing::post(move || {
            let started = start_in.clone();
            let released = release_in.clone();
            async move {
                started.notify_one();
                released.notified().await;
                let mut observation = crate::observation_payload();
                observation.metric_id = None;
                let page = serde_json::json!({"observations":[observation],"warnings":[]});
                axum::Json(serde_json::json!({
                    "model":"synthetic-model", "engine":"ninfer", "prompt_version":"laboratory-page-v2",
                    "content":page.to_string(), "raw_response_body":"synthetic native output",
                    "observations":page["observations"], "warnings":[]
                }))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut config = (*state.config).clone();
    config.ocr.enabled = true;
    config.ocr.url = format!("http://{address}");
    config.jobs.lease_seconds = 3;
    state.config = Arc::new(config);
    let report_id = archive(&state, "alice").await;
    let worker_state = state.clone();
    let worker =
        tokio::spawn(async move { helpyourself::worker::extract_next(&worker_state).await });
    tokio::time::timeout(Duration::from_secs(10), started.notified())
        .await
        .unwrap();
    let original = state
        .database
        .job_by_file(&user.user_id, &report_id)
        .await
        .unwrap();
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(state.config.server.data_dir.join("database.sqlite"))
            .busy_timeout(Duration::from_secs(5)),
    )
    .await
    .unwrap();
    let initial_deadline: i64 = sqlx::query_scalar(READ_LEASE)
        .bind(&original.job_id)
        .fetch_one(&mut connection)
        .await
        .unwrap();
    // Longer than the initial lease, with database writes while HTTP inference is pending.
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let mut transaction = connection.begin_with("BEGIN IMMEDIATE").await.unwrap();
        sqlx::query(WRITE_REVISION)
            .bind(&user.user_id)
            .execute(&mut *transaction)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
    }
    let renewed = state
        .database
        .job_by_file(&user.user_id, &report_id)
        .await
        .unwrap();
    let renewed_deadline: i64 = sqlx::query_scalar(READ_LEASE)
        .bind(&original.job_id)
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert!(renewed_deadline > initial_deadline);
    assert_eq!(renewed.status, "running");
    released.notify_one();
    assert!(
        tokio::time::timeout(Duration::from_secs(10), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    );
    let completed = state
        .database
        .job_by_file(&user.user_id, &report_id)
        .await
        .unwrap();
    assert_eq!(completed.status, "succeeded");
    let cleared: Option<i64> = sqlx::query_scalar(READ_LEASE)
        .bind(&original.job_id)
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert!(cleared.is_none());
    assert_eq!(
        state
            .database
            .observations(&user.user_id, &report_id)
            .await
            .unwrap()
            .len(),
        1
    );
    server.abort();
}

#[tokio::test]
async fn complete_ocr_with_review_warnings_succeeds_without_confirming_or_losing_evidence() {
    let (_directory, mut state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let mut observation = crate::observation_payload();
    observation.metric_id = None;
    let page = serde_json::json!({"observations":[observation],"warnings":["Text layer contains only the footer; values were read from the image."]});
    let output = serde_json::json!({"model":"synthetic","engine":"ninfer","prompt_version":"laboratory-page-v2",
        "content":page.to_string(), "raw_response_body":"synthetic full native response",
        "observations":page["observations"], "warnings":page["warnings"]});
    let router = axum::Router::new().route(
        "/api/v1/documents/extract",
        axum::routing::post(move || {
            let output = output.clone();
            async move { axum::Json(output) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut config = (*state.config).clone();
    config.ocr.enabled = true;
    config.ocr.url = format!("http://{address}");
    state.config = std::sync::Arc::new(config);
    let report_id = archive(&state, "alice").await;
    helpyourself::worker::extract_next(&state).await.unwrap();
    assert_eq!(
        state
            .database
            .job_by_file(&user.user_id, &report_id)
            .await
            .unwrap()
            .status,
        "succeeded"
    );
    let report = state
        .database
        .report(&user.user_id, &report_id)
        .await
        .unwrap();
    assert_eq!(report["pages"][0]["status"], "needs_review");
    assert_eq!(report["observations"][0]["status"], "pending");
    assert!(report["observations"][0]["payload"]["metric_id"].is_null());
    assert!(
        report["pages"][0]["content"]
            .as_str()
            .unwrap()
            .contains("footer")
    );
    assert!(
        state
            .database
            .confirmed(&user.user_id)
            .await
            .unwrap()
            .is_empty()
    );
    server.abort();
}
