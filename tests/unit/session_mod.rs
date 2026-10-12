use super::*;
fn account(number: &str) -> Account {
    Account {
        msisdn: number.into(),
        access_token: format!("token-{number}"),
        refresh_token: Some(format!("refresh-{number}")),
        authenticated_at: chrono::Utc::now(),
        token_saved_at: None,
        access_token_expires_at: None,
    }
}
#[test]
fn add_does_not_select_and_remove_preserves_unrelated_accounts() {
    let mut session = Session {
        device_id: "device".into(),
        ..Session::default()
    };
    session.add_account(account("9811111111"));
    assert!(session.account.is_none());
    session.switch("9811111111").unwrap();
    session.add_account(account("9822222222"));
    assert_eq!(session.account.as_ref().unwrap().msisdn, "9811111111");
    session.add_account(account("9822222222"));
    assert_eq!(session.other_accounts.len(), 1);
    session.remove("9822222222").unwrap();
    assert_eq!(session.account.as_ref().unwrap().msisdn, "9811111111");
    assert!(session.remove("9833333333").is_err());
    session.add_account(account("9822222222"));
    session.remove("9811111111").unwrap();
    assert_eq!(session.account.as_ref().unwrap().msisdn, "9822222222");
    session.remove("9822222222").unwrap();
    assert!(session.account.is_none());
    assert!(session.other_accounts.is_empty());
}

#[test]
fn switching_preserves_identity_and_tokens_and_logout_removes_only_active() {
    let mut session = Session {
        device_id: "first-device".into(),
        ..Session::default()
    };
    session.set_account(account("9811111111"));
    session.set_account(account("9822222222"));
    session.device_id = "second-device".into();
    session.switch("9811111111").unwrap();
    assert_eq!(session.device_id, "first-device");
    assert_eq!(
        session.account.as_ref().unwrap().access_token,
        "token-9811111111"
    );
    session.switch("9822222222").unwrap();
    assert_eq!(session.device_id, "second-device");
    assert!(session.switch("9833333333").is_err());
    session.logout();
    assert_eq!(session.account.as_ref().unwrap().msisdn, "9811111111");
    assert!(session.other_accounts.is_empty());
}
#[test]
fn existing_session_and_relogin_do_not_duplicate_accounts() {
    let mut session: Session = serde_json::from_value(
        serde_json::json!({"device_id":"old", "pending_msisdn":null,"account":null}),
    )
    .unwrap();
    session.set_account(account("9811111111"));
    session.set_account(account("9822222222"));
    session.set_account(account("9811111111"));
    assert_eq!(session.other_accounts.len(), 1);
    assert_eq!(session.other_accounts[0].account.msisdn, "9822222222");
}
