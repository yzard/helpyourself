use crate::{add_user, fixture};
use axum::http::StatusCode;
use helpyourself::{
    health::{ConnectionRequest, HealthRecordInput, SyncRequest},
    models::ListRequest,
};
use serde_json::{Value, json};

#[tokio::test]
async fn raw_health_is_indexed_replayable_exported_isolated_and_deleted() {
    let (_directory, state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let other = add_user(&state, "bob").await;
    let token = crate::token(&state, "alice").await;
    let other_token = crate::token(&state, "bob").await;
    let app = helpyourself::app::create_application(state.clone());
    for platform in ["apple_health", "health_connect"] {
        let connection = state
            .database
            .create_connection(
                &user.user_id,
                ConnectionRequest {
                    platform: platform.into(),
                    installation_id: uuid::Uuid::new_v4().to_string(),
                },
            )
            .await
            .unwrap();
        let record = HealthRecordInput {
            record_id: "raw-sample".into(),
            source_id: "original-device".into(),
            record_type: "unsupported_numeric_type".into(),
            start_at: 100,
            end_at: 101,
            version: 1,
            deleted: false,
            payload: json!({"raw_archive":{"format":"nskeyedarchiver-secure-v1","data":"opaque".repeat(100_000)},"nested":{"flag":true,"values":[1,"original",null]},"series":[{"timestamp":100.25,"quantity":"4 count"}]}),
        };
        let request = SyncRequest {
            connection_id: connection.connection_id,
            batch_id: uuid::Uuid::new_v4().to_string(),
            record_type: record.record_type.clone(),
            coverage_status: "observed".into(),
            records: vec![record.clone()],
        };
        let encoded = serde_json::to_value(&request).unwrap();
        assert_eq!(
            crate::request(&app, "/api/v1/health/sync", Some(&token), encoded.clone())
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            crate::request(&app, "/api/v1/health/sync", Some(&token), encoded.clone())
                .await
                .1["replayed"],
            true
        );
        let files = state
            .database
            .health_raw_files(
                &user.user_id,
                &ListRequest {
                    after_id: None,
                    limit: 100,
                },
            )
            .await
            .unwrap();
        let file = files["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|file| file["platform"] == platform)
            .unwrap();
        let raw_id = file["raw_id"].as_str().unwrap();
        let path = file["relative_path"].as_str().unwrap();
        assert!(path.starts_with(if platform == "apple_health" {
            "raw/apple_health/"
        } else {
            "raw/google_health/"
        }));
        let bytes = std::fs::read(state.config.data_dir.join(path)).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            serde_json::to_value(&record).unwrap()
        );
        assert!(
            state
                .database
                .health_raw_path(&other.user_id, raw_id)
                .await
                .is_err()
        );
        assert_eq!(
            crate::request(
                &app,
                "/api/v1/health/raw/list",
                Some(&other_token),
                json!({"limit":100})
            )
            .await
            .1["files"],
            json!([])
        );
        let export = state.database.request_export(&user.user_id).await.unwrap();
        helpyourself::maintenance::export_next(&state)
            .await
            .unwrap();
        let export_path = state
            .config
            .data_dir
            .join(format!("exports/{}/{export}.zip", user.user_id));
        let mut zip = zip::ZipArchive::new(std::fs::File::open(export_path).unwrap()).unwrap();
        assert!(zip.by_name(path).is_ok());
        drop(zip);
        let mut deletion: SyncRequest = serde_json::from_value(encoded).unwrap();
        deletion.batch_id = uuid::Uuid::new_v4().to_string();
        deletion.records[0].deleted = true;
        deletion.records[0].source_id = "*".into();
        assert_eq!(
            crate::request(
                &app,
                "/api/v1/health/sync",
                Some(&token),
                serde_json::to_value(deletion).unwrap()
            )
            .await
            .0,
            StatusCode::OK
        );
        assert!(
            state
                .database
                .health_raw_path(&user.user_id, raw_id)
                .await
                .is_err()
        );
        helpyourself::maintenance::cleanup(&state).await.unwrap();
        assert!(!state.config.data_dir.join(path).exists());
    }
}

#[test]
fn raw_cleanup_paths_cannot_escape_a_source_and_owner() {
    let user = uuid::Uuid::new_v4();
    assert!(helpyourself::raw::valid_relative_path(&format!(
        "raw/photos/{user}/image"
    )));
    for path in [
        "raw/../../etc/passwd",
        "raw/photos/not-a-user/image",
        "raw/unknown/user/file",
        "raw/photos/../file",
        "raw\\photos\\file",
    ] {
        assert!(!helpyourself::raw::valid_relative_path(path));
    }
}
