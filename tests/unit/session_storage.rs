use super::*;

#[test]
#[ignore = "Uses isolated native credential entries"]
fn unchanged_saves_preserve_accounts_and_missing_entries_are_repaired() {
    let service = format!("ncell-cli-test-{}", uuid::Uuid::new_v4());
    let store = KeyringStore::with_service(&service).unwrap();
    let mut session = Session {
        device_id: "device".into(),
        ..Session::default()
    };
    for number in ["9811111111", "9822222222"] {
        session.set_account(
            serde_json::from_value(serde_json::json!({
                "msisdn":number, "access_token":"access", "refresh_token":"refresh",
                "authenticated_at":"2026-10-12T00:00:00Z"
            }))
            .unwrap(),
        );
    }
    store.save(&session).unwrap();
    store.save(&session).unwrap();
    let inactive = store.account_entry("9811111111").unwrap();
    inactive.delete_credential().unwrap();
    store.save(&session).unwrap();
    let loaded = store.load().unwrap().unwrap();
    assert_eq!(loaded.account.unwrap().msisdn, "9822222222");
    assert_eq!(loaded.other_accounts[0].account.msisdn, "9811111111");
    store.clear().unwrap();
}

#[test]
#[ignore = "Uses isolated native credential entries"]
fn migration_is_once_only_and_commands_cannot_overlap() {
    let old = format!("ncell-cli-test-{}", uuid::Uuid::new_v4());
    let new = format!("ncell-cli-test-{}", uuid::Uuid::new_v4());
    let legacy = KeyringStore::with_service(&old).unwrap();
    legacy
        .save(&Session {
            device_id: "original".into(),
            ..Session::default()
        })
        .unwrap();
    drop(legacy);
    // An existing pre-migration destination must not hide the newer accounts.
    let destination = keyring::Entry::new(&new, "session").unwrap();
    destination
        .set_password(
            &serde_json::to_string(&Session {
                device_id: "older-destination".into(),
                ..Session::default()
            })
            .unwrap(),
        )
        .unwrap();
    let store = KeyringStore::with_migration(&new, &old).unwrap();
    assert_eq!(store.load().unwrap().unwrap().device_id, "original");
    assert!(matches!(
        KeyringStore::with_service(&new),
        Err(Error::SessionBusy)
    ));
    store.save(&Session::default()).unwrap();
    drop(store);
    let legacy = KeyringStore::with_service(&old).unwrap();
    legacy
        .save(&Session {
            device_id: "stale".into(),
            ..Session::default()
        })
        .unwrap();
    legacy.clear().unwrap();
    drop(legacy);
    let store = KeyringStore::with_migration(&new, &old).unwrap();
    assert!(store.load().unwrap().unwrap().device_id.is_empty());
    store.clear().unwrap();
}

#[test]
#[ignore = "Uses isolated native credential entries"]
fn damaged_existing_index_is_not_silently_overwritten() {
    let service = format!("ncell-cli-test-{}", uuid::Uuid::new_v4());
    let entry = keyring::Entry::new(&service, "session").unwrap();
    entry.set_password("damaged").unwrap();
    let store = KeyringStore::with_service(&service).unwrap();
    assert!(matches!(
        store.save(&Session::default()),
        Err(Error::InvalidSession)
    ));
    assert_eq!(entry.get_password().unwrap(), "damaged");
    entry.delete_credential().unwrap();
}
