use crate::{add_user, fixture, png, token, upload};
use helpyourself::{app::create_application, error::AppError};

#[tokio::test]
async fn jobs_are_isolated_and_unavailable_extraction_stays_blocked() {
    let (_directory, state) = fixture().await;
    let alice = add_user(&state, "alice").await;
    let bob = add_user(&state, "bob").await;
    let token = token(&state, "alice").await;
    let (_, response) = upload(
        &create_application(state.clone()),
        &token,
        &uuid::Uuid::new_v4().to_string(),
        &png(),
    )
    .await;
    let job_id = response["job"]["job_id"].as_str().unwrap();
    assert_eq!(response["job"]["status"], "blocked");
    assert!(matches!(
        state.database.job(&bob.user_id, job_id).await,
        Err(AppError::NotFound)
    ));
    assert!(
        state
            .database
            .retry_job(&alice.user_id, job_id)
            .await
            .is_err()
    );
    assert!(
        state
            .database
            .claim_job(100, 10, 3)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn expired_lease_cannot_overwrite_new_worker_and_attempts_are_bounded() {
    let (_directory, state) = fixture().await;
    let user = add_user(&state, "alice").await;
    let token = token(&state, "alice").await;
    let (_, response) = upload(
        &create_application(state.clone()),
        &token,
        &uuid::Uuid::new_v4().to_string(),
        &png(),
    )
    .await;
    let job_id = response["job"]["job_id"].as_str().unwrap();
    state
        .database
        .queue_blocked_job(&user.user_id, job_id)
        .await
        .unwrap();
    let first = state.database.claim_job(100, 10, 2).await.unwrap().unwrap();
    assert!(
        state
            .database
            .claim_job(101, 10, 2)
            .await
            .unwrap()
            .is_none()
    );
    state.database.renew_job(&first, 105, 10).await.unwrap();
    assert!(
        state
            .database
            .claim_job(111, 10, 2)
            .await
            .unwrap()
            .is_none()
    );
    let second = state.database.claim_job(115, 10, 2).await.unwrap().unwrap();
    assert_ne!(first.lease_token, second.lease_token);
    assert!(state.database.finish_job(&first, 116, None).await.is_err());
    assert!(state.database.renew_job(&first, 116, 10).await.is_err());
    assert!(
        state
            .database
            .claim_job(125, 10, 2)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        state
            .database
            .job(&user.user_id, job_id)
            .await
            .unwrap()
            .status,
        "failed"
    );
    state
        .database
        .retry_job(&user.user_id, job_id)
        .await
        .unwrap();
    let third = state.database.claim_job(126, 10, 2).await.unwrap().unwrap();
    state.database.finish_job(&third, 127, None).await.unwrap();
    assert_eq!(
        state
            .database
            .job(&user.user_id, job_id)
            .await
            .unwrap()
            .status,
        "succeeded"
    );
}
