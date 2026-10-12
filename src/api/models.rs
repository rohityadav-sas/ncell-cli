use crate::error::{Error, Result};
use chrono::{SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

pub(super) fn request_header(msisdn: &str, action: &str) -> Value {
    json!({
        "requestId": Uuid::new_v4().to_string(),
        "timestamp": Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        "channel": "sca",
        "deviceType": "android",
        "clientip": "N/A",
        "action": action,
        "msisdn": msisdn,
        "location": "N/A",
        "primaryMsisdn": msisdn,
        "languageCode": "1",
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<Value>,
}

pub(super) fn tokens(mut value: Value) -> Result<Tokens> {
    let body = value
        .as_object_mut()
        .and_then(|body| {
            body.remove("validateOTPResponse")
                .or_else(|| body.remove("userAuthResponse"))
        })
        .ok_or(Error::InvalidResponse)?;
    let tokens: Tokens = serde_json::from_value(body).map_err(|_| Error::InvalidResponse)?;
    if tokens.access_token.trim().is_empty() {
        return Err(Error::InvalidResponse);
    }
    Ok(tokens)
}

#[cfg(test)]
#[path = "../../tests/unit/api_models.rs"]
mod tests;
