use crate::app::AppState;
use axum::{Router, http::header, response::IntoResponse, routing::get};

const POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' blob:; frame-src blob:; font-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'; object-src 'none'";

fn asset(content_type: &'static str, bytes: &'static [u8]) -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_SECURITY_POLICY, POLICY),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CACHE_CONTROL, "no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(|| async {
                asset(
                    "text/html; charset=utf-8",
                    include_bytes!("../frontend/index.html"),
                )
            }),
        )
        .route(
            "/assets/app.css",
            get(|| async {
                asset(
                    "text/css; charset=utf-8",
                    include_bytes!("../frontend/app.css"),
                )
            }),
        )
        .route(
            "/assets/app.mjs",
            get(|| async {
                asset(
                    "text/javascript; charset=utf-8",
                    include_bytes!("../frontend/app.mjs"),
                )
            }),
        )
        .route(
            "/assets/client.mjs",
            get(|| async {
                asset(
                    "text/javascript; charset=utf-8",
                    include_bytes!("../frontend/client.mjs"),
                )
            }),
        )
        .route(
            "/assets/presentation.mjs",
            get(|| async {
                asset(
                    "text/javascript; charset=utf-8",
                    include_bytes!("../frontend/presentation.mjs"),
                )
            }),
        )
}
