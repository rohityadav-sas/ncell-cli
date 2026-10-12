use crate::error::{Error, Result};
use reqwest::blocking::{Client, multipart::Form};
use scraper::{Html, Selector};
use serde_json::Value;
use std::time::Duration;

pub fn check_in(url: &str, activities: &[u64]) -> Result<Value> {
    let web = Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(90))
        .user_agent("Mozilla/5.0 (Linux; Android 13; wv) AppleWebKit/537.36 Version/4.0 Chrome/120.0.0.0 Mobile Safari/537.36")
        .build().map_err(Error::from_http)?;
    submit(
        &web,
        url,
        activities,
        "https://sajeeb.com.np/ncellapp/checkin/api",
    )
}

fn submit(web: &Client, url: &str, activities: &[u64], base: &str) -> Result<Value> {
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
    let selector = Selector::parse("input[name=csrf_token]").unwrap();
    let csrf = document
        .select(&selector)
        .next()
        .and_then(|input| input.value().attr("value"))
        .filter(|token| !token.is_empty())
        .ok_or(Error::InvalidInput(
            "Check-in page did not provide a CSRF token",
        ))?;
    if activities.is_empty() {
        claim(web, &format!("{base}/checkin.php"), csrf, None)
    } else {
        let results = activities
            .iter()
            .map(|&id| {
                let response = claim(web, &format!("{base}/record_activity.php"), csrf, Some(id));
                match response {
                    Ok(response) => Ok(serde_json::json!({"id": id, "response": response})),
                    Err(Error::CheckIn { response, .. }) => {
                        Ok(serde_json::json!({"id": id, "response": response}))
                    }
                    Err(error) => Err(error),
                }
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Value::Array(results))
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
    let status = response.status();
    let data: Value = response.json().map_err(|_| Error::InvalidResponse)?;
    if !status.is_success() || data["success"] != true {
        return Err(Error::CheckIn {
            message: data["message"]
                .as_str()
                .or_else(|| data["error"].as_str())
                .unwrap_or("Check-in failed")
                .to_owned(),
            status: status.as_u16(),
            response: Box::new(data),
        });
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_sends_id_and_fresh_csrf_once() {
        let mut server = mockito::Server::new();
        let page = server
            .mock("GET", "/earnmore.php")
            .with_header("set-cookie", "PHPSESSID=activity-session; Path=/")
            .with_body("<input name='csrf_token' value='activity-token'>")
            .create();
        let claim = server.mock("POST", "/api/record_activity.php")
            .match_header("cookie", "PHPSESSID=activity-session")
            .match_body(mockito::Matcher::AllOf(vec![
                mockito::Matcher::Regex("(?s)name=\"activity_id\"\\r\\n\\r\\n[12]\\r\\n".into()),
                mockito::Matcher::Regex("(?s)name=\"csrf_token\"\\r\\n\\r\\nactivity-token\\r\\n".into()),
            ]))
            .with_body(r#"{"success":true,"points_awarded":25,"new_token_balance":130,"new_week_points":6565}"#)
            .expect(2).create();
        let web = Client::builder().cookie_store(true).build().unwrap();
        let result = submit(
            &web,
            &format!("{}/earnmore.php", server.url()),
            &[1, 2],
            &format!("{}/api", server.url()),
        )
        .unwrap();
        assert_eq!(result[0]["response"]["points_awarded"], 25);
        page.assert();
        claim.assert();
    }

    #[test]
    fn check_in_uses_fresh_csrf_cookie_and_preserves_rejection_without_retry() {
        let mut server = mockito::Server::new();
        let page = server
            .mock("GET", "/index.php")
            .with_header("set-cookie", "PHPSESSID=test-session; Path=/; HttpOnly")
            .with_body("<input value='fresh-token' type='hidden' name='csrf_token'>")
            .expect(1)
            .create();
        let claim = server
            .mock("POST", "/api/checkin.php")
            .match_header("cookie", "PHPSESSID=test-session")
            .match_body(mockito::Matcher::Regex(
                "(?s)name=\"csrf_token\".*fresh-token".into(),
            ))
            .with_body(r#"{"success":false,"message":"Already checked in today"}"#)
            .expect(1)
            .create();
        let web = Client::builder().cookie_store(true).build().unwrap();
        let error = submit(
            &web,
            &format!("{}/index.php", server.url()),
            &[],
            &format!("{}/api", server.url()),
        )
        .unwrap_err();
        match error {
            Error::CheckIn { response, .. } => {
                assert_eq!(response["message"], "Already checked in today")
            }
            _ => panic!("Expected check-in rejection"),
        }
        page.assert();
        claim.assert();
    }
}
