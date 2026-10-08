use crate::{add_user, archive, confirm, fixture};
use std::io::Read;

#[tokio::test]
async fn export_contains_raw_and_revisions_without_credentials_and_delete_cleans_them() {
    let (_directory, state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let report = archive(&state, "alice").await;
    confirm(&state, &user.user_id, &report).await;
    additional_sources(&state, &user.user_id).await;
    let association = state
        .database
        .behavior_associations(
            &user.user_id,
            helpyourself::wellness::associations::Request {
                end_date: "2026-01-01".into(),
                timezone: "UTC".into(),
                outcome: "sleep".into(),
                source: "apple_health:watch".into(),
                behaviors: vec!["synthetic".into()],
                covariates: vec![],
                lag_days: 0,
            },
        )
        .await
        .unwrap();
    let export = state.database.request_export(&user.user_id).await.unwrap();
    assert!(
        helpyourself::maintenance::export_next(&state)
            .await
            .unwrap()
    );
    state
        .database
        .export_ready(&user.user_id, &export)
        .await
        .unwrap();
    let path = state
        .config
        .data_dir
        .join("exports")
        .join(&user.user_id)
        .join(format!("{export}.zip"));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let mut bytes = Vec::new();
    zip.by_name(&format!("raw/photos/{}/{report}", user.user_id))
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, crate::png());
    assert!(zip.by_name("observation_revisions.jsonl").is_ok());
    assert!(zip.by_name("sessions.jsonl").is_err());
    drop(zip);
    let checker = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../archive/check_export.py"
        ))
        .arg(&path)
        .args(["--max-bytes", "10485760"])
        .output()
        .expect("Python 3 is required for the independent export contract test");
    assert!(
        checker.status.success(),
        "{}",
        String::from_utf8_lossy(&checker.stderr)
    );
    let checked: serde_json::Value = serde_json::from_slice(&checker.stdout).unwrap();
    assert_eq!(checked["status"], "passed");
    assert_eq!(checked["format_version"], 1);
    assert!(checked["member_sha256_checked"].as_u64().unwrap() >= 19);
    assert_eq!(checked["document_sha256_checked"], 1);
    assert_eq!(checked["health_payloads_checked"], 3);
    let restore_root = tempfile::tempdir().unwrap();
    let restored_path = restore_root.path().join("restored");
    let restored = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../archive/restore_export.py"
        ))
        .arg(&path)
        .arg("--destination")
        .arg(&restored_path)
        .args(["--username", "restored", "--max-bytes", "10485760"])
        .output()
        .unwrap();
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    let restored_db = helpyourself::database::Database::open(&restored_path)
        .await
        .unwrap();
    assert_eq!(
        restored_db.data_revision(&user.user_id).await.unwrap(),
        state.database.data_revision(&user.user_id).await.unwrap()
    );
    assert!(
        restored_db
            .report_summary(&user.user_id, &report)
            .await
            .is_ok()
    );
    assert!(restored_db.credentials("restored").await.unwrap().is_none());
    restored_db
        .enable_user(
            "restored",
            &helpyourself::authentication::hash_password("restored-password".into())
                .await
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(restored_db.credentials("restored").await.unwrap().is_some());
    assert_eq!(
        std::fs::read(restored_path.join(format!("raw/photos/{}/{report}", user.user_id))).unwrap(),
        crate::png()
    );
    assert_eq!(
        restored_db
            .association_result(&user.user_id, association["result_id"].as_str().unwrap())
            .await
            .unwrap(),
        association
    );
    restored_db.close().await;
    state
        .database
        .delete_report(&user.user_id, &report, 2)
        .await
        .unwrap();
    assert!(
        state
            .database
            .export_ready(&user.user_id, &export)
            .await
            .is_err()
    );
    helpyourself::maintenance::cleanup(&state).await.unwrap();
    assert!(!path.exists());
    assert!(
        !state
            .config
            .data_dir
            .join("raw/photos")
            .join(&user.user_id)
            .join(&report)
            .exists()
    );
    assert!(state.database.report(&user.user_id, &report).await.is_err());
}

