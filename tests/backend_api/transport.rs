use crate::{add_user, fixture, request, token};
use axum::{
    body::{Body, Bytes},
    http::{Request, StatusCode},
};
use std::{sync::Arc, time::Duration};
use tower::ServiceExt;

#[tokio::test]
async fn slow_health_bodies_are_bounded_before_buffering_and_cancellation_releases_admission() {
    let (_directory, state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let token = token(&state, "alice").await;
    let app = helpyourself::app::create_application(state.clone());
    let mut callers = Vec::new();
    for _ in 0..2 {
        let reading = Arc::new(tokio::sync::Notify::new());
        let read_in = reading.clone();
        let stream = futures_util::stream::once(async move {
            read_in.notify_one();
            std::future::pending::<Result<Bytes, std::io::Error>>().await
        });
        let slow = Request::builder()
            .method("POST")
            .uri("/api/v1/health/sync")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from_stream(stream))
            .unwrap();
        callers.push(tokio::spawn(app.clone().oneshot(slow)));
        tokio::time::timeout(Duration::from_secs(10), reading.notified())
            .await
            .unwrap();
    }
    assert_eq!(
        request(
            &app,
            "/api/v1/health/sync",
            Some(&token),
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        request(&app, "/api/v1/server/status", None, serde_json::json!({}))
            .await
            .0,
        StatusCode::OK
    );
    for caller in callers {
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
    }
    let connection = state
        .database
        .create_connection(
            &user.user_id,
            helpyourself::health::ConnectionRequest {
                platform: "apple_health".into(),
                installation_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await
        .unwrap();
    let input = serde_json::to_value(super::health_batch::batch(
        &connection.connection_id,
        vec![],
    ))
    .unwrap();
    let valid = Request::builder()
        .method("POST")
        .uri("/api/v1/health/sync")
        .header("content-type", "Application/JSON; charset=utf-8")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(serde_json::to_vec(&input).unwrap()))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(valid).await.unwrap().status(),
        StatusCode::OK
    );
    assert_eq!(
        request(
            &app,
            "/api/v1/health/sync",
            Some(&token),
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}
