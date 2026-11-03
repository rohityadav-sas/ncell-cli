use crate::{
    api::Client,
    error::{Error, Result},
    session::{Account, SessionStore},
};

pub fn normalize_number(input: &str) -> Result<String> {
    let clean: String = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect();
    let number = clean.strip_prefix("+977").unwrap_or(&clean);
    if number.len() != 10 || !number.starts_with('9') || !number.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(Error::InvalidInput(
            "Use a 10-digit Nepal mobile number, optionally prefixed with +977",
        ));
    }
    Ok(number.to_string())
}

pub struct AuthService<S> {
    api: Client,
    store: S,
}
impl<S: SessionStore> AuthService<S> {
    pub fn new(api: Client, store: S) -> Self {
        Self { api, store }
    }

    /// Retry once after an explicit expired-access-token rejection, or proactively
    /// refresh if the stored token is already known to be expired.
    pub fn authenticated<T>(
        &self,
        request: impl Fn(&Client, &Account, &str) -> Result<T>,
    ) -> Result<T> {
        if let Ok(Some(session)) = self.store.load()
            && let Some(account) = session.active()
            && account.is_expired()
            && account.refresh_token.is_some()
        {
            let _ = self.refresh();
        }
        let execute = || {
            let session = self.store.load()?.ok_or(Error::NotLoggedIn)?;
            let account = session.active().ok_or(Error::NotLoggedIn)?;
            request(&self.api, account, &account.device_id)
        };
        match execute() {
            Err(Error::Api {
                status: 401, code, ..
            }) if code == "LGN2001" => {
                self.refresh()?;
                execute()
            }
            result => result,
        }
    }

    pub fn request_otp(&self, number: &str) -> Result<()> {
        let number = normalize_number(number)?;
        let session = self.store.load()?.unwrap_or_default();
        let device_id = session
            .accounts
            .iter()
            .find(|a| a.msisdn == number)
            .map(|a| a.device_id.clone())
            .or_else(|| {
                session
                    .pending
                    .as_ref()
                    .filter(|p| p.msisdn == number)
                    .map(|p| p.device_id.clone())
            })
            .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string()[..16].into());

        self.store.update(|session| {
            session.pending = Some(crate::session::PendingLogin {
                msisdn: number.clone(),
                device_id: device_id.clone(),
            });
            Ok(())
        })?;
        self.api.generate_otp(&number, &device_id)
    }

    pub fn verify_otp(&self, number: &str, otp: &str) -> Result<()> {
        self.verify_account(number, otp, true)
    }

    pub fn verify_account(&self, number: &str, otp: &str, activate: bool) -> Result<()> {
        let number = normalize_number(number)?;
        if !(4..=8).contains(&otp.len()) || !otp.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::InvalidInput("OTP must contain 4 to 8 digits"));
        }
        let session = self.store.load()?.ok_or(Error::NoPendingLogin)?;
        let pending = session.pending.as_ref().ok_or(Error::NoPendingLogin)?;
        if pending.msisdn != number || pending.device_id.is_empty() {
            return Err(Error::NoPendingLogin);
        }
        let device_id = pending.device_id.clone();
        let tokens = self.api.validate_otp(&number, &device_id, otp)?;
        let now = chrono::Utc::now();
        let account = Account {
            msisdn: number.clone(),
            device_id,
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            authenticated_at: now,
            token_saved_at: Some(now),
            access_token_expires_at: token_expiry(tokens.expires_in),
        };
        self.store.update(|session| {
            session.pending = None;
            session.save_account(account, activate);
            Ok(())
        })
    }

    pub fn refresh(&self) -> Result<()> {
        let session = self.store.load()?.ok_or(Error::NotLoggedIn)?;
        let account = session.active().ok_or(Error::NotLoggedIn)?;
        let old_refresh = account
            .refresh_token
            .as_deref()
            .filter(|token| !token.is_empty())
            .ok_or(Error::InvalidInput("No refresh token saved. Log in again"))?
            .to_owned();
        let msisdn = account.msisdn.clone();
        let device_id = account.device_id.clone();

        let tokens = match self.api.refresh(&msisdn, &device_id, &old_refresh) {
            Err(Error::Api {
                status: 401, code, ..
            }) if code == "LGN2004" => {
                let already_rotated = self
                    .store
                    .load()?
                    .and_then(|s| s.accounts.into_iter().find(|a| a.msisdn == msisdn))
                    .is_some_and(|a| a.refresh_token.as_deref() != Some(old_refresh.as_str()));
                if already_rotated {
                    return Ok(());
                }
                return Err(Error::InvalidInput(
                    "Session expired. Run `ncell login` to sign in again.",
                ));
            }
            res => res?,
        };

        let now = chrono::Utc::now();
        let expiry = token_expiry(tokens.expires_in);

        self.store.update(|session| {
            if let Some(acc) = session.accounts.iter_mut().find(|a| a.msisdn == msisdn)
                && (acc.refresh_token.as_deref() == Some(&old_refresh)
                    || acc.refresh_token.is_none())
            {
                acc.token_saved_at = Some(now);
                acc.access_token_expires_at = expiry;
                acc.access_token = tokens.access_token;
                acc.refresh_token = tokens.refresh_token.or(acc.refresh_token.take());
            }
            Ok(())
        })
    }
}

fn token_expiry(expiry: Option<serde_json::Value>) -> Option<String> {
    let value = expiry?;
    if let Some(timestamp) = value.as_str().filter(|s| {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").is_ok()
            || chrono::DateTime::parse_from_rfc3339(s).is_ok()
    }) {
        // Preserve server-local timestamps without inventing a timezone.
        return Some(timestamp.to_owned());
    }
    let seconds = value.as_u64().or_else(|| value.as_str()?.parse().ok())?;
    chrono::Utc::now()
        .checked_add_signed(chrono::Duration::try_seconds(i64::try_from(seconds).ok()?)?)
        .map(|t| t.to_rfc3339())
}

#[cfg(test)]
#[path = "../tests/unit/auth.rs"]
mod metadata_tests;