#[tokio::test]
async fn account_deletion_revokes_sessions_and_cancels_queued_work() {
    let (_directory, state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let report = archive(&state, "alice").await;
    let token = crate::token(&state, "alice").await;
    additional_sources(&state, &user.user_id).await;
    state.database.request_export(&user.user_id).await.unwrap();
    state.database.delete_account(&user.user_id).await.unwrap();
    assert!(
        state
            .database
            .session_user(
                &helpyourself::authentication::digest(token.as_bytes()),
                helpyourself::authentication::now().unwrap()
            )
            .await
            .is_err()
    );
    assert!(
        state
            .database
            .job_by_file(&user.user_id, &report)
            .await
            .is_err()
    );
    assert!(
        !helpyourself::maintenance::export_next(&state)
            .await
            .unwrap()
    );
    helpyourself::maintenance::cleanup(&state).await.unwrap();
    for source in helpyourself::raw::SOURCES {
        assert!(
            !state
                .config
                .data_dir
                .join("raw")
                .join(source)
                .join(&user.user_id)
                .exists()
        );
    }

    assert!(
        !state
            .config
            .data_dir
            .join("raw/photos")
            .join(&user.user_id)
            .exists()
    );
}

#[test]
fn csv_formula_cells_are_inert() {
    assert_eq!(helpyourself::maintenance::csv_cell("=1+1"), "\"'=1+1\"");
}

#[tokio::test]
async fn delayed_report_cleanup_does_not_remove_a_new_export() {
    let (_directory, state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let report = archive(&state, "alice").await;
    state
        .database
        .delete_report(&user.user_id, &report, 1)
        .await
        .unwrap();
    let export = state.database.request_export(&user.user_id).await.unwrap();
    helpyourself::maintenance::export_next(&state)
        .await
        .unwrap();
    helpyourself::maintenance::cleanup(&state).await.unwrap();
    state
        .database
        .export_ready(&user.user_id, &export)
        .await
        .unwrap();
    assert!(
        state
            .config
            .data_dir
            .join("exports")
            .join(&user.user_id)
            .join(format!("{export}.zip"))
            .exists()
    );
}

#[tokio::test]
async fn deleted_upload_cannot_be_revived_by_a_lost_response_retry() {
    let (_directory, state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let token = crate::token(&state, "alice").await;
    let app = helpyourself::app::create_application(state.clone());
    let upload_id = uuid::Uuid::new_v4().to_string();
    let (_, response) = crate::upload(&app, &token, &upload_id, &crate::png()).await;
    let report = response["file"]["file_id"].as_str().unwrap();
    state
        .database
        .delete_report(&user.user_id, report, 1)
        .await
        .unwrap();
    let (status, _) = crate::upload(&app, &token, &upload_id, &crate::png()).await;
    assert_eq!(status, axum::http::StatusCode::CONFLICT);
    assert!(
        state
            .database
            .reports(
                &user.user_id,
                &helpyourself::models::ListRequest {
                    limit: 10,
                    after_id: None
                }
            )
            .await
            .unwrap()
            .is_empty()
    );
}

async fn additional_sources(state: &helpyourself::app::AppState, user: &str) {
    state.database.save_wellness_entry(user, serde_json::from_value(serde_json::json!({
        "record_id":uuid::Uuid::new_v4(),"version":1,"batch_id":uuid::Uuid::new_v4(),"at":100,"timezone":"UTC",
        "entry":{"kind":"body","content":{"weight_kg":70,"body_fat_percent":null,"waist_cm":null,"note":"synthetic"}}
    })).unwrap()).await.unwrap();
    state.database.import_gpx(user, helpyourself::wellness::import::XmlImportRequest {
        filename:"synthetic.gpx".into(), xml:r#"<gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1"><trk><trkseg><trkpt lat="0" lon="0"><time>2026-01-01T00:00:00Z</time></trkpt><trkpt lat="0" lon="0.001"><time>2026-01-01T00:01:00Z</time></trkpt></trkseg></trk></gpx>"#.into()
    }).await.unwrap();
    state
        .database
        .import_tcx(
            user,
            helpyourself::wellness::import::XmlImportRequest {
                filename: "indoor.tcx".into(),
                xml: super::wellness::tcx_fixture(),
            },
        )
        .await
        .unwrap();
}
