mod storage;
use serde::{Deserialize, Serialize};
pub use storage::{KeyringStore, SessionStore};

// Deliberately no Debug implementation: this object contains credentials.
#[derive(Default, Serialize, Deserialize)]
pub struct Session {
    pub device_id: String,
    pub pending_msisdn: Option<String>,
    pub account: Option<Account>,
    #[serde(default)]
    pub other_accounts: Vec<SavedAccount>,
}
#[derive(Serialize, Deserialize)]
pub struct Account {
    pub msisdn: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub token_saved_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub access_token_expires_at: Option<String>,
    pub authenticated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize, Deserialize)]
pub struct SavedAccount {
    pub device_id: String,
    pub account: Account,
}
impl Session {
    pub fn set_account(&mut self, account: Account) {
        self.other_accounts
            .retain(|saved| saved.account.msisdn != account.msisdn);
        if let Some(previous) = self.account.replace(account)
            && self
                .account
                .as_ref()
                .is_some_and(|a| a.msisdn != previous.msisdn)
        {
            self.other_accounts.push(SavedAccount {
                device_id: self.device_id.clone(),
                account: previous,
            });
        }
        self.pending_msisdn = None;
    }
    pub fn add_account(&mut self, account: Account) {
        if self
            .account
            .as_ref()
            .is_some_and(|a| a.msisdn == account.msisdn)
        {
            self.account = Some(account);
        } else {
            self.other_accounts
                .retain(|saved| saved.account.msisdn != account.msisdn);
            self.other_accounts.push(SavedAccount {
                device_id: self.device_id.clone(),
                account,
            });
        }
        self.pending_msisdn = None;
    }
    pub fn remove(&mut self, number: &str) -> crate::error::Result<()> {
        if self.account.as_ref().is_some_and(|a| a.msisdn == number) {
            self.logout();
        } else {
            let index = self
                .other_accounts
                .iter()
                .position(|saved| saved.account.msisdn == number)
                .ok_or(crate::error::Error::InvalidInput("Account is not saved"))?;
            self.other_accounts.remove(index);
            if self.pending_msisdn.as_deref() == Some(number) {
                self.pending_msisdn = None;
            }
        }
        Ok(())
    }
    pub fn switch(&mut self, number: &str) -> crate::error::Result<()> {
        if self.account.as_ref().is_some_and(|a| a.msisdn == number) {
            return Ok(());
        }
        let index = self
            .other_accounts
            .iter()
            .position(|saved| saved.account.msisdn == number)
            .ok_or(crate::error::Error::InvalidInput(
                "Account is not saved. Log in to it first",
            ))?;
        let saved = self.other_accounts.remove(index);
        self.set_account(saved.account);
        self.device_id = saved.device_id;
        Ok(())
    }
    pub fn logout(&mut self) {
        self.account = None;
        self.pending_msisdn = None;
        self.device_id.clear();
        if let Some(saved) = self.other_accounts.pop() {
            self.device_id = saved.device_id;
            self.account = Some(saved.account);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/session_mod.rs"]
mod tests;
