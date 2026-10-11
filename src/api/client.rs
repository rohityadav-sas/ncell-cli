use crate::error::{Error, Result};
use serde_json::Value;
use std::time::Duration;

pub const BASE_URL: &str = "https://sca.ncell.com.np/adl/et/telco/selfcare/ncell/api";

pub struct Client {
    http: reqwest::blocking::Client,
    base_url: String,
}

impl Client {
    pub fn new() -> Result<Self> {
        Self::with_base_url(BASE_URL)
    }

    /// Custom origins are intended for local integration tests. The CLI always uses BASE_URL.
    pub fn with_base_url(base_url: &str) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(45))
            .timeout(Duration::from_secs(90))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(Error::from_http)?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').into(),
        })
    }

    pub(crate) fn post(&self, path: &str, body: &Value, token: Option<&str>) -> Result<Value> {
        let mut request = self
            .http
            .post(format!("{}{path}", self.base_url))
            .json(body);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(Error::from_http)?;
        let status = response.status();
        let value: Value = response.json().map_err(|_| Error::InvalidResponse)?;
        let header = &value["responseHeader"];
        let code = &header["responseCode"];
        let subscription_success = path == "/v1.0/billingmgt/product/subscribe"
            && header["responseDescDisplay"] == "BIL1000";
        if !status.is_success() || (!subscription_success && code != "200" && code != 200) {
            // Only expose short alphanumeric codes, never arbitrary server text or tokens.
            let detail = header["responseDescDisplay"].as_str().filter(|s| {
                (6..=12).contains(&s.len()) && {
                    let (prefix, digits) = s.as_bytes().split_at(s.len() - 4);
                    prefix.iter().all(u8::is_ascii_uppercase)
                        && digits.iter().all(u8::is_ascii_digit)
                }
            });
            let code = detail
                .map(str::to_owned)
                .or_else(|| match code {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .filter(|s| s.len() <= 32 && s.chars().all(|c| c.is_ascii_alphanumeric()))
                .unwrap_or_else(|| "unknown".into());
            return Err(Error::Api {
                status: status.as_u16(),
                code,
                response: Box::new(value),
            });
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_codes_with_short_and_long_prefixes_keep_their_meanings() {
        let mut server = mockito::Server::new();
        let client = Client::with_base_url(&server.url()).unwrap();
        for (code, meaning) in [("CL3001", "Loan request failed"), ("BSAM5001", "Try again")] {
            let request = server
                .mock("POST", "/test")
                .with_status(400)
                .with_body(
                    serde_json::json!({"responseHeader": {
                        "responseCode": "08", "responseDescDisplay": code
                    }})
                    .to_string(),
                )
                .create();
            let error = client
                .post("/test", &serde_json::json!({}), None)
                .unwrap_err();
            assert!(error.to_string().contains(code));
            assert!(error.to_string().contains(meaning));
            request.assert();
        }
    }
}
