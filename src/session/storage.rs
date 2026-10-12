use super::Session;
use crate::error::{Error, Result};

pub trait SessionStore {
    fn load(&self) -> Result<Option<Session>>;
    fn save(&self, session: &Session) -> Result<()>;
    fn clear(&self) -> Result<()>;
}
pub struct KeyringStore {
    entry: keyring::Entry,
    service: String,
    _lock: std::fs::File,
}
impl KeyringStore {
    pub fn new() -> Result<Self> {
        #[cfg(not(target_os = "windows"))]
        return Self::with_service("ncell-cli");
        #[cfg(target_os = "windows")]
        {
            Self::with_migration("ncell-cli", "ncell-cli-v2")
        }
    }
    #[cfg(target_os = "windows")]
    fn with_migration(service: &str, old_service: &str) -> Result<Self> {
        let store = Self::with_service(service)?;
        if store
            .read_index()?
            .as_ref()
            .and_then(|index| index["storage_version"].as_u64())
            != Some(3)
        {
            let legacy = Self::with_service(old_service)?;
            // Prefer the v2 accounts once, even when an older destination exists.
            // Mark empty sessions too so removed accounts cannot reappear.
            let session = legacy.load()?.or(store.load()?).unwrap_or_default();
            store.save(&session)?;
        }
        Ok(store)
    }
    fn account_entry(&self, number: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, &format!("account-{number}")).map_err(|_| Error::Storage)
    }
    pub fn with_service(service: &str) -> Result<Self> {
        if !service
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(Error::InvalidInput("Invalid credential service name"));
        }
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(std::env::temp_dir().join(format!("{service}.lock")))
            .map_err(|_| Error::Storage)?;
        lock.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => Error::SessionBusy,
            std::fs::TryLockError::Error(_) => Error::Storage,
        })?;
        Ok(Self {
            _lock: lock,
            entry: keyring::Entry::new(service, "session").map_err(|_| Error::Storage)?,
            service: service.to_owned(),
        })
    }
    fn read_index(&self) -> Result<Option<serde_json::Value>> {
        match self.entry.get_password() {
            Ok(value) => serde_json::from_str(&value)
                .map(Some)
                .map_err(|_| Error::InvalidSession),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(Error::Storage),
        }
    }
}
impl SessionStore for KeyringStore {
    fn load(&self) -> Result<Option<Session>> {
        match self.entry.get_password() {
            Ok(value) => {
                let mut value: serde_json::Value =
                    serde_json::from_str(&value).map_err(|_| Error::InvalidSession)?;
                let mut accounts = Vec::new();
                for number in value["saved_accounts"].as_array().into_iter().flatten() {
                    let number = number.as_str().ok_or(Error::InvalidSession)?;
                    let saved = self
                        .account_entry(number)?
                        .get_password()
                        .map_err(|_| Error::Storage)?;
                    accounts.push(
                        serde_json::from_str::<serde_json::Value>(&saved)
                            .map_err(|_| Error::InvalidSession)?,
                    );
                }
                if !accounts.is_empty() {
                    value["other_accounts"] = accounts.into();
                }
                serde_json::from_value(value)
                    .map(Some)
                    .map_err(|_| Error::InvalidSession)
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(Error::Storage),
        }
    }
    fn save(&self, session: &Session) -> Result<()> {
        // Keep each inactive account in its own native credential entry.
        let previous = self.read_index()?;
        let mut value = serde_json::to_value(session).map_err(|_| Error::InvalidSession)?;
        let mut numbers = Vec::new();
        for saved in &session.other_accounts {
            let entry = self.account_entry(&saved.account.msisdn)?;
            let encoded = serde_json::to_string(saved).map_err(|_| Error::InvalidSession)?;
            let unchanged = match entry.get_password() {
                Ok(current) => current == encoded,
                Err(keyring::Error::NoEntry) => false,
                Err(_) => return Err(Error::Storage),
            };
            if !unchanged {
                entry.set_password(&encoded).map_err(|_| Error::Storage)?;
                if entry.get_password().map_err(|_| Error::Storage)? != encoded {
                    return Err(Error::Storage);
                }
            }
            numbers.push(saved.account.msisdn.clone());
        }
        value.as_object_mut().unwrap().remove("other_accounts");
        value["saved_accounts"] = serde_json::json!(numbers);
        value["storage_version"] = serde_json::json!(3);
        let encoded = value.to_string();
        if previous.as_ref() != Some(&value) {
            self.entry
                .set_password(&encoded)
                .map_err(|_| Error::Storage)?;
            if self.entry.get_password().map_err(|_| Error::Storage)? != encoded {
                return Err(Error::Storage);
            }
        }
        for old in previous
            .as_ref()
            .and_then(|v| v["saved_accounts"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
        {
            if !numbers.iter().any(|number| number == old) {
                self.account_entry(old)?
                    .delete_credential()
                    .map_err(|_| Error::Storage)?;
            }
        }
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        if let Some(session) = self.load()? {
            for saved in session.other_accounts {
                self.account_entry(&saved.account.msisdn)?
                    .delete_credential()
                    .map_err(|_| Error::Storage)?;
            }
        }
        match self.entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(Error::Storage),
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
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
}
