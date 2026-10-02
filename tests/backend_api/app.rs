use helpyourself::{app::AppState, config::Config};

#[tokio::test]
async fn second_server_cannot_share_data_and_interrupted_uploads_are_cleaned() {
    let (directory, state) = crate::fixture().await;
    let configuration = Config::load(&directory.path().join("config.toml")).unwrap();
    assert!(AppState::open(configuration.clone()).await.is_err());
    let pending = configuration.server.data_dir.join("tmp/interrupted");
    std::fs::write(&pending, b"private fragment").unwrap();
    let orphan_directory = configuration
        .server
        .data_dir
        .join("raw/photos")
        .join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir_all(&orphan_directory).unwrap();
    let orphan = orphan_directory.join(uuid::Uuid::new_v4().to_string());
    std::fs::write(&orphan, b"uncommitted archive").unwrap();
    state.database.close().await;
    drop(state);
    let reopened = AppState::open(configuration).await.unwrap();
    assert!(!pending.exists());
    assert!(!orphan.exists());
    reopened.database.close().await;
}

#[tokio::test]
async fn graceful_shutdown_cleans_failed_uploads_before_closing_storage() {
    let (_directory, state) = crate::fixture().await;
    crate::add_user(&state, "alice").await;
    let token = crate::token(&state, "alice").await;
    let app = helpyourself::app::create_application(state.clone());
    let (status, _) = crate::upload(
        &app,
        &token,
        &uuid::Uuid::new_v4().to_string(),
        b"invalid document",
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    let root = state.config.server.data_dir.join("tmp");
    assert!(
        tokio::fs::read_dir(&root)
            .await
            .unwrap()
            .next_entry()
            .await
            .unwrap()
            .is_some()
    );
    state.shutdown().await.unwrap();
    assert!(
        tokio::fs::read_dir(&root)
            .await
            .unwrap()
            .next_entry()
            .await
            .unwrap()
            .is_none()
    );
}
