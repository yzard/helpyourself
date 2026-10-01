use helpyourself::{database::Database, error::AppError};
mod queries;
mod schema;

#[tokio::test]
async fn schema_survives_reopen_and_rejects_duplicate_users() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path()).await.unwrap();
    let user = database.create_user("alice", "hash", 1).await.unwrap();
    assert!(matches!(
        database.create_user("alice", "other", 1).await,
        Err(AppError::Conflict(_))
    ));
    database.close().await;
    let reopened = Database::open(directory.path()).await.unwrap();
    assert_eq!(
        reopened
            .credentials("alice")
            .await
            .unwrap()
            .unwrap()
            .user_id,
        user.user_id
    );
    assert!(reopened.change_credentials("missing", None).await.is_err());
    reopened.close().await;
}

#[tokio::test]
async fn login_rate_limit_is_atomic_and_persistent() {
    let directory = tempfile::tempdir().unwrap();
    let database = Database::open(directory.path()).await.unwrap();
    let (first, second) = tokio::join!(
        database.reserve_login("hash", 100, 60, 1),
        database.reserve_login("hash", 100, 60, 1)
    );
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    database.close().await;
    let database = Database::open(directory.path()).await.unwrap();
    assert!(matches!(
        database.reserve_login("hash", 101, 60, 1).await,
        Err(AppError::RateLimited)
    ));
    assert!(database.reserve_login("hash", 160, 60, 1).await.is_ok());
}
