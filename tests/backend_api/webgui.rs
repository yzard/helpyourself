use crate::{add_user, fixture, request, token};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use helpyourself::app::create_application;
use http_body_util::BodyExt;
use serde_json::json;
use tower::ServiceExt;

#[tokio::test]
async fn embedded_viewer_assets_are_served_with_correct_types_and_security_headers() {
    let (_directory, state) = fixture().await;
    let router = create_application(state);
    for (path, content_type, expected) in [
        ("/", "text/html; charset=utf-8", "<title>helpyourself"),
        ("/assets/app.css", "text/css; charset=utf-8", ".sidebar"),
        (
            "/assets/app.mjs",
            "text/javascript; charset=utf-8",
            "Run lipid review",
        ),
        (
            "/assets/client.mjs",
            "text/javascript; charset=utf-8",
            "class Client",
        ),
        (
            "/assets/presentation.mjs",
            "text/javascript; charset=utf-8",
            "textContent",
        ),
    ] {
        let response = router
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], content_type);
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(response.headers()["referrer-policy"], "no-referrer");
        let policy = response.headers()["content-security-policy"]
            .to_str()
            .unwrap();
        assert!(policy.contains("connect-src 'self'"));
        assert!(policy.contains("frame-ancestors 'none'"));
        assert!(!policy.contains("unsafe-inline"));
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&bytes).contains(expected));
        let head = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("HEAD")
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(head.status(), StatusCode::OK);
        assert!(
            head.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .is_empty()
        );
    }
}

#[tokio::test]
async fn viewer_does_not_swallow_unknown_assets_or_api_paths_and_keeps_data_authenticated() {
    let (_directory, state) = fixture().await;
    add_user(&state, "web-reader").await;
    let session = token(&state, "web-reader").await;
    let router = create_application(state);
    for path in [
        "/assets/missing.js",
        "/assets/Cargo.toml",
        "/raw/report.pdf",
        "/api/v1/unknown",
    ] {
        let response = router
            .clone()
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(
            response.headers()["content-type"]
                .to_str()
                .unwrap()
                .contains("application/json")
        );
    }
    for path in ["reports/list", "health/list", "analysis/list"] {
        assert_eq!(
            request(&router, &format!("/api/v1/{path}"), None, json!({}))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        request(
            &router,
            "/api/v1/reports/list",
            Some(&session),
            json!({"limit":100,"after_id":null})
        )
        .await
        .1["reports"],
        json!([])
    );
    assert_eq!(
        request(
            &router,
            "/api/v1/health/list",
            Some(&session),
            json!({"limit":50,"offset":0})
        )
        .await
        .1["records"],
        json!([])
    );
}
