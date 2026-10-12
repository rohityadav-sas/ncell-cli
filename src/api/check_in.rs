use crate::error::{Error, Result};
use reqwest::blocking::{Client, multipart::Form};
use scraper::{Html, Selector};
use serde_json::Value;
use std::time::Duration;

pub fn check_in(url: &str) -> Result<Value> {
    let web = Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(90))
        .user_agent("Mozilla/5.0 (Linux; Android 13; wv) AppleWebKit/537.36 Version/4.0 Chrome/120.0.0.0 Mobile Safari/537.36")
        .build().map_err(Error::from_http)?;
    submit(
        &web,
        url,
        "https://sajeeb.com.np/ncellapp/checkin/api/checkin.php",
    )
}

fn submit(web: &Client, url: &str, endpoint: &str) -> Result<Value> {
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
    let response = web
        .post(endpoint)
        .multipart(Form::new().text("csrf_token", csrf.to_owned()))
        .send()
        .map_err(Error::from_http)?;
    let status = response.status();
    let data: Value = response.json().map_err(|_| Error::InvalidResponse)?;
    if !status.is_success() || data["success"] != true {
        return Err(Error::CheckIn {
            message: data["message"]
                .as_str()
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
            &format!("{}/api/checkin.php", server.url()),
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
