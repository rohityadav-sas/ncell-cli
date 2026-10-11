use super::{
    Client,
    models::{Tokens, request_header, tokens},
};
use crate::error::Result;
use serde_json::json;

impl Client {
    pub fn generate_otp(&self, msisdn: &str, device_id: &str) -> Result<()> {
        self.post("/v1.0/user/otp/generate", &json!({
            "generateOTPRequest": { "msisdn": msisdn, "deviceId": device_id, "action": "LOGIN" },
            "requestHeader": request_header(msisdn, "LOGIN"),
        }), None).map(|_| ())
    }

    pub fn validate_otp(&self, msisdn: &str, device_id: &str, otp: &str) -> Result<Tokens> {
        tokens(self.post(
            "/v1.0/user/otp/validate",
            &json!({
                "validateOTPRequest": {
                    "msisdn": msisdn, "deviceId": device_id,
                    "otpDetail": { "action": "LOGIN", "otp": otp },
                },
                "requestHeader": request_header(msisdn, "LOGIN"),
            }),
            None,
        )?)
    }

    pub fn refresh(&self, msisdn: &str, device_id: &str, refresh_token: &str) -> Result<Tokens> {
        let mut header = request_header(msisdn, "LOGOUT_ACTION");
        header["deviceId"] = device_id.into();
        tokens(self.post(
            "/v1.0/user/refresh/token",
            &json!({
                "refreshTokenRequest": { "refreshToken": refresh_token },
                "requestHeader": header,
            }),
            None,
        )?)
    }
}
