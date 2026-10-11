mod storage;
use serde::{Deserialize, Serialize};
pub use storage::{KeyringStore, SessionStore};

// Deliberately no Debug implementation: this object contains credentials.
#[derive(Default, Serialize, Deserialize)]
pub struct Session {
    pub device_id: String,
    pub pending_msisdn: Option<String>,
    pub account: Option<Account>,
}
#[derive(Serialize, Deserialize)]
pub struct Account {
    pub msisdn: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub authenticated_at: chrono::DateTime<chrono::Utc>,
}
