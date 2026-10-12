use crate::{
    api::Client,
    error::{Error, Result},
    session::{Account, SessionStore},
};

pub fn normalize_number(input: &str) -> Result<&str> {
    let number = input.strip_prefix("+977").unwrap_or(input);
    if number.len() != 10 || !number.starts_with('9') || !number.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(Error::InvalidInput(
            "Use a 10-digit Nepal mobile number, optionally prefixed with +977",
        ));
    }
    Ok(number)
}

pub struct AuthService<S> {
    api: Client,
    store: S,
}
impl<S: SessionStore> AuthService<S> {
    pub fn new(api: Client, store: S) -> Self {
        Self { api, store }
    }

    /// Retry once after an explicit expired-access-token rejection.
    pub fn authenticated<T>(
        &self,
        request: impl Fn(&Client, &Account, &str) -> Result<T>,
    ) -> Result<T> {
        let execute = || {
            let session = self.store.load()?.ok_or(Error::NotLoggedIn)?;
            let account = session.account.as_ref().ok_or(Error::NotLoggedIn)?;
            request(&self.api, account, &session.device_id)
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
        let mut session = self.store.load()?.unwrap_or_default();
        if session.device_id.is_empty() {
            session.device_id = uuid::Uuid::new_v4().simple().to_string()[..16].into();
        }
        // Persist identity before sending SMS so cancellation never loses the device binding.
        session.pending_msisdn = Some(number.into());
        self.store.save(&session)?;
        self.api.generate_otp(number, &session.device_id)
    }

    pub fn verify_otp(&self, number: &str, otp: &str) -> Result<()> {
        self.verify_account(number, otp, true)
    }

    pub fn verify_account(&self, number: &str, otp: &str, activate: bool) -> Result<()> {
        let number = normalize_number(number)?;
        if !(4..=8).contains(&otp.len()) || !otp.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::InvalidInput("OTP must contain 4 to 8 digits"));
        }
        let mut session = self.store.load()?.ok_or(Error::NoPendingLogin)?;
        if session.pending_msisdn.as_deref() != Some(number) || session.device_id.is_empty() {
            return Err(Error::NoPendingLogin);
        }
        let tokens = self.api.validate_otp(number, &session.device_id, otp)?;
        let now = chrono::Utc::now();
        let account = Account {
            msisdn: number.into(),
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            authenticated_at: now,
            token_saved_at: Some(now),
            access_token_expires_at: token_expiry(tokens.expires_in),
        };
        if activate {
            session.set_account(account);
        } else {
            session.add_account(account);
        }
        self.store.save(&session)
    }

    pub fn refresh(&self) -> Result<()> {
        let mut session = self.store.load()?.ok_or(Error::NotLoggedIn)?;
        let account = session.account.as_mut().ok_or(Error::NotLoggedIn)?;
        let refresh_token = account
            .refresh_token
            .as_deref()
            .filter(|token| !token.is_empty())
            .ok_or(Error::InvalidInput("No refresh token saved. Log in again"))?;
        let tokens = self
            .api
            .refresh(&account.msisdn, &session.device_id, refresh_token)?;
        account.token_saved_at = Some(chrono::Utc::now());
        account.access_token_expires_at = token_expiry(tokens.expires_in);
        account.access_token = tokens.access_token;
        account.refresh_token = tokens.refresh_token.or(account.refresh_token.take());
        self.store.save(&session)
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
mod metadata_tests {
    use super::*;
    #[test]
    fn preserves_server_expiry_timestamp() {
        assert_eq!(
            token_expiry(Some(serde_json::json!("2026-10-12T21:49:20"))).as_deref(),
            Some("2026-10-12T21:49:20")
        );
        assert!(token_expiry(Some(serde_json::json!("unknown"))).is_none());
    }
}
