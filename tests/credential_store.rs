use ncell_cli::session::{Account, FileStore, Session, SessionStore};

fn scratch() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("ncell-cli-test-{}", uuid::Uuid::new_v4()))
}

fn account(number: &str) -> Account {
    serde_json::from_value(serde_json::json!({"msisdn":number,"access_token":format!("access-{number}"),"refresh_token":format!("refresh-{number}"),"authenticated_at":"2026-10-12T00:00:00Z"})).unwrap()
}

#[test]
fn file_store_round_trip() {
    let dir = scratch();
    let store = FileStore::in_dir(&dir).unwrap();
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
    assert!(!dir.join("session.json").exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn multiple_accounts_round_trip() {
    let dir = scratch();
    let store = FileStore::in_dir(&dir).unwrap();
    let mut session = Session {
        device_id: "device".into(),
        ..Session::default()
    };
    for number in ["9811111111", "9822222222"] {
        session.set_account(account(number));
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
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn legacy_shaped_session_loads_and_three_account_logout_cleans_session_file() {
    let dir = scratch();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("session.json"),
        r#"{"device_id":"legacy-device","pending_msisdn":null,"account":{"msisdn":"9811111111","access_token":"legacy-access","refresh_token":"legacy-refresh","authenticated_at":"2026-10-12T00:00:00Z"}}"#,
    )
    .unwrap();
    let store = FileStore::in_dir(&dir).unwrap();
    let mut session = store.load().unwrap().unwrap();
    for number in ["9822222222", "9833333333"] {
        session.set_account(account(number));
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
    store.clear().unwrap();
    assert!(!dir.join("session.json").exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn add_remove_without_active_account_round_trips() {
    let dir = scratch();
    let store = FileStore::in_dir(&dir).unwrap();
    let mut session = Session {
        device_id: "device".into(),
        ..Session::default()
    };
    session.add_account(account("9811111111"));
    store.save(&session).unwrap();
    let mut loaded = store.load().unwrap().unwrap();
    assert!(loaded.account.is_none());
    assert_eq!(loaded.other_accounts.len(), 1);
    loaded.remove("9811111111").unwrap();
    store.save(&loaded).unwrap();
    assert!(store.load().unwrap().unwrap().other_accounts.is_empty());
    store.clear().unwrap();
    assert!(!dir.join("session.json").exists());
    std::fs::remove_dir_all(&dir).ok();
}
