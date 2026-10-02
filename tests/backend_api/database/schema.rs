use helpyourself::database::Database;
use sqlx::{Connection, SqliteConnection, sqlite::SqliteConnectOptions};

#[tokio::test]
async fn current_schema_has_raw_indexes_and_enforces_extraction_owner() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path()).await.unwrap();
    database.close().await;
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(directory.path().join("database.sqlite"))
            .foreign_keys(true),
    )
    .await
    .unwrap();
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(version, 7);
    let error = sqlx::query("INSERT INTO extraction_outputs (user_id, report_id, run_id, page_number, stage, response_body, status_code, model, adapter, created_at) VALUES ('missing', 'missing', 'run', 1, 'ocr', '{}', 200, 'model', 'adapter', 1)")
        .execute(&mut connection).await.unwrap_err();
    assert!(matches!(error, sqlx::Error::Database(error) if error.is_foreign_key_violation()));
}
