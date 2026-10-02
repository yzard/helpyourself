use crate::{add_user, fixture};
use helpyourself::health::{ConnectionRequest, HealthRecordInput, SyncRequest};
use serde_json::json;
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};

pub fn record(id: &str, payload: serde_json::Value) -> HealthRecordInput {
    HealthRecordInput {
        record_id: id.into(),
        source_id: "synthetic".into(),
        record_type: "synthetic_raw".into(),
        start_at: 1,
        end_at: 2,
        version: 1,
        deleted: false,
        payload,
    }
}
pub fn batch(connection: &str, records: Vec<HealthRecordInput>) -> SyncRequest {
    SyncRequest {
        connection_id: connection.into(),
        batch_id: uuid::Uuid::new_v4().to_string(),
        record_type: "synthetic_raw".into(),
        coverage_status: "observed".into(),
        records,
    }
}

#[tokio::test]
async fn streaming_digest_matches_existing_canonical_bytes_and_invalid_late_record_is_atomic() {
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
    let input = batch(
        &connection.connection_id,
        vec![record(
            "valid",
            json!({"unicode":"µ 中文", "nested":{"unknown":[1,2,"\\n"]}}),
        )],
    );
    let encoded = serde_json::to_vec(&input).unwrap();
    let batch_id = input.batch_id.clone();
    state
        .database
        .sync_health(&user.user_id, input)
        .await
        .unwrap();
    let mut connection_sql = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(state.config.server.data_dir.join("database.sqlite")),
    )
    .await
    .unwrap();
    let stored: String = sqlx::query_scalar("SELECT digest FROM sync_batches WHERE batch_id = ?")
        .bind(batch_id)
        .fetch_one(&mut connection_sql)
        .await
        .unwrap();
    assert_eq!(stored, helpyourself::authentication::digest(&encoded));
    let mut invalid = record("bad", json!({}));
    invalid.version = 0;
    let result = state
        .database
        .sync_health(
            &user.user_id,
            batch(
                &connection.connection_id,
                vec![record("must-not-exist", json!({"value":2})), invalid],
            ),
        )
        .await;
    assert!(matches!(
        result,
        Err(helpyourself::error::AppError::Invalid(_))
    ));
    let rows = state
        .database
        .health_records(&user.user_id, 100, 0)
        .await
        .unwrap();
    assert_eq!(rows["records"].as_array().unwrap().len(), 1);
    assert_eq!(
        rows["records"][0]["record"]["payload"],
        json!({"unicode":"µ 中文", "nested":{"unknown":[1,2,"\\n"]}})
    );
    let files = state
        .database
        .health_raw_files(
            &user.user_id,
            &helpyourself::models::ListRequest {
                after_id: None,
                limit: 100,
            },
        )
        .await
        .unwrap();
    assert_eq!(files["files"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn payload_and_batch_limits_fail_before_any_record_or_coverage_is_written() {
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
    for sizes in [
        vec![32 * 1024 * 1024],
        vec![21 * 1024 * 1024, 21 * 1024 * 1024],
    ] {
        let records = sizes
            .into_iter()
            .enumerate()
            .map(|(i, size)| record(&i.to_string(), json!({"blob":"x".repeat(size)})))
            .collect();
        let result = state
            .database
            .sync_health(&user.user_id, batch(&connection.connection_id, records))
            .await;
        assert!(matches!(
            result,
            Err(helpyourself::error::AppError::TooLarge)
        ));
    }
    assert!(
        state
            .database
            .health_records(&user.user_id, 100, 0)
            .await
            .unwrap()["records"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let coverage = state.database.health_coverage(&user.user_id).await.unwrap();
    assert!(coverage["coverage"][0]["status"].is_null());
}
