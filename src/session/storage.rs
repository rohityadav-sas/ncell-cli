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
        Self::with_service("ncell-cli")
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
#[path = "../../tests/unit/session_storage.rs"]
mod tests;
