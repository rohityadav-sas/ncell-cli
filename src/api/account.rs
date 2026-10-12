use super::{Client, models::request_header};
use crate::error::{Error, Result};
use serde_json::{Value, json};

impl Client {
    pub fn check_in_url(&self, msisdn: &str, token: &str) -> Result<String> {
        let response = self.post(
            "/v1.0/utilitymgt/third-party-encryption",
            &json!({
                "thirdPartyClientRequest": {"clientId": "47c0b8f2-1766-c12a-5c88-519e40af83b4"},
                "requestHeader": {"msisdn": msisdn},
            }),
            Some(token),
        )?;
        let encrypted = required_text(&response["encryptionResponse"]["msisdn"])?;
        // The API already URL-encodes this value; encoding it again breaks authentication.
        if !encrypted
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"%-._~".contains(&b))
        {
            return Err(Error::InvalidResponse);
        }
        Ok(format!(
            "https://sajeeb.com.np/ncellapp/checkin/index.php?MSISDN={encrypted}"
        ))
    }

    pub fn send_free_sms(
        &self,
        msisdn: &str,
        token: &str,
        destination: &str,
        content: &str,
    ) -> Result<Value> {
        let destination = crate::auth::normalize_number(destination)?;
        if content.trim().is_empty() {
            return Err(Error::InvalidInput("SMS text cannot be empty"));
        }
        let response = self.post(
            "/v1.0/smsmgt/free/sms/send",
            &json!({
                "sendSMSFreeRequest": {
                    "destination": destination, "content": content, "isConfirm": 0,
                },
                "requestHeader": request_header(msisdn, "MY_SERVICES"),
            }),
            Some(token),
        )?;
        if response["sendFreeSMSResponse"]["status"] != "success" {
            return Err(Error::InvalidResponse);
        }
        Ok(response)
    }

    pub fn subscribe(
        &self,
        msisdn: &str,
        token: &str,
        device_id: &str,
        plan: &Value,
    ) -> Result<Value> {
        let code = required_text(&plan["techInfo"]["subscriptionCode"])?;
        let name = required_text(&plan["ncellName"])?;
        let price = plan["productOfferingPrice"]["price"]
            .as_str()
            .ok_or(Error::InvalidResponse)?;
        if device_id.is_empty() {
            return Err(Error::InvalidSession);
        }
        let mut header = request_header(msisdn, "SUBSCRIBE");
        header["deviceId"] = device_id.into();
        header["appVersion"] = "8.2.1.0".into();
        let response = self.post(
            "/v1.0/billingmgt/product/subscribe",
            &json!({
                "productSubscriptionSummaryRequest": {
                    "deviceId": device_id, "msisdn": msisdn, "subscriptionCode": code,
                    "productName": name, "productPrice": price, "ncellProductName": name,
                    "medium": "APP", "linkId": "00000000000000000", "termCode": 0,
                    "useCoupon": false, "deeplinkClickId": "",
                },
                "requestHeader": header,
            }),
            Some(token),
        )?;
        if response["responseHeader"]["responseDescDisplay"] == "BIL1000"
            || response["productSubscriptionSummaryResponse"]["status"]
                .as_str()
                .is_some_and(|s| s.eq_ignore_ascii_case("success"))
        {
            Ok(response)
        } else {
            Err(Error::InvalidResponse)
        }
    }

    pub fn categories(&self, msisdn: &str, token: &str) -> Result<Value> {
        if let Some((created, cached)) = self.categories_cache.lock().unwrap().get(msisdn)
            && created.elapsed() < std::time::Duration::from_secs(600)
        {
            return Ok(cached.clone());
        }
        let header = request_header(msisdn, "N/A");
        let response = self.post(
            "/v1.0/product-catalog/tags",
            &json!({"requestHeader": header}),
            Some(token),
        )?;
        if !response["tags"].is_array() {
            return Err(Error::InvalidResponse);
        }
        self.categories_cache.lock().unwrap().insert(
            msisdn.to_owned(),
            (std::time::Instant::now(), response.clone()),
        );
        Ok(response)
    }

    pub fn plans(&self, msisdn: &str, token: &str, category: u64) -> Result<Value> {
        self.search_plans(msisdn, token, category, "")
    }

    pub fn search_plans(
        &self,
        msisdn: &str,
        token: &str,
        category: u64,
        keyword: &str,
    ) -> Result<Value> {
        let catalog = if category == 66 {
            "data"
        } else {
            let tags = self.categories(msisdn, token)?;
            let categories = tags["tags"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["id"] == category)
                .and_then(|t| t["categories"].as_array())
                .ok_or(Error::InvalidInput(
                    "Unknown category. Use ncell plans categories",
                ))?;
            [
                ("GSM_DATA", "data"),
                ("GSM_VOICE", "voice"),
                ("GSM_SMS", "sms"),
            ]
            .into_iter()
            .find(|(name, _)| categories.iter().any(|c| c["name"] == *name))
            .map(|(_, catalog)| catalog)
            .ok_or(Error::InvalidInput("Unsupported category type"))?
        };
        let mut body = json!({
            "msisdn": msisdn,
            "languageCode": "1",
            "tagId": category,
            "requestHeader": request_header(msisdn, "SHOP"),
        });
        if !keyword.is_empty() {
            body["keyword"] = keyword.into();
        }
        let response = self.post(
            &format!("/v1.0/product/{catalog}-plans"),
            &body,
            Some(token),
        )?;
        if !response["availablePackages"].is_array() {
            return Err(Error::InvalidResponse);
        }
        Ok(response)
    }

    pub fn balance(&self, msisdn: &str, token: &str) -> Result<Value> {
        let response = self.post(
            "/v1.0/accountmgt/balance/query",
            &json!({
                "queryBalanceRequest": { "msisdn": msisdn },
                "requestHeader": request_header(msisdn, "MY_SERVICES"),
            }),
            Some(token),
        )?;
        response_object(response, "queryBalanceResponse")
    }
    pub fn profile(&self, msisdn: &str, token: &str) -> Result<Value> {
        let mut header = request_header(msisdn, "PROFILE");
        header["appVersion"] = "8.2.1.0".into();
        let response = self.post(
            "/v1.0/subscriber/profile/query",
            &json!({
                "querySubscriberProfileRequest": {},
                "requestHeader": header,
            }),
            Some(token),
        )?;
        response_object(response, "querySubscriberProfileResponse")
    }
}

fn response_object(mut response: Value, key: &str) -> Result<Value> {
    response
        .as_object_mut()
        .and_then(|body| body.remove(key))
        .filter(Value::is_object)
        .ok_or(Error::InvalidResponse)
}

fn required_text(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or(Error::InvalidResponse)
}

#[cfg(test)]
#[path = "../../tests/unit/api_account.rs"]
mod tests;
