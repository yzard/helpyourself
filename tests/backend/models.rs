use helpyourself::models::{ListRequest, LoginRequest};

#[test]
fn clients_cannot_override_identity_and_list_limits_are_bounded() {
    assert!(
        serde_json::from_str::<LoginRequest>(
            r#"{"username":"alice","password":"x","user_id":"bob"}"#
        )
        .is_err()
    );
    for limit in [0, 101, -1] {
        assert!(
            ListRequest {
                after_id: None,
                limit
            }
            .validate()
            .is_err()
        );
    }
    assert!(
        ListRequest {
            after_id: None,
            limit: 100
        }
        .validate()
        .is_ok()
    );
}
