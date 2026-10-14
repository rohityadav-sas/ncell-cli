use crate::error::{Error, Result};
use reqwest::blocking::{Client, multipart::Form};
use scraper::{Html, Selector};
use serde_json::Value;
use std::time::Duration;

pub(super) const WEB_UA: &str = "wv";

pub(super) fn web_client() -> Result<Client> {
    Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(90))
        .user_agent(WEB_UA)
        .build()
        .map_err(Error::from_http)
}

fn bootstrap(web: &Client, url: &str) -> Result<(String, Option<u64>)> {
    let page = web.get(url).send().map_err(Error::from_http)?;
    if !page.status().is_success() {
        return Err(Error::CheckIn {
            message: "Check-in website rejected the request".into(),
            status: page.status().as_u16(),
            response: Box::new(Value::Null),
        });
    }
    let html = page.text().map_err(Error::from_http)?;
    let document = Html::parse_document(&html);
    static SELECTOR: std::sync::LazyLock<Selector> =
        std::sync::LazyLock::new(|| Selector::parse("input[name=csrf_token]").unwrap());
    let selector = &*SELECTOR;
    let csrf = document
        .select(selector)
        .next()
        .and_then(|input| input.value().attr("value"))
        .filter(|token| !token.is_empty())
        .unwrap_or_default()
        .to_owned();
    Ok((csrf, token_balance(&document)))
}

fn token_balance(document: &Html) -> Option<u64> {
    static SELECTOR: std::sync::LazyLock<Selector> =
        std::sync::LazyLock::new(|| Selector::parse("#tokens-display").unwrap());
    document
        .select(&SELECTOR)
        .next()?
        .text()
        .collect::<String>()
        .trim()
        .replace(',', "")
        .parse()
        .ok()
}

pub struct GameSession {
    pub(super) web: Client,
    pub(super) csrf: String,
    pub tokens: Option<u64>,
}
impl GameSession {
    pub fn new(url: &str) -> Result<Self> {
        let game = Self::open(url)?;
        require_csrf(&game.csrf)?;
        Ok(game)
    }
    /// Open a session for status or rankings; read-only requests do not require CSRF.
    pub fn open(url: &str) -> Result<Self> {
        let web = web_client()?;
        let (csrf, tokens) = bootstrap(&web, url)?;
        Ok(Self { web, csrf, tokens })
    }
    pub fn tasks(&self) -> Result<Value> {
        self.tasks_at(&super::game_endpoint("api/challenge_status.php"))
    }
    fn tasks_at(&self, endpoint: &str) -> Result<Value> {
        decode(
            self.web.get(endpoint).send().map_err(Error::from_http)?,
            false,
        )
    }
    pub fn ranks(&self) -> Result<Value> {
        super::game_ranks::fetch(&self.web)
    }
    pub fn claim(&mut self, activities: &[u64], mut on_task: impl FnMut(&Value)) -> Result<Value> {
        require_csrf(&self.csrf)?;
        self.claim_at(activities, &super::game_endpoint("api"), &mut on_task)
    }
    fn claim_at(
        &mut self,
        activities: &[u64],
        base: &str,
        on_task: &mut impl FnMut(&Value),
    ) -> Result<Value> {
        if activities.is_empty() {
            let result = match claim(&self.web, &format!("{base}/checkin.php"), &self.csrf, None) {
                Err(Error::CheckIn { response, .. }) if already_checked_in(&response) => {
                    Ok(*response)
                }
                result => result,
            }?;
            if !already_checked_in(&result) {
                self.tokens = result["data"]["game_tokens"].as_u64();
            }
            return Ok(result);
        }
        let mut results = Vec::with_capacity(activities.len());
        for &id in activities {
            let response = match claim(
                &self.web,
                &format!("{base}/record_activity.php"),
                &self.csrf,
                Some(id),
            ) {
                Ok(response) => response,
                Err(Error::CheckIn { response, .. }) => *response,
                Err(error) => return Err(error),
            };
            if !already_claimed(&response) {
                self.tokens = response["new_token_balance"].as_u64();
            }
            let item = serde_json::json!({"id":id,"response":response});
            on_task(&item);
            results.push(item);
        }
        Ok(results.into())
    }
    pub fn spin(&self) -> Result<Value> {
        require_csrf(&self.csrf)?;
        self.spin_at(&super::game_endpoint("fetchSpinReward.php"))
    }
    fn spin_at(&self, endpoint: &str) -> Result<Value> {
        decode(
            self.web
                .post(endpoint)
                .multipart(Form::new().text("csrf_token", self.csrf.clone()))
                .send()
                .map_err(Error::from_http)?,
            true,
        )
    }
    pub fn scratch(&self) -> Result<Value> {
        require_csrf(&self.csrf)?;
        self.scratch_at(&super::game_endpoint("fetchScratchReward.php"))
    }
    fn scratch_at(&self, endpoint: &str) -> Result<Value> {
        decode(
            self.web
                .post(endpoint)
                .form(&[("csrf_token", &self.csrf)])
                .send()
                .map_err(Error::from_http)?,
            true,
        )
    }
}

