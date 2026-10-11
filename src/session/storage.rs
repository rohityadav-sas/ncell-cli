use super::Session;
use crate::error::{Error, Result};

pub trait SessionStore {
    fn load(&self) -> Result<Option<Session>>;
    fn save(&self, session: &Session) -> Result<()>;
    fn clear(&self) -> Result<()>;
}
pub struct KeyringStore {
    entry: keyring::Entry,
}
impl KeyringStore {
    pub fn new() -> Result<Self> {
        Self::with_service("ncell-cli")
    }
    pub fn with_service(service: &str) -> Result<Self> {
        Ok(Self {
            entry: keyring::Entry::new(service, "session").map_err(|_| Error::Storage)?,
        })
    }
}
impl SessionStore for KeyringStore {
    fn load(&self) -> Result<Option<Session>> {
        match self.entry.get_password() {
            Ok(value) => serde_json::from_str(&value)
                .map(Some)
                .map_err(|_| Error::InvalidSession),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(Error::Storage),
        }
    }
    fn save(&self, session: &Session) -> Result<()> {
        let value = serde_json::to_string(session).map_err(|_| Error::InvalidSession)?;
        self.entry.set_password(&value).map_err(|_| Error::Storage)
    }
    fn clear(&self) -> Result<()> {
        match self.entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(Error::Storage),
        }
    }
}
