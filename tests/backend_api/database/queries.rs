use crate::{add_user, fixture, token};
use helpyourself::{authentication::digest, models::ArchivedFile};
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};

#[tokio::test]
async fn database_enforces_resource_owner_and_only_persists_session_digest() {
    let (_directory, state) = fixture().await;
    let alice = add_user(&state, "alice").await;
    let bob = add_user(&state, "bob").await;
    let session_token = token(&state, "alice").await;
    let file = ArchivedFile {
        file_id: uuid::Uuid::new_v4().to_string(),
        upload_id: uuid::Uuid::new_v4().to_string(),
        original_name: "synthetic.pdf".into(),
        processing_path: None,
        processing_sha256: None,
        relative_path: format!("raw/documents/{}/fixture", alice.user_id),
        content_type: "application/pdf".into(),
        sha256: digest(b"synthetic"),
        byte_count: 9,
        page_count: 1,
        created_at: 100,
    };
    state
        .database
        .archive_file(&alice.user_id, &file)
        .await
        .unwrap();
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(state.config.server.data_dir.join("database.sqlite"))
            .foreign_keys(true),
    )
    .await
    .unwrap();
    let stored: String = sqlx::query_scalar("SELECT token_hash FROM sessions")
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(stored, digest(session_token.as_bytes()));
    assert_ne!(stored, session_token);
    let failed = sqlx::query(
        "INSERT INTO jobs (job_id, user_id, file_id, kind, status, created_at)
         VALUES (?, ?, ?, 'document_extract', 'queued', 1)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&bob.user_id)
    .bind(&file.file_id)
    .execute(&mut connection)
    .await
    .unwrap_err();
    assert!(matches!(failed, sqlx::Error::Database(error) if error.is_foreign_key_violation()));
}

#[tokio::test]
async fn unknown_newer_schema_is_not_silently_accepted() {
    let directory = tempfile::tempdir().unwrap();
    let database = helpyourself::database::Database::open(directory.path())
        .await
        .unwrap();
    database.close().await;
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new().filename(directory.path().join("database.sqlite")),
    )
    .await
    .unwrap();
    sqlx::query("PRAGMA user_version = 999")
        .execute(&mut connection)
        .await
        .unwrap();
    connection.close().await.unwrap();
    assert!(
        helpyourself::database::Database::open(directory.path())
            .await
            .is_err()
    );
}
