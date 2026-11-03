use super::*;
use crate::session::Account;

fn scratch() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("ncell-cli-test-{}", uuid::Uuid::new_v4()))
}

fn account(number: &str) -> Account {
    serde_json::from_value(serde_json::json!({
        "msisdn": number, "device_id": "device", "access_token": "access", "refresh_token": "refresh",
        "authenticated_at": "2026-10-12T00:00:00Z"
    }))
    .unwrap()
}

#[test]
fn round_trip_preserves_active_and_inactive_accounts() {
    let dir = scratch();
    let store = FileStore::in_dir(&dir).unwrap();
    let mut session = Session::default();
    session.set_account(account("9811111111"));
    session.set_account(account("9822222222"));
    store.save(&session).unwrap();
    // Saving identical bytes twice is harmless.
    store.save(&session).unwrap();
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.active().unwrap().msisdn, "9822222222");
    assert_eq!(loaded.accounts.len(), 2);
    assert_eq!(loaded.accounts[0].msisdn, "9811111111");
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
    assert!(loaded.active().is_none());
    store.clear().unwrap();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn multiple_stores_can_coexist_on_the_same_directory() {
    let dir = scratch();
    let store1 = FileStore::in_dir(&dir).unwrap();
    let store2 = FileStore::in_dir(&dir).unwrap();
    assert!(store1.load().unwrap().is_none());
    assert!(store2.load().unwrap().is_none());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn active_lock_reports_busy() {
    let dir = scratch();
    let store = FileStore::in_dir(&dir).unwrap();
    // Simulate another process holding the lock file
    let lock_file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("session.lock"))
        .unwrap();
    lock_file.try_lock().unwrap();

    let session = Session::default();
    assert!(matches!(store.save(&session), Err(Error::SessionBusy)));
    drop(lock_file);
    assert!(store.save(&session).is_ok());
    std::fs::remove_dir_all(&dir).ok();
}
