mod analysis;
mod app;
mod authentication;
mod cli;
mod config;
mod database;
mod documents;
#[path = "main.rs"]
mod entrypoint;
mod error;
mod execution;
mod files;
mod health;
mod health_batch;
mod jobs;
mod laboratory;
mod lifecycle;
mod maintenance;
mod middleware;
mod models;
mod provider;
mod raw;
mod reports;
mod routes;
mod temporary;
mod transport;
mod webgui;
mod worker;

pub fn observation_payload() -> helpyourself::reports::ObservationPayload {
    serde_json::from_value(json!({"raw_name":"LDL Cholesterol","raw_result":"120","raw_unit":"mg/dL","reference_range":"<100","report_flag":"H",
        "sampled_at":"2026-08-01","metric_id":"ldl_cholesterol","source":{"page":1,"quote":"LDL Cholesterol 120 mg/dL","bounding_box":null},"notes":null})).unwrap()
}

pub async fn archive(state: &AppState, username: &str) -> String {
    let token = token(state, username).await;
    let (status, response) = upload(
        &helpyourself::app::create_application(state.clone()),
        &token,
        &uuid::Uuid::new_v4().to_string(),
        &png(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{response}");
    response["file"]["file_id"].as_str().unwrap().into()
}

pub async fn confirm(state: &AppState, user_id: &str, report_id: &str) -> String {
    let report = state
        .database
        .report_summary(user_id, report_id)
        .await
        .unwrap();
    let response = state
        .database
        .review(
            user_id,
            helpyourself::reports::ReviewRequest {
                report_id: report_id.into(),
                expected_revision: report.revision,
                context: None,
                observations: vec![helpyourself::reports::ReviewItem {
                    observation_id: None,
                    expected_revision: None,
                    status: "confirmed".into(),
                    payload: observation_payload(),
                }],
            },
        )
        .await
        .unwrap();
    response["observations"][0]["observation_id"]
        .as_str()
        .unwrap()
        .into()
}

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use helpyourself::{
    app::AppState,
    authentication::hash_password,
    config::{Config, TEMPLATE},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

pub async fn fixture() -> (TempDir, AppState) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    std::fs::write(&path, configured_template()).unwrap();
    let config = Config::load(directory.path()).unwrap();
    let state = AppState::open(config).await.unwrap();
    (directory, state)
}

pub fn configured_template() -> String {
    TEMPLATE.replacen(
        "api_key = \"\"",
        "api_key = \"synthetic-ocr-service-key-123456789\"",
        1,
    )
}

pub async fn add_user(state: &AppState, username: &str) -> helpyourself::models::User {
    let hash = hash_password("correct-password-123".into()).await.unwrap();
    state
        .database
        .create_user(username, &hash, 1)
        .await
        .unwrap()
}

pub async fn request(
    router: &Router,
    path: &str,
    token: Option<&str>,
    payload: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let response = router
        .clone()
        .oneshot(builder.body(Body::from(payload.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&bytes)})),
    )
}

pub async fn token(state: &AppState, username: &str) -> String {
    helpyourself::authentication::login(
        state,
        helpyourself::models::LoginRequest {
            username: username.into(),
            password: "correct-password-123".into(),
        },
    )
    .await
    .unwrap()
    .token
}

pub fn png() -> Vec<u8> {
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(2, 2)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    encoded.into_inner()
}

pub async fn upload(
    router: &Router,
    token: &str,
    upload_id: &str,
    content: &[u8],
) -> (StatusCode, Value) {
    let mut payload = b"--boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"report.png\"\r\nContent-Type: image/png\r\n\r\n".to_vec();
    payload.extend_from_slice(content);
    payload.extend_from_slice(b"\r\n--boundary--\r\n");
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/files/upload")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "multipart/form-data; boundary=boundary")
                .header("x-upload-id", upload_id)
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"text":String::from_utf8_lossy(&bytes)})),
    )
}
