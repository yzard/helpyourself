use crate::{add_user, archive, confirm, fixture, observation_payload};
use helpyourself::laboratory::normalized;
use helpyourself::reports::{ReviewItem, ReviewRequest, TrendRequest, trends};

#[tokio::test]
async fn partial_review_history_and_conflicts_are_transactional() {
    let (_directory, state) = fixture().await;
    let alice = add_user(&state, "alice").await;
    let bob = add_user(&state, "bob").await;
    let report = archive(&state, "alice").await;
    let request = ReviewRequest {
        report_id: report.clone(),
        expected_revision: 1,
        context: None,
        observations: vec![
            ReviewItem {
                observation_id: None,
                expected_revision: None,
                status: "confirmed".into(),
                payload: observation_payload(),
            },
            ReviewItem {
                observation_id: None,
                expected_revision: None,
                status: "pending".into(),
                payload: observation_payload(),
            },
        ],
    };
    state
        .database
        .review(&alice.user_id, request)
        .await
        .unwrap();
    assert_eq!(
        state
            .database
            .confirmed(&alice.user_id)
            .await
            .unwrap()
            .len(),
        1
    );
    let rows = state
        .database
        .observations(&alice.user_id, &report)
        .await
        .unwrap();
    let id = rows
        .iter()
        .find(|row| row.status == "confirmed")
        .unwrap()
        .observation_id
        .clone();
    let mut edited = observation_payload();
    edited.raw_result = "130".into();
    state
        .database
        .review(
            &alice.user_id,
            ReviewRequest {
                report_id: report.clone(),
                expected_revision: 2,
                context: None,
                observations: vec![ReviewItem {
                    observation_id: Some(id.clone()),
                    expected_revision: Some(1),
                    status: "confirmed".into(),
                    payload: edited,
                }],
            },
        )
        .await
        .unwrap();
    assert_eq!(
        state
            .database
            .observation_history(&alice.user_id, &id)
            .await
            .unwrap()["history"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        state
            .database
            .observation_history(&bob.user_id, &id)
            .await
            .is_err()
    );
    assert!(
        state
            .database
            .review(
                &alice.user_id,
                ReviewRequest {
                    report_id: report.clone(),
                    expected_revision: 2,
                    context: None,
                    observations: vec![]
                }
            )
            .await
            .is_err()
    );
    assert_eq!(
        state
            .database
            .report_summary(&alice.user_id, &report)
            .await
            .unwrap()
            .revision,
        3
    );
    let result = trends(
        &state.database,
        &alice.user_id,
        TrendRequest {
            metric_ids: vec!["ldl_cholesterol".into()],
            start_at: None,
            end_at: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(result["points"][0]["value"], "130");
}

#[tokio::test]
async fn duplicate_relations_exclude_points_and_forbid_cycles() {
    let (_directory, state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let first = archive(&state, "alice").await;
    let second = archive(&state, "alice").await;
    confirm(&state, &user.user_id, &first).await;
    confirm(&state, &user.user_id, &second).await;
    state
        .database
        .relate_reports(&user.user_id, &first, Some(&second), "duplicate", 2)
        .await
        .unwrap();
    assert_eq!(
        state.database.confirmed(&user.user_id).await.unwrap().len(),
        1
    );
    assert!(
        state
            .database
            .relate_reports(&user.user_id, &second, Some(&first), "duplicate", 2)
            .await
            .is_err()
    );
    state
        .database
        .relate_reports(&user.user_id, &first, None, "duplicate", 3)
        .await
        .unwrap();
    assert_eq!(
        state.database.confirmed(&user.user_id).await.unwrap().len(),
        2
    );
}

#[test]
fn unit_conversion_never_rewrites_comparators_or_unknown_units() {
    let mut payload = observation_payload();
    payload.raw_unit = Some("mmol/L".into());
    payload.raw_result = "2.586".into();
    assert_eq!(
        normalized(&payload).unwrap(),
        ("100".into(), "mg/dL".into())
    );
    payload.raw_result = "<2.586".into();
    assert!(normalized(&payload).is_none());
    payload.raw_result = "2.586".into();
    payload.raw_unit = Some("unknown".into());
    assert!(normalized(&payload).is_none());
    payload.source.page = 2;
    assert!(payload.validate(1).is_err());
}

#[tokio::test]
async fn confirmed_units_share_scale_and_unknown_values_report_specific_reasons() {
    let (_directory, state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let report = archive(&state, "alice").await;
    let values = [
        ("2.586", "mmol/L"),
        ("100", "mg/dL"),
        ("<2.586", "mmol/L"),
        ("positive", "mg/dL"),
        ("5", "unknown"),
    ];
    let observations = values
        .into_iter()
        .map(|(value, unit)| {
            let mut payload = observation_payload();
            payload.raw_result = value.into();
            payload.raw_unit = Some(unit.into());
            payload.reference_range = Some("<2.586 mmol/L".into());
            ReviewItem {
                observation_id: None,
                expected_revision: None,
                status: "confirmed".into(),
                payload,
            }
        })
        .collect();
    let result = state
        .database
        .review(
            &user.user_id,
            ReviewRequest {
                report_id: report,
                expected_revision: 1,
                context: None,
                observations,
            },
        )
        .await
        .unwrap();
    assert_eq!(result["observations"].as_array().unwrap().len(), 5);
    let derived = trends(
        &state.database,
        &user.user_id,
        TrendRequest {
            metric_ids: vec!["ldl_cholesterol".into()],
            start_at: None,
            end_at: None,
        },
    )
    .await
    .unwrap();
    let points = derived["points"].as_array().unwrap();
    assert_eq!(points.len(), 2);
    assert!(
        points
            .iter()
            .all(|point| point["value"] == "100" && point["reference"]["upper"] == "100")
    );
    assert_eq!(derived["incomparable_count"], 3);
    let reasons = derived["excluded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["reason"].as_str().unwrap())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(
        reasons,
        ["comparison", "unparsed_value", "unsupported_unit"]
            .into_iter()
            .collect()
    );
}
