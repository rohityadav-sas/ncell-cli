use super::{
    APP_VERSION, CATEGORY_TTL_SECS, Client, GAME_CLIENT_ID, PLANS_TTL_SECS, models::request_header,
};
use crate::error::{Error, Result};
use serde_json::{Value, json};

impl Client {
    pub fn check_in_url(&self, msisdn: &str, token: &str) -> Result<String> {
        let response = self.post(
            "/v1.0/utilitymgt/third-party-encryption",
            &json!({
                "thirdPartyClientRequest": {"clientId": GAME_CLIENT_ID},
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
            "{}?MSISDN={encrypted}",
            super::game_endpoint("index.php")
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
        header["appVersion"] = APP_VERSION.into();
        let (status, response) = self.post_raw(
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
        // Success is judged here, not in the HTTP layer: Ncell answers
        // subscriptions with BIL1000 instead of the usual code 200.
        // An HTTP error still wins over a success-looking body.
        let subscribed = (200..300).contains(&status)
            && (response["responseHeader"]["responseDescDisplay"] == "BIL1000"
                || response["productSubscriptionSummaryResponse"]["status"]
                    .as_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case("success")));
        if subscribed {
            // The bought pack may vanish from the list; drop cached plan pages
            // so the next lookup refetches instead of showing a stale entry.
            if let Ok(mut cache) = self.plans_cache.lock() {
                let prefix = format!("{msisdn}:");
                cache.retain(|key, _| !key.starts_with(&prefix));
            }
            Ok(response)
        } else {
            Err(super::client::api_error(status, response))
        }
    }

    pub fn categories(&self, msisdn: &str, token: &str) -> Result<Value> {
        let cache = self.categories_cache.lock().map_err(|_| Error::Storage)?;
        if let Some((created, entry)) = cache.get(msisdn)
            && created.elapsed() < std::time::Duration::from_secs(CATEGORY_TTL_SECS)
        {
            return Ok(entry.clone());
        }
        // Release the mutex before the slow request, then re-lock only to store.
        drop(cache);
        let header = request_header(msisdn, "N/A");
        let response = self.post(
            "/v1.0/product-catalog/tags",
            &json!({"requestHeader": header}),
            Some(token),
        )?;
        if !response["tags"].is_array() {
            return Err(Error::InvalidResponse);
        }
        self.categories_cache
            .lock()
            .map_err(|_| Error::Storage)?
            .insert(
                msisdn.to_owned(),
                (std::time::Instant::now(), response.clone()),
            );
        Ok(response)
    }

    /// Category IDs arrive as numbers from some endpoints and as strings from
    /// others; accept both so a string ID never reports "unknown category".
    fn category_id_matches(value: &Value, category: u64) -> bool {
        value.as_u64().is_some_and(|id| id == category)
            || value
                .as_str()
                .is_some_and(|id| id.parse::<u64>().is_ok_and(|parsed| parsed == category))
    }

    pub fn search_plans(
        &self,
        msisdn: &str,
        token: &str,
        category: u64,
        keyword: &str,
    ) -> Result<Value> {
        // The Ctrl+Space prefetch and the subscribe lookup ask for the same
        // page (same category, empty keyword). Serve repeats from memory so a
        // second `plans subscribe 1225` does not refetch over the network.
        let key = format!("{msisdn}:{category}:{keyword}");
        if let Ok(cache) = self.plans_cache.lock()
            && let Some((created, entry)) = cache.get(&key)
            && created.elapsed() < std::time::Duration::from_secs(PLANS_TTL_SECS)
        {
            return Ok(entry.clone());
        }
        let catalog = if category == super::DEFAULT_PLAN_CATEGORY {
            "data"
        } else {
            let tags = self.categories(msisdn, token)?;
            let tags = tags["tags"].as_array().ok_or(Error::InvalidResponse)?;
            let categories = tags
                .iter()
                .find(|t| Self::category_id_matches(&t["id"], category))
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
            "sortby": {"name": "price", "isDescending": false},
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
        if let Ok(mut cache) = self.plans_cache.lock() {
            cache.insert(key, (std::time::Instant::now(), response.clone()));
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
        header["appVersion"] = APP_VERSION.into();
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
