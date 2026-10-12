use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
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
mod tests {
    use super::*;

    #[test]
    fn app_codes_have_meanings_without_exposing_server_data() {
        for (code, expected) in [
            ("BIL2007", "insufficient"),
            ("OTP2006", "Expired"),
            ("SMS2000", "failed"),
            ("CL3001", "Loan"),
            ("BSAM5001", "Try again"),
        ] {
            assert!(api_error_detail(code).contains(expected));
        }
        assert!(api_error_detail("DVS2008").contains("unconfirmed"));
        assert!(api_error_detail("BIL1000").contains("Subscription success"));
    }

    #[test]
    fn authentication_errors_have_safe_labels_and_preserve_codes() {
        for code in [
            "LGN2000", "LGN2001", "LGN2002", "LGN2003", "LGN2004", "LGN2005", "LGN2007", "LGN2009",
            "LGN3000",
        ] {
            let error = Error::Api {
                status: 401,
                code: code.into(),
                response: Box::new(serde_json::json!({"private": "secret"})),
            };
            let message = error.to_string();
            assert!(message.contains(code));
            assert!(!api_error_detail(code).is_empty());
            assert!(!message.contains("secret"));
        }
        assert!(api_error_detail("NEW9999").contains("unavailable"));
        assert!(api_error_detail("LGN2003").contains("unconfirmed"));
    }
}
