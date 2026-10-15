use crate::error::{Error, Result};
use serde_json::Value;
use std::time::Duration;

pub const BASE_URL: &str = "https://sca.ncell.com.np/adl/et/telco/selfcare/ncell/api";

#[derive(Clone)]
pub struct Client {
    http: reqwest::blocking::Client,
    base_url: String,
    pub(super) categories_cache: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, Value)>>,
    >,
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
            categories_cache: Default::default(),
        })
    }

    /// Raw POST: transport and HTTP-level failures become errors, but the
    /// Ncell envelope (`responseCode`) is left for the caller to judge.
    /// Non-JSON error pages (e.g. a 502 HTML page) report their HTTP status
    /// instead of a misleading "invalid response".
    pub(crate) fn post_raw(
        &self,
        path: &str,
        body: &Value,
        token: Option<&str>,
    ) -> Result<(u16, Value)> {
        let mut request = self
            .http
            .post(format!("{}{path}", self.base_url))
            .json(body);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let response = request.send().map_err(Error::from_http)?;
        let status = response.status().as_u16();
        let value: Value = response.json().map_err(|_| {
            if (200..300).contains(&status) {
                Error::InvalidResponse
            } else {
                Error::Api {
                    status,
                    code: format!("HTTP{status}"),
                    response: Box::new(Value::Null),
                }
            }
        })?;
        Ok((status, value))
    }

    pub(crate) fn post(&self, path: &str, body: &Value, token: Option<&str>) -> Result<Value> {
        let (status, value) = self.post_raw(path, body, token)?;
        let header = &value["responseHeader"];
        let code = &header["responseCode"];
        if !(200..300).contains(&status) || (code != "200" && code != 200) {
            return Err(api_error(status, value));
        }
        Ok(value)
    }
}

/// Build the shared rejection error: HTTP status plus the short envelope code.
/// Only short alphanumeric codes are exposed, never arbitrary server text or tokens.
pub(super) fn api_error(status: u16, value: Value) -> Error {
    let header = &value["responseHeader"];
    let code = &header["responseCode"];
    let detail = header["responseDescDisplay"].as_str().filter(|s| {
        (6..=12).contains(&s.len()) && {
            let (prefix, digits) = s.as_bytes().split_at(s.len() - 4);
            prefix.iter().all(u8::is_ascii_uppercase) && digits.iter().all(u8::is_ascii_digit)
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
    Error::Api {
        status,
        code,
        response: Box::new(value),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/api_client.rs"]
mod tests;
