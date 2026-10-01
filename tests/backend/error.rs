use axum::response::IntoResponse;
use helpyourself::error::AppError;
use http_body_util::BodyExt;

#[tokio::test]
async fn internal_errors_do_not_disclose_driver_or_file_details() {
    let response =
        AppError::Io(std::io::Error::other("PRIVATE_HEALTH_VALUE at secret/path")).into_response();
    assert_eq!(response.status(), 500);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(!body.contains("PRIVATE"));
    assert!(!body.contains("secret"));
    assert!(body.contains("internal_error"));
}
