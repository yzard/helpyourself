use axum::{
    extract::{MatchedPath, Request},
    http::{HeaderValue, header},
    middleware::Next,
    response::Response,
};
use std::time::Instant;

pub async fn request_logging(request: Request, next: Next) -> Response {
    let started = Instant::now();
    let method = request.method().clone();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_owned())
        .unwrap_or_else(|| "unmatched".into());
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    if let Ok(header) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", header);
    }
    tracing::info!(%request_id, %method, %route, status = response.status().as_u16(), elapsed_ms = started.elapsed().as_millis(), "request completed");
    response
}
