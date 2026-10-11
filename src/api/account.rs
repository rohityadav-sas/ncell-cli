use super::{Client, models::request_header};
use crate::error::{Error, Result};
use serde_json::{Value, json};

impl Client {
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

    pub fn tags(&self, msisdn: &str, token: &str) -> Result<Value> {
        let mut header = request_header(msisdn, "N/A");
        header["appVersion"] = "8.2.1.0".into();
        let response = self.post(
            "/v1.0/product-catalog/tags",
            &json!({"requestHeader": header}),
            Some(token),
        )?;
        if !response["tags"].is_array() {
            return Err(Error::InvalidResponse);
        }
        Ok(response)
    }

    pub fn plans(&self, msisdn: &str, token: &str, tag: u64) -> Result<Value> {
        self.search_plans(msisdn, token, tag, "")
    }

    pub fn search_plans(
        &self,
        msisdn: &str,
        token: &str,
        tag: u64,
        keyword: &str,
    ) -> Result<Value> {
        let catalog = if tag == 66 {
            "data"
        } else {
            let tags = self.tags(msisdn, token)?;
            let categories = tags["tags"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["id"] == tag)
                .and_then(|t| t["categories"].as_array())
                .ok_or(Error::InvalidInput("Unknown tag. Use plans --list-tags"))?;
            [
                ("GSM_DATA", "data"),
                ("GSM_VOICE", "voice"),
                ("GSM_SMS", "sms"),
            ]
            .into_iter()
            .find(|(name, _)| categories.iter().any(|c| c["name"] == *name))
            .map(|(_, catalog)| catalog)
            .ok_or(Error::InvalidInput("Unsupported tag category"))?
        };
        let mut body = json!({
            "msisdn": msisdn,
            "languageCode": "1",
            "tagId": tag,
            "sortby": { "name": "price" },
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