pub(super) fn require_csrf(csrf: &str) -> Result<()> {
    if csrf.is_empty() {
        Err(Error::InvalidInput(
            "Check-in page did not provide a CSRF token",
        ))
    } else {
        Ok(())
    }
}

fn claim(web: &Client, endpoint: &str, csrf: &str, activity: Option<u64>) -> Result<Value> {
    let mut form = Form::new().text("csrf_token", csrf.to_owned());
    if let Some(id) = activity {
        form = form.text("activity_id", id.to_string());
    }
    let response = web
        .post(endpoint)
        .multipart(form)
        .send()
        .map_err(Error::from_http)?;
    decode(response, false)
}

pub fn already_claimed(data: &Value) -> bool {
    ["Task already completed today", "Reward already claimed"].contains(
        &data["error"]
            .as_str()
            .or_else(|| data["message"].as_str())
            .unwrap_or(""),
    )
}

pub fn already_checked_in(data: &Value) -> bool {
    let message = data["message"]
        .as_str()
        .or_else(|| data["error"].as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    message.contains("already checked")
}

/// Server-controlled game text, stripped of control characters at the boundary
/// so no print site can leak terminal escapes into one-shot or error output.
pub(super) fn clean_message(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_control()).collect()
}

/// Read a game JSON body together with its HTTP status. Non-JSON error pages
/// (a 502 HTML page, a login redirect) report their status instead of a
/// misleading "invalid response".
pub(super) fn game_json(
    response: reqwest::blocking::Response,
    service: &str,
) -> Result<(u16, Value)> {
    let status = response.status().as_u16();
    let data: Value = response.json().map_err(|_| {
        if (200..300).contains(&status) {
            Error::InvalidResponse
        } else {
            Error::CheckIn {
                message: format!("{service} returned HTTP {status}"),
                status,
                response: Box::new(Value::Null),
            }
        }
    })?;
    Ok((status, data))
}

fn decode(response: reqwest::blocking::Response, reward: bool) -> Result<Value> {
    let (status, data) = game_json(response, "Game website")?;
    let success = if reward {
        data["value"].is_string()
            && (data["new_token_balance"].is_number() || data["points_remaining"].is_number())
            && data["new_week_points"].is_number()
            && data["error"].is_null()
            && data["success"] != false
    } else {
        data["success"] == true
    };
    if !(200..300).contains(&status) || !success {
        return Err(Error::CheckIn {
            message: clean_message(
                data["message"]
                    .as_str()
                    .or_else(|| data["error"].as_str())
                    .unwrap_or("Game request failed"),
            ),
            status,
            response: Box::new(data),
        });
    }
    Ok(data)
}

#[cfg(test)]
#[path = "../../tests/unit/api_check_in.rs"]
mod tests;
