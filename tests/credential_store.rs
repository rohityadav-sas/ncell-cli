use ncell_cli::session::{KeyringStore, Session, SessionStore};

#[test]
#[ignore = "Requires an unlocked native credential store; uses an isolated disposable entry"]
fn native_store_round_trip() {
    let service = format!("ncell-cli-test-{}", uuid::Uuid::new_v4());
    let store = KeyringStore::with_service(&service).unwrap();
    assert!(store.load().unwrap().is_none());
    store
        .save(&Session {
            device_id: "test-device".into(),
            ..Session::default()
        })
        .unwrap();
    let loaded = store.load();
    store.clear().unwrap();
    assert_eq!(loaded.unwrap().unwrap().device_id, "test-device");
    assert!(store.load().unwrap().is_none());
}

#[test]
#[ignore = "Uses isolated native credential entries"]
fn multiple_accounts_round_trip() {
    let store =
        KeyringStore::with_service(&format!("ncell-cli-test-{}", uuid::Uuid::new_v4())).unwrap();
    let mut session = Session {
        device_id: "device".into(),
        ..Session::default()
    };
    for number in ["9811111111", "9822222222"] {
        session.set_account(ncell_cli::session::Account {
            msisdn: number.into(),
            access_token: "x".repeat(400),
            refresh_token: Some("y".repeat(400)),
            authenticated_at: chrono::Utc::now(),
            token_saved_at: None,
            access_token_expires_at: None,
        });
    }
    store.save(&session).unwrap();
    let mut loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.other_accounts.len(), 1);
    loaded.switch("9811111111").unwrap();
    store.save(&loaded).unwrap();
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.account.unwrap().msisdn, "9811111111");
    assert_eq!(loaded.other_accounts[0].account.msisdn, "9822222222");
    store.clear().unwrap();
    assert!(store.load().unwrap().is_none());
}

#[test]
#[ignore = "Uses isolated native credential entries"]
fn old_session_migrates_and_three_account_logout_cleans_credentials() {
    let service = format!("ncell-cli-test-{}", uuid::Uuid::new_v4());
    let entry = keyring::Entry::new(&service, "session").unwrap();
    entry.set_password(r#"{"device_id":"legacy-device","pending_msisdn":null,"account":{"msisdn":"9811111111","access_token":"legacy-access","refresh_token":"legacy-refresh","authenticated_at":"2026-10-12T00:00:00Z"}}"#).unwrap();
    let store = KeyringStore::with_service(&service).unwrap();
    let mut session = store.load().unwrap().unwrap();
    for number in ["9822222222", "9833333333"] {
        let mut account: ncell_cli::session::Account = serde_json::from_value(serde_json::json!({"msisdn":number,"access_token":format!("access-{number}"),"refresh_token":format!("refresh-{number}"),"authenticated_at":"2026-10-12T00:00:00Z"})).unwrap();
        account.token_saved_at = Some(chrono::Utc::now());
        session.set_account(account);
    }
    store.save(&session).unwrap();
    for number in ["9811111111", "9822222222", "9833333333", "9811111111"] {
        let mut loaded = store.load().unwrap().unwrap();
        loaded.switch(number).unwrap();
        store.save(&loaded).unwrap();
        assert_eq!(
            store.load().unwrap().unwrap().account.unwrap().msisdn,
            number
        );
    }
    for remaining in [2, 1, 0] {
        let mut loaded = store.load().unwrap().unwrap();
        loaded.logout();
        store.save(&loaded).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(
            loaded.other_accounts.len() + usize::from(loaded.account.is_some()),
            remaining
        );
    }
    for number in ["9811111111", "9822222222", "9833333333"] {
        assert!(matches!(
            keyring::Entry::new(&service, &format!("account-{number}"))
                .unwrap()
                .get_password(),
            Err(keyring::Error::NoEntry)
        ));
    }
    store.clear().unwrap();
}

#[test]
#[ignore = "Uses isolated native credential entries"]
fn add_remove_without_active_account_round_trips() {
    let service = format!("ncell-cli-test-{}", uuid::Uuid::new_v4());
    let store = KeyringStore::with_service(&service).unwrap();
    let mut session = Session {
        device_id: "device".into(),
        ..Session::default()
    };
    let account=serde_json::from_value(serde_json::json!({"msisdn":"9811111111","access_token":"access","refresh_token":"refresh","authenticated_at":"2026-10-12T00:00:00Z"})).unwrap();
    session.add_account(account);
    store.save(&session).unwrap();
    let mut loaded = store.load().unwrap().unwrap();
    assert!(loaded.account.is_none());
    assert_eq!(loaded.other_accounts.len(), 1);
    loaded.remove("9811111111").unwrap();
    store.save(&loaded).unwrap();
    assert!(store.load().unwrap().unwrap().other_accounts.is_empty());
    assert!(matches!(
        keyring::Entry::new(&service, "account-9811111111")
            .unwrap()
            .get_password(),
        Err(keyring::Error::NoEntry)
    ));
    store.clear().unwrap();
}
