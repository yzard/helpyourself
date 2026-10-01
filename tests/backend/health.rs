use crate::{add_user, fixture};
use helpyourself::health::{AggregateRequest, ConnectionRequest, HealthRecordInput, SyncRequest};
use serde_json::json;

fn record() -> HealthRecordInput {
    HealthRecordInput {
        record_id: "sample-1".into(),
        source_id: "watch".into(),
        record_type: "steps".into(),
        start_at: 1772946000,
        end_at: 1772949600,
        version: 1,
        deleted: false,
        payload: json!({"value":100,"unit":"count"}),
    }
}
fn batch(connection: &str, records: Vec<HealthRecordInput>) -> SyncRequest {
    SyncRequest {
        connection_id: connection.into(),
        batch_id: uuid::Uuid::new_v4().to_string(),
        record_type: "steps".into(),
        coverage_status: "observed".into(),
        records,
    }
}

#[tokio::test]
async fn batch_replay_is_atomic_and_wildcard_deletion_cannot_resurrect() {
    let (_directory, state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let other = add_user(&state, "bob").await;
    let connection = state
        .database
        .create_connection(
            &user.user_id,
            ConnectionRequest {
                platform: "apple_health".into(),
                installation_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await
        .unwrap();
    let request = batch(&connection.connection_id, vec![record()]);
    let encoded = serde_json::to_string(&request).unwrap();
    state
        .database
        .sync_health(&user.user_id, request)
        .await
        .unwrap();
    assert_eq!(
        state
            .database
            .sync_health(&user.user_id, serde_json::from_str(&encoded).unwrap())
            .await
            .unwrap()["replayed"],
        true
    );
    assert!(
        state
            .database
            .sync_health(&other.user_id, serde_json::from_str(&encoded).unwrap())
            .await
            .is_err()
    );
    let mut changed: SyncRequest = serde_json::from_str(&encoded).unwrap();
    changed.records[0].payload["value"] = json!(999);
    assert!(
        state
            .database
            .sync_health(&user.user_id, changed)
            .await
            .is_err()
    );
    let mut deleted = record();
    deleted.deleted = true;
    deleted.source_id = "*".into();
    deleted.payload = json!({});
    state
        .database
        .sync_health(
            &user.user_id,
            batch(&connection.connection_id, vec![deleted]),
        )
        .await
        .unwrap();
    let mut old = record();
    old.version = 100;
    state
        .database
        .sync_health(&user.user_id, batch(&connection.connection_id, vec![old]))
        .await
        .unwrap();
    assert_eq!(
        state
            .database
            .health_records(&user.user_id, 100, 0)
            .await
            .unwrap()["records"],
        json!([])
    );
}

#[tokio::test]
async fn steps_from_different_sources_are_not_added_and_dst_uses_real_day_boundaries() {
    let (_directory, state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let connection = state
        .database
        .create_connection(
            &user.user_id,
            ConnectionRequest {
                platform: "apple_health".into(),
                installation_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await
        .unwrap();
    let mut first = record();
    first.start_at = helpyourself::reports::parse_time("2026-03-08T00:00:00-05:00").unwrap();
    first.end_at = helpyourself::reports::parse_time("2026-03-09T00:00:00-04:00").unwrap();
    first.payload["value"] = json!(2300);
    let mut second = first.clone();
    second.source_id = "phone".into();
    state
        .database
        .sync_health(
            &user.user_id,
            batch(&connection.connection_id, vec![first, second]),
        )
        .await
        .unwrap();
    let result = state
        .database
        .aggregate_health(
            &user.user_id,
            AggregateRequest {
                record_type: "steps".into(),
                start_date: "2026-03-08".into(),
                end_date: "2026-03-08".into(),
                timezone: "America/New_York".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(result["days"].as_array().unwrap().len(), 2);
    assert_eq!(result["days"][0]["value"], 2300.0);
    assert_eq!(
        result["source_policy"],
        "separate_sources_no_cross_source_sum"
    );
}

#[tokio::test]
async fn failed_query_preserves_success_coverage_and_invalidates_old_export() {
    let (_directory, state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let connection = state
        .database
        .create_connection(
            &user.user_id,
            ConnectionRequest {
                platform: "apple_health".into(),
                installation_id: uuid::Uuid::new_v4().to_string(),
            },
        )
        .await
        .unwrap();
    state
        .database
        .sync_health(
            &user.user_id,
            batch(&connection.connection_id, vec![record()]),
        )
        .await
        .unwrap();
    let export = state.database.request_export(&user.user_id).await.unwrap();
    helpyourself::maintenance::export_next(&state)
        .await
        .unwrap();
    let mut failure = batch(&connection.connection_id, vec![]);
    failure.coverage_status = "error".into();
    state
        .database
        .sync_health(&user.user_id, failure)
        .await
        .unwrap();
    let coverage = state.database.health_coverage(&user.user_id).await.unwrap();
    let row = &coverage["coverage"][0];
    assert_eq!(row["status"], "error");
    assert!(row["last_success_at"].as_i64().is_some());
    assert_eq!(row["visible_start_at"], record().start_at);
    assert!(
        state
            .database
            .export_ready(&user.user_id, &export)
            .await
            .is_err()
    );
}

#[test]
fn sleep_overlaps_merge_and_heart_rate_ignores_unknown_units() {
    use helpyourself::health::{StoredHealthRecord, aggregate_records};
    let day = chrono::NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
    let start = helpyourself::reports::parse_time("2026-08-01").unwrap();
    let stored = |id: &str, from: i64, to: i64, kind: &str, payload: serde_json::Value| {
        let input = HealthRecordInput {
            record_id: id.into(),
            source_id: "watch".into(),
            record_type: kind.into(),
            start_at: from,
            end_at: to,
            version: 1,
            deleted: false,
            payload,
        };
        StoredHealthRecord {
            platform: "apple_health".into(),
            source_id: input.source_id.clone(),
            record_id: id.into(),
            record_type: kind.into(),
            start_at: from,
            end_at: to,
            version: 1,
            payload_json: serde_json::to_string(&input).unwrap(),
        }
    };
    let sleep = aggregate_records(
        vec![
            stored("a", start, start + 3600, "sleep", json!({"category":3})),
            stored(
                "b",
                start + 1800,
                start + 5400,
                "sleep",
                json!({"category":5}),
            ),
            stored(
                "awake",
                start + 5400,
                start + 7200,
                "sleep",
                json!({"category":2}),
            ),
        ],
        "sleep",
        day,
        day,
        chrono_tz::UTC,
    )
    .unwrap();
    assert_eq!(sleep["days"][0]["value"], 5400.0);
    let heart = aggregate_records(
        vec![
            stored(
                "a",
                start,
                start,
                "heart_rate",
                json!({"value":60,"unit":"count/min"}),
            ),
            stored(
                "b",
                start + 1,
                start + 1,
                "heart_rate",
                json!({"value":80,"unit":"count/min"}),
            ),
            stored(
                "bad",
                start + 2,
                start + 2,
                "heart_rate",
                json!({"value":999,"unit":"unknown"}),
            ),
        ],
        "heart_rate",
        day,
        day,
        chrono_tz::UTC,
    )
    .unwrap();
    assert_eq!(heart["days"][0]["value"], 70.0);
    let missing = aggregate_records(vec![], "heart_rate", day, day, chrono_tz::UTC).unwrap();
    assert!(missing["days"].as_array().unwrap().is_empty());
}
