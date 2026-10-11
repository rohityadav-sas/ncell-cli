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
        let number = normalize_number(number)?;
        if !(4..=8).contains(&otp.len()) || !otp.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Error::InvalidInput("OTP must contain 4 to 8 digits"));
        }
        let mut session = self.store.load()?.ok_or(Error::NoPendingLogin)?;
        if session.pending_msisdn.as_deref() != Some(number) || session.device_id.is_empty() {
            return Err(Error::NoPendingLogin);
        }
        let tokens = self.api.validate_otp(number, &session.device_id, otp)?;
        session.account = Some(Account {
            msisdn: number.into(),
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            authenticated_at: chrono::Utc::now(),
        });
        session.pending_msisdn = None;
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
        account.access_token = tokens.access_token;
        account.refresh_token = tokens.refresh_token.or(account.refresh_token.take());
        self.store.save(&session)
    }
}
