use super::*;
use crate::session::Account;

fn scratch() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("ncell-cli-test-{}", uuid::Uuid::new_v4()))
}

fn account(number: &str) -> Account {
    serde_json::from_value(serde_json::json!({
        "msisdn": number, "access_token": "access", "refresh_token": "refresh",
        "authenticated_at": "2026-10-12T00:00:00Z"
    }))
    .unwrap()
}

#[test]
fn round_trip_preserves_active_and_inactive_accounts() {
    let dir = scratch();
    let store = FileStore::in_dir(&dir).unwrap();
    let mut session = Session {
        device_id: "device".into(),
        ..Session::default()
    };
    session.set_account(account("9811111111"));
    session.set_account(account("9822222222"));
    store.save(&session).unwrap();
    // Saving identical bytes twice is harmless.
    store.save(&session).unwrap();
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.account.unwrap().msisdn, "9822222222");
    assert_eq!(loaded.other_accounts.len(), 1);
    assert_eq!(loaded.other_accounts[0].account.msisdn, "9811111111");
    store.clear().unwrap();
    assert!(store.load().unwrap().is_none());
    assert!(!dir.join("session.json").exists());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn damaged_session_file_fails_load_but_allows_overwrite() {
    let dir = scratch();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("session.json"), "damaged").unwrap();
    let store = FileStore::in_dir(&dir).unwrap();
    assert!(matches!(store.load(), Err(Error::InvalidSession)));
    store.save(&Session::default()).unwrap();
    let loaded = store.load().unwrap().unwrap();
    assert!(loaded.account.is_none());
    store.clear().unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn second_store_on_the_same_directory_reports_busy() {
    let dir = scratch();
    let store = FileStore::in_dir(&dir).unwrap();
    assert!(matches!(FileStore::in_dir(&dir), Err(Error::SessionBusy)));
    drop(store);
    FileStore::in_dir(&dir).unwrap();
    std::fs::remove_dir_all(&dir).ok();
}
