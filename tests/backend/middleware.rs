use axum::{body::Body, http::Request};
use tower::ServiceExt;

#[tokio::test]
async fn responses_are_private_and_request_ids_are_server_generated() {
    let (_directory, state) = crate::fixture().await;
    let response = helpyourself::app::create_application(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/server/status")
                .header("x-request-id", "untrusted")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_ne!(response.headers()["x-request-id"], "untrusted");
}
