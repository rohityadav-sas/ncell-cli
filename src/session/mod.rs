mod storage;
use serde::{Deserialize, Serialize};
pub use storage::{FileStore, SessionStore};

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingLogin {
    pub msisdn: String,
    pub device_id: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub msisdn: String,
    #[serde(default)]
    pub device_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub token_saved_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub access_token_expires_at: Option<String>,
    pub authenticated_at: chrono::DateTime<chrono::Utc>,
}

impl Account {
    pub fn expires_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        let s = self.access_token_expires_at.as_deref()?;
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return Some(dt.with_timezone(&chrono::Utc));
        }
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
            let npt = chrono::FixedOffset::east_opt(5 * 3600 + 45 * 60)?;
            use chrono::TimeZone;
            return npt
                .from_local_datetime(&naive)
                .single()
                .map(|dt| dt.with_timezone(&chrono::Utc));
        }
        None
    }

    pub fn is_expired(&self) -> bool {
        self.expires_at()
            .is_some_and(|exp| exp <= chrono::Utc::now())
    }
}

// Deliberately no Debug implementation: this object contains credentials.
#[derive(Clone, Default, Serialize)]
pub struct Session {
    pub active: Option<String>,
    pub accounts: Vec<Account>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<PendingLogin>,
}

impl<'de> Deserialize<'de> for Session {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawSession {
            active: Option<String>,
            accounts: Option<Vec<Account>>,
            pending: Option<PendingLogin>,

            // Legacy fields:
            device_id: Option<String>,
            pending_msisdn: Option<String>,
            account: Option<Account>,
            #[serde(default)]
            other_accounts: Vec<LegacySavedAccount>,
        }

        #[derive(Deserialize)]
        struct LegacySavedAccount {
            device_id: String,
            account: Account,
        }

        let raw = RawSession::deserialize(deserializer)?;

        if let Some(accounts) = raw.accounts {
            return Ok(Session {
                active: raw.active,
                accounts,
                pending: raw.pending,
            });
        }

        let legacy_dev = raw.device_id.unwrap_or_default();
        let mut accounts = Vec::new();
        let mut active = None;

        if let Some(mut acc) = raw.account {
            if acc.device_id.is_empty() {
                acc.device_id = legacy_dev.clone();
            }
            active = Some(acc.msisdn.clone());
            accounts.push(acc);
        }

        for mut saved in raw.other_accounts {
            if saved.account.device_id.is_empty() {
                saved.account.device_id = saved.device_id;
            }
            accounts.push(saved.account);
        }

        let pending = raw.pending.or_else(|| {
            raw.pending_msisdn.map(|msisdn| PendingLogin {
                msisdn,
                device_id: legacy_dev,
            })
        });

        Ok(Session {
            active,
            accounts,
            pending,
        })
    }
}

impl Session {
    pub fn active(&self) -> Option<&Account> {
        self.accounts
            .iter()
            .find(|a| Some(&a.msisdn) == self.active.as_ref())
    }

    pub fn active_mut(&mut self) -> Option<&mut Account> {
        let active = self.active.as_ref()?;
        self.accounts.iter_mut().find(|a| &a.msisdn == active)
    }

    pub fn account(&self) -> Option<&Account> {
        self.active()
    }

    pub fn account_mut(&mut self) -> Option<&mut Account> {
        self.active_mut()
    }

    pub fn save_account(&mut self, account: Account, activate: bool) {
        if activate {
            self.active = Some(account.msisdn.clone());
        }
        self.accounts.retain(|x| x.msisdn != account.msisdn);
        self.accounts.push(account);
        self.pending = None;
    }

    pub fn set_account(&mut self, account: Account) {
        self.save_account(account, true);
    }

    pub fn add_account(&mut self, account: Account) {
        self.save_account(account, false);
    }

    pub fn switch(&mut self, number: &str) -> crate::error::Result<()> {
        if !self.accounts.iter().any(|a| a.msisdn == number) {
            return Err(crate::error::Error::InvalidInput(
                "Account is not saved. Log in to it first",
            ));
        }
        self.active = Some(number.to_string());
        Ok(())
    }

    pub fn remove(&mut self, number: &str) -> crate::error::Result<()> {
        let prev_len = self.accounts.len();
        self.accounts.retain(|a| a.msisdn != number);
        if self.accounts.len() == prev_len {
            return Err(crate::error::Error::InvalidInput("Account is not saved"));
        }
        if self.pending.as_ref().is_some_and(|p| p.msisdn == number) {
            self.pending = None;
        }
        if self.active.as_deref() == Some(number) {
            self.active = self.accounts.last().map(|a| a.msisdn.clone());
        }
        Ok(())
    }

    pub fn logout(&mut self) {
        if let Some(active) = self.active.clone() {
            let _ = self.remove(&active);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/session_mod.rs"]
mod tests;
