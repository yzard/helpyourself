use crate::{add_user, fixture, png, request, token, upload};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use helpyourself::app::create_application;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn archive_replay_download_and_cross_user_isolation() {
    let (_directory, state) = fixture().await;
    add_user(&state, "alice").await;
    add_user(&state, "bob").await;
    let alice = token(&state, "alice").await;
    let bob = token(&state, "bob").await;
    let router = create_application(state.clone());
    let upload_id = uuid::Uuid::new_v4().to_string();
    let (status, first) = upload(&router, &alice, &upload_id, &png()).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["file"]["content_type"], "image/png");
    let (_, second) = upload(&router, &alice, &upload_id, &png()).await;
    assert_eq!(first["file"]["file_id"], second["file"]["file_id"]);
    assert_eq!(second["replayed"], true);
    let file_id = first["file"]["file_id"].as_str().unwrap();
    assert_eq!(
        request(
            &router,
            "/api/v1/files/get",
            Some(&bob),
            json!({"file_id":file_id})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let (_, listing) = request(
        &router,
        "/api/v1/files/list",
        Some(&alice),
        json!({"limit":100}),
    )
    .await;
    assert_eq!(listing["files"].as_array().unwrap().len(), 1);
    let (_, listing) = request(
        &router,
        "/api/v1/files/list",
        Some(&bob),
        json!({"limit":100}),
    )
    .await;
    assert!(listing["files"].as_array().unwrap().is_empty());
    for (session, expected) in [(&alice, StatusCode::OK), (&bob, StatusCode::NOT_FOUND)] {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/files/{file_id}/download"))
                    .header("authorization", format!("Bearer {session}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        if expected == StatusCode::OK {
            assert_eq!(
                response
                    .into_body()
                    .collect()
                    .await
                    .unwrap()
                    .to_bytes()
                    .as_ref(),
                png()
            );
        }
    }
    helpyourself::maintenance::cleanup(&state).await.unwrap();
    assert!(
        std::fs::read_dir(state.config.server.data_dir.join("tmp"))
            .unwrap()
            .next()
            .is_none()
    );
}

#[tokio::test]
async fn protected_routes_require_session_and_logout_revokes_it() {
    let (_directory, state) = fixture().await;
    add_user(&state, "alice").await;
    let router = create_application(state);
    assert_eq!(
        request(&router, "/api/v1/user/get", None, json!({}))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, response) = request(
        &router,
        "/api/v1/session/login",
        None,
        json!({"username":"alice","password":"correct-password-123"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = response["token"].as_str().unwrap();
    assert_eq!(
        request(&router, "/api/v1/user/get", Some(token), json!({}))
            .await
            .1["username"],
        "alice"
    );
    assert_eq!(
        request(&router, "/api/v1/session/logout", Some(token), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        request(&router, "/api/v1/user/get", Some(token), json!({}))
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn invalid_upload_does_not_create_records_or_leave_temporary_files() {
    let (_directory, state) = fixture().await;
    add_user(&state, "alice").await;
    let token = token(&state, "alice").await;
    let router = create_application(state.clone());
    assert_eq!(
        upload(
            &router,
            &token,
            &uuid::Uuid::new_v4().to_string(),
            b"invalid"
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        upload(&router, &token, "../escape", &png()).await.0,
        StatusCode::BAD_REQUEST
    );
    helpyourself::maintenance::cleanup(&state).await.unwrap();
    assert!(
        std::fs::read_dir(state.config.server.data_dir.join("tmp"))
            .unwrap()
            .next()
            .is_none()
    );
    assert_eq!(
        request(
            &router,
            "/api/v1/files/list",
            Some(&token),
            json!({"limit":100})
        )
        .await
        .1["files"],
        json!([])
    );
}

#[tokio::test]
async fn concurrent_replays_and_reused_ids_do_not_duplicate_or_overwrite() {
    let (_directory, state) = fixture().await;
    add_user(&state, "alice").await;
    let token = token(&state, "alice").await;
    let router = create_application(state.clone());
    let upload_id = uuid::Uuid::new_v4().to_string();
    let content = png();
    let (first, second) = tokio::join!(
        upload(&router, &token, &upload_id, &content),
        upload(&router, &token, &upload_id, &content)
    );
    assert_eq!(first.0, StatusCode::OK, "{}", first.1);
    assert_eq!(second.0, StatusCode::OK, "{}", second.1);
    assert_eq!(first.1["file"]["file_id"], second.1["file"]["file_id"]);
    let mut changed = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(3, 3)
        .write_to(&mut changed, image::ImageFormat::Png)
        .unwrap();
    assert_eq!(
        upload(&router, &token, &upload_id, changed.get_ref())
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (_, listing) = request(
        &router,
        "/api/v1/jobs/list",
        Some(&token),
        json!({"limit":100}),
    )
    .await;
    assert_eq!(listing["jobs"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn large_valid_upload_has_route_specific_limit_and_excess_is_rejected() {
    let (_directory, mut state) = fixture().await;
    add_user(&state, "alice").await;
    let token = token(&state, "alice").await;
    let mut image = image::RgbImage::new(200, 200);
    for (index, pixel) in image.pixels_mut().enumerate() {
        let hash = helpyourself::authentication::digest(&index.to_le_bytes());
        *pixel = image::Rgb([hash.as_bytes()[0], hash.as_bytes()[1], hash.as_bytes()[2]]);
    }
    let mut encoded = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image)
        .write_to(&mut encoded, image::ImageFormat::Png)
        .unwrap();
    assert!(encoded.get_ref().len() > 16_384);
    assert_eq!(
        upload(
            &create_application(state.clone()),
            &token,
            &uuid::Uuid::new_v4().to_string(),
            encoded.get_ref()
        )
        .await
        .0,
        StatusCode::OK
    );
    let mut configuration = (*state.config).clone();
    configuration.storage.maximum_upload_bytes = 1024;
    state.config = std::sync::Arc::new(configuration);
    assert_eq!(
        upload(
            &create_application(state),
            &token,
            &uuid::Uuid::new_v4().to_string(),
            encoded.get_ref()
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
}

#[tokio::test]
async fn archived_files_and_sessions_survive_server_restart() {
    let (directory, state) = fixture().await;
    add_user(&state, "alice").await;
    let token = token(&state, "alice").await;
    let router = create_application(state.clone());
    let (_, response) = upload(&router, &token, &uuid::Uuid::new_v4().to_string(), &png()).await;
    let file_id = response["file"]["file_id"].as_str().unwrap().to_owned();
    state.database.close().await;
    drop(router);
    drop(state);
    let configuration =
        helpyourself::config::Config::load(&directory.path().join("config.toml")).unwrap();
    let reopened = helpyourself::app::AppState::open(configuration)
        .await
        .unwrap();
    let router = create_application(reopened);
    assert_eq!(
        request(
            &router,
            "/api/v1/files/get",
            Some(&token),
            json!({"file_id":file_id})
        )
        .await
        .0,
        StatusCode::OK
    );
}
