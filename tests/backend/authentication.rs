use crate::{add_user, fixture};
use helpyourself::{
    authentication::{digest, hash_password, login, normalize_username, now},
    models::LoginRequest,
};

#[tokio::test]
async fn passwords_sessions_and_revoke_are_real() {
    let (_directory, state) = fixture().await;
    add_user(&state, "alice").await;
    assert!(hash_password("short".into()).await.is_err());
    assert_eq!(normalize_username(" ALICE ").unwrap(), "alice");
    assert!(normalize_username("../alice").is_err());
    assert!(normalize_username("a").is_err());
    for username in ["alice", "missing"] {
        assert!(
            login(
                &state,
                LoginRequest {
                    username: username.into(),
                    password: "wrong-password".into()
                }
            )
            .await
            .is_err()
        );
    }
    let session = login(
        &state,
        LoginRequest {
            username: "Alice".into(),
            password: "correct-password-123".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(session.token.len(), 64);
    let token_hash = digest(session.token.as_bytes());
    assert_eq!(
        state
            .database
            .session_user(&token_hash, now().unwrap())
            .await
            .unwrap()
            .username,
        "alice"
    );
    assert!(
        state
            .database
            .session_user(&token_hash, session.expires_at)
            .await
            .is_err()
    );
    state.database.revoke_session(&token_hash).await.unwrap();
    assert!(
        state
            .database
            .session_user(&token_hash, now().unwrap())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn password_reset_invalidates_inflight_credentials_and_old_sessions() {
    let (_directory, state) = fixture().await;
    add_user(&state, "alice").await;
    let old = state.database.credentials("alice").await.unwrap().unwrap();
    let session = crate::token(&state, "alice").await;
    let new_hash = hash_password("a-new-password-456".into()).await.unwrap();
    state
        .database
        .change_credentials("alice", Some(&new_hash))
        .await
        .unwrap();
    assert!(
        state
            .database
            .session_user(&digest(session.as_bytes()), now().unwrap())
            .await
            .is_err()
    );
    assert!(
        state
            .database
            .create_session("stale", now().unwrap() + 100, &old, now().unwrap())
            .await
            .is_err()
    );
    assert!(
        login(
            &state,
            LoginRequest {
                username: "alice".into(),
                password: "correct-password-123".into()
            }
        )
        .await
        .is_err()
    );
    assert!(
        login(
            &state,
            LoginRequest {
                username: "alice".into(),
                password: "a-new-password-456".into()
            }
        )
        .await
        .is_ok()
    );
    state
        .database
        .change_credentials("alice", None)
        .await
        .unwrap();
    assert!(
        login(
            &state,
            LoginRequest {
                username: "alice".into(),
                password: "a-new-password-456".into()
            }
        )
        .await
        .is_err()
    );
}
