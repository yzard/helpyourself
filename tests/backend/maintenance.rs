use crate::{add_user, archive, confirm, fixture};
use std::io::Read;

#[tokio::test]
async fn export_contains_raw_and_revisions_without_credentials_and_delete_cleans_them() {
    let (_directory, state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let report = archive(&state, "alice").await;
    confirm(&state, &user.user_id, &report).await;
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
        .server
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
            .server
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
    assert!(
        !state
            .config
            .server
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
            .server
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
