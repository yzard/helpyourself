use crate::{add_user, archive, fixture};

#[tokio::test]
async fn actual_http_extraction_creates_pending_candidates_with_provenance() {
    let (_directory, mut state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let (address, server) = super::provider::mock().await;
    let mut config = (*state.config).clone();
    config.providers.ocr.enabled = true;
    config.providers.ocr.adapter = "openai_chat".into();
    config.providers.ocr.base_url = address;
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
async fn pdf_rendering_and_unlimited_ocr_parser_pipeline_create_reviewable_rows() {
    let (_directory, mut state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let (address, server) = super::provider::mock().await;
    let mut config = (*state.config).clone();
    config.providers.ocr.enabled = true;
    config.providers.ocr.base_url = address.clone();
    let parser = config.providers.document_parser.as_mut().unwrap();
    parser.enabled = true;
    parser.base_url = address;
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
    assert_eq!(archived["extraction_outputs"].as_array().unwrap().len(), 2);
    server.abort();
}

#[tokio::test]
async fn malformed_ocr_is_archived_before_validation_and_obeys_owner_and_deletion() {
    use helpyourself::reports::ExtractionOutputRequest;
    use serde_json::json;
    let (_directory, mut state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let other = add_user(&state, "bob").await;
    let body = json!({"choices":[{"message":{"content":"An original row with <5, but not JSON"}}],"usage":{"tokens":1}}).to_string();
    let response = body.clone();
    let router = axum::Router::new().route(
        "/v1/chat/completions",
        axum::routing::post(move || {
            let response = response.clone();
            async move { response }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let mut config = (*state.config).clone();
    config.providers.ocr.enabled = true;
    config.providers.ocr.adapter = "openai_chat".into();
    config.providers.ocr.base_url = format!("http://{address}/v1");
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
    assert_eq!(original["status_code"], 200);
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
    let app = axum::Router::new().route("/v1/chat/completions",axum::routing::post(move || {
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
    config.providers.ocr.enabled = true;
    config.providers.ocr.adapter = "openai_chat".into();
    config.providers.ocr.base_url = format!("http://{address}/v1");
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
