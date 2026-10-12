use crate::error::{Error, Result};
use reqwest::blocking::{Client, multipart::Form};
use serde_json::{Value, json};

const ENDPOINT: &str = "https://sajeeb.com.np/ncellapp/checkin/math_challenge_api.php";

impl super::check_in::GameSession {
    pub fn math(
        &self,
        on_answer: impl FnMut(&Value),
        manual_answer: impl FnMut(&Value) -> Result<String>,
    ) -> Result<Value> {
        super::check_in::require_csrf(&self.csrf)?;
        play(&self.web, ENDPOINT, &self.csrf, on_answer, manual_answer)
    }
}

fn play(
    web: &Client,
    endpoint: &str,
    csrf: &str,
    mut on_answer: impl FnMut(&Value),
    mut manual_answer: impl FnMut(&Value) -> Result<String>,
) -> Result<Value> {
    let start = request(web, endpoint, csrf, "start_game", None)?;
    if start["success"] != true {
        return Err(Error::InvalidResponse);
    }
    // Captured games have 20 levels and three lives; bound unexpected server loops.
    for _ in 0..60 {
        let question = request(web, endpoint, csrf, "get_question", None)?;
        if question["win"] == true || question["game_over"] == true {
            return Ok(question);
        }
        let answer = match solve(&question) {
            Ok(answer) => answer,
            Err(Error::InvalidInput(_)) => manual_answer(&question)?,
            Err(error) => return Err(error),
        };
        let response = request(web, endpoint, csrf, "submit_answer", Some(&answer))?;
        on_answer(
            &json!({"level":question["level"], "question":question["question"],
            "answer":answer, "response":response}),
        );
        match response["result"].as_str() {
            Some("win" | "game_over" | "lose") => return Ok(response),
            Some("correct" | "wrong") => (),
            _ => return Err(Error::InvalidResponse),
        }
    }
    Err(Error::InvalidInput(
        "Math game exceeded the expected number of questions",
    ))
}

fn request(
    web: &Client,
    endpoint: &str,
    csrf: &str,
    action: &str,
    answer: Option<&str>,
) -> Result<Value> {
    let mut form = Form::new()
        .text("action", action.to_owned())
        .text("csrf_token", csrf.to_owned());
    if let Some(answer) = answer {
        form = form.text("answer", answer.to_owned());
    }
    let response = web
        .post(endpoint)
        .multipart(form)
        .send()
        .map_err(Error::from_http)?;
    let status = response.status();
    let data: Value = response.json().map_err(|_| Error::InvalidResponse)?;
    if !status.is_success() || data["success"] == false || !data["error"].is_null() {
        return Err(Error::CheckIn {
            message: data["error"]
                .as_str()
                .or_else(|| data["message"].as_str())
                .unwrap_or("Math game request failed")
                .into(),
            status: status.as_u16(),
            response: Box::new(data),
        });
    }
    Ok(data)
}

fn solve(question: &Value) -> Result<String> {
    let expression = question["question"]
        .as_str()
        .ok_or(Error::InvalidResponse)?
        .replace('×', "*")
        .replace('÷', "/")
        .replace('−', "-")
        .replace("% of ", "/100 * ");
    let mut functions = |name: &str, values: Vec<f64>| {
        (name == "avg" && !values.is_empty())
            .then(|| values.iter().sum::<f64>() / values.len() as f64)
    };
    let answer = fasteval::ez_eval(&expression, &mut functions)
        .ok()
        .filter(|value| value.is_finite())
        .ok_or(Error::InvalidInput(
            "Unsupported math question; no answer submitted",
        ))?;
    question["options"]
        .as_array()
        .ok_or(Error::InvalidResponse)?
        .iter()
        .find(|option| {
            option
                .as_f64()
                .is_some_and(|value| (value - answer).abs() < 1e-8)
        })
        .map(Value::to_string)
        .ok_or(Error::InvalidInput(
            "Calculated answer is not among the options; no answer submitted",
        ))
}

#[cfg(test)]
#[path = "../../tests/unit/api_game_math.rs"]
mod tests;
