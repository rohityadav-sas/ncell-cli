use super::*;

fn account(number: &str) -> Account {
    Account {
        msisdn: number.into(),
        device_id: format!("device-{number}"),
        access_token: format!("token-{number}"),
        refresh_token: Some(format!("refresh-{number}")),
        authenticated_at: chrono::Utc::now(),
        token_saved_at: None,
        access_token_expires_at: None,
    }
}

#[test]
fn add_does_not_select_and_remove_preserves_unrelated_accounts() {
    let mut session = Session::default();
    session.add_account(account("9811111111"));
    assert!(session.active().is_none());
    session.switch("9811111111").unwrap();
    session.add_account(account("9822222222"));
    assert_eq!(session.active().unwrap().msisdn, "9811111111");
    session.add_account(account("9822222222"));
    assert_eq!(session.accounts.len(), 2);
    session.remove("9822222222").unwrap();
    assert_eq!(session.active().unwrap().msisdn, "9811111111");
    assert!(session.remove("9833333333").is_err());
    session.add_account(account("9822222222"));
    session.remove("9811111111").unwrap();
    assert_eq!(session.active().unwrap().msisdn, "9822222222");
    session.remove("9822222222").unwrap();
    assert!(session.active().is_none());
    assert!(session.accounts.is_empty());
}

#[test]
fn switching_preserves_identity_and_tokens_and_logout_removes_only_active() {
    let mut session = Session::default();
    let mut a1 = account("9811111111");
    a1.device_id = "first-device".into();
    let mut a2 = account("9822222222");
    a2.device_id = "second-device".into();

    session.set_account(a1);
    session.set_account(a2);
    session.switch("9811111111").unwrap();
    assert_eq!(session.active().unwrap().device_id, "first-device");
    assert_eq!(session.active().unwrap().access_token, "token-9811111111");

    session.switch("9822222222").unwrap();
    assert_eq!(session.active().unwrap().device_id, "second-device");
    assert!(session.switch("9833333333").is_err());

    session.logout();
    assert_eq!(session.active().unwrap().msisdn, "9811111111");
    assert_eq!(session.accounts.len(), 1);
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
    assert_eq!(session.accounts.len(), 2);
    assert_eq!(session.active().unwrap().msisdn, "9811111111");
}

#[test]
fn token_expiry_parsing_handles_npt_and_rfc3339() {
    let mut acc = account("9811111111");
    acc.access_token_expires_at = Some("2026-10-12T21:49:20".into());
    let exp = acc.expires_at().unwrap();
    // 21:49:20 NPT (UTC+5:45) is 16:04:20 UTC
    assert_eq!(exp.to_rfc3339(), "2026-10-12T16:04:20+00:00");

    acc.access_token_expires_at = Some("2026-10-12T16:04:20Z".into());
    assert_eq!(
        acc.expires_at().unwrap().to_rfc3339(),
        "2026-10-12T16:04:20+00:00"
    );
}
