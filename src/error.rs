use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    InvalidInput(&'static str),
    #[error("Network request failed; check your connection and try again")]
    Network,
    #[error(
        "Connection to Ncell timed out. Run `ncell doctor` to check HTTPS without requesting an OTP"
    )]
    Timeout,
    #[error(
        "Could not establish a connection to Ncell (DNS, TCP or TLS failure). Run `ncell doctor` for a connection check"
    )]
    Connection,
    #[error("Ncell returned an invalid response")]
    InvalidResponse,
    #[error("Ncell rejected the request (HTTP {status}, code {code}){detail}", detail = api_error_detail(.code))]
    Api {
        status: u16,
        code: String,
        response: Box<serde_json::Value>,
    },
    #[error("Credential storage is unavailable or locked; unlock your system keyring and retry")]
    Storage,
    #[error("Saved session is invalid; run `ncell logout` to reset it")]
    InvalidSession,
    #[error("No pending OTP for this number; run `ncell login <number>` first")]
    NoPendingLogin,
    #[error("No saved account. Run `ncell login <number>` or import the current app session")]
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
    match code {
        "LGN2000" => ": Mobile number is missing",
        "LGN2001" => ": Access token has expired",
        "LGN2002" => ": Authentication token was not supplied",
        "LGN2003" => ": Authentication error; exact meaning is unconfirmed",
        "LGN2004" => ": Refresh token is invalid or the device ID does not match",
        "LGN2005" => ": Number change is not allowed",
        "LGN2007" => ": Invalid mobile number",
        "LGN2009" => ": Request validation failed; a required field may be missing",
        "LGN3000" => ": Unable to process the request; try again later",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(api_error_detail("NEW9999"), "");
        assert!(api_error_detail("LGN2003").contains("unconfirmed"));
    }
}
