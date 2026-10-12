use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Script(String),
    #[error("{0}")]
    InvalidInput(&'static str),
    #[error("Network request failed; check your connection and try again")]
    Network,
    #[error("Connection to Ncell timed out; check your connection and try again")]
    Timeout,
    #[error("Could not establish a connection to Ncell (DNS, TCP or TLS failure)")]
    Connection,
    #[error("Ncell returned an invalid response")]
    InvalidResponse,
    #[error("Ncell rejected the request (HTTP {status}, code {code}){detail}", detail = api_error_detail(.code))]
    Api {
        status: u16,
        code: String,
        response: Box<serde_json::Value>,
    },
    #[error("{message} (HTTP {status})")]
    CheckIn {
        message: String,
        status: u16,
        response: Box<serde_json::Value>,
    },
    #[error("One or more tasks could not be claimed")]
    GameTasks { response: Box<serde_json::Value> },
    #[error("Math game lost")]
    GameLost,
    #[error("Credential storage is unavailable or locked; unlock your system keyring and retry")]
    Storage,
    #[error("Another command is using the saved accounts. Wait for it to finish and retry")]
    SessionBusy,
    #[error("Saved session is invalid; run `ncell logout` to reset it")]
    InvalidSession,
    #[error("No pending OTP for this number; run `ncell login <number>` first")]
    NoPendingLogin,
    #[error("No saved account. Run `ncell login <number>`")]
    NotLoggedIn,
    #[error("Could not read the OTP from the terminal")]
    Prompt,
}
impl Error {
    pub(crate) fn from_http(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self::Timeout
        } else if error.is_connect() {
            Self::Connection
        } else {
            Self::Network
        }
    }
}
pub type Result<T> = std::result::Result<T, Error>;

fn api_error_detail(code: &str) -> &'static str {
    include!("response_codes.rs")
}

#[cfg(test)]
#[path = "../tests/unit/error.rs"]
mod tests;
