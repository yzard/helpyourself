use crate::{add_user, fixture, png, request, token, upload};
use axum::http::StatusCode;
use serde_json::json;

#[tokio::test]
async fn iphone_contract_review_trend_export_and_account_isolation() {
    let (_directory, state) = fixture().await;
    add_user(&state, "alice").await;
    add_user(&state, "bob").await;
    let alice = token(&state, "alice").await;
    let bob = token(&state, "bob").await;
    let app = helpyourself::app::create_application(state.clone());
    let (_, uploaded) = upload(&app, &alice, &uuid::Uuid::new_v4().to_string(), &png()).await;
    let report = uploaded["file"]["file_id"].as_str().unwrap();
    let review = json!({"report_id":report,"expected_revision":1,"context":{"fasting":true},"observations":[{"observation_id":null,"expected_revision":null,"status":"confirmed","payload":crate::observation_payload()}]});
    assert_eq!(
        request(&app, "/api/v1/reports/review", Some(&bob), review.clone())
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (status, result) = request(&app, "/api/v1/reports/review", Some(&alice), review).await;
    assert_eq!(status, StatusCode::OK, "{result}");
    assert_eq!(result["report"]["revision"], 2);
    let observation = result["observations"][0]["observation_id"]
        .as_str()
        .unwrap();
    let (status, trend) = request(
        &app,
        "/api/v1/trends/get",
        Some(&alice),
        json!({"metric_ids":["ldl_cholesterol"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(trend["points"][0]["report_id"], report);
    assert_eq!(trend["points"][0]["value"], "120");
    for (route, body) in [
        ("reports/get", json!({"report_id":report})),
        (
            "reports/delete",
            json!({"report_id":report,"expected_revision":2}),
        ),
        (
            "observations/history",
            json!({"observation_id":observation}),
        ),
        ("jobs/get", json!({"job_id":uploaded["job"]["job_id"]})),
    ] {
        assert_eq!(
            request(&app, &format!("/api/v1/{route}"), Some(&bob), body)
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }
    let (status, export) = request(&app, "/api/v1/exports/create", Some(&alice), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        request(&app, "/api/v1/exports/delete", Some(&bob), export.clone())
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    helpyourself::maintenance::export_next(&state)
        .await
        .unwrap();
    assert_eq!(
        request(
            &app,
            "/api/v1/user/delete",
            Some(&alice),
            json!({"confirmation":"wrong"})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &app,
            "/api/v1/user/delete",
            Some(&alice),
            json!({"confirmation":"alice"})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &app,
            "/api/v1/reports/list",
            Some(&alice),
            json!({"limit":100})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}
