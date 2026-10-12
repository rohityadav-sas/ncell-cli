#[cfg(test)]
use super::check_in::web_client;
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
mod tests {
    use super::*;

    #[test]
    fn complete_game_streams_twenty_answers_without_extra_requests() {
        let mut server = mockito::Server::new();
        let stage = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = stage.clone();
        let request = server.mock("POST", "/math")
            .with_body_from_request(move |request| {
                let body = String::from_utf8_lossy(request.body().unwrap());
                let step = observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let value = if step == 0 {
                    assert!(body.contains("start_game"));
                    json!({"success":true,"balance":75})
                } else if step % 2 == 1 {
                    assert!(body.contains("get_question"));
                    json!({"level":step.div_ceil(2),"question":"2 + 3","options":[5,6,7,8]})
                } else {
                    assert!(body.contains("submit_answer"));
                    assert!(body.contains("\r\n5\r\n"));
                    json!({"result":if step==40 {"win"} else {"correct"},"total_score":step/2*5,"bg_reward":10})
                };
                value.to_string().into_bytes()
            }).expect(41).create();
        let mut answers = 0;
        let response = play(
            &web_client().unwrap(),
            &format!("{}/math", server.url()),
            "csrf",
            |item| {
                answers += 1;
                assert_eq!(item["level"], answers);
                assert_eq!(
                    stage.load(std::sync::atomic::Ordering::SeqCst),
                    1 + answers * 2
                );
            },
            |_| panic!("Must not prompt"),
        )
        .unwrap();
        assert_eq!(answers, 20);
        assert_eq!(response["result"], "win");
        assert_eq!(response["bg_reward"], 10);
        request.assert();
    }

    #[test]
    fn failed_start_is_not_retried_and_never_fetches_questions() {
        let mut server = mockito::Server::new();
        let start = server
            .mock("POST", "/math")
            .match_body(mockito::Matcher::Regex("(?s)start_game".into()))
            .with_body(r#"{"success":false,"error":"Not enough points"}"#)
            .expect(1)
            .create();
        let question = server
            .mock("POST", "/math")
            .match_body(mockito::Matcher::Regex("(?s)get_question".into()))
            .expect(0)
            .create();
        assert!(
            play(
                &web_client().unwrap(),
                &format!("{}/math", server.url()),
                "csrf",
                |_| panic!("Must not answer"),
                |_| panic!("Must not prompt")
            )
            .is_err()
        );
        start.assert();
        question.assert();
    }

    #[test]
    fn repeated_games_reuse_bootstrap_cookie_and_csrf() {
        let mut server = mockito::Server::new();
        let page = server
            .mock("GET", "/index")
            .with_header("set-cookie", "PHPSESSID=reused; Path=/")
            .with_body("<input name='csrf_token' value='shared'>")
            .expect(1)
            .create();
        let start = server
            .mock("POST", "/math")
            .match_header("cookie", "PHPSESSID=reused")
            .match_body(mockito::Matcher::Regex("(?s)start_game.*shared".into()))
            .with_body(r#"{"success":true}"#)
            .expect(2)
            .create();
        let question = server
            .mock("POST", "/math")
            .match_header("cookie", "PHPSESSID=reused")
            .match_body(mockito::Matcher::Regex("(?s)get_question.*shared".into()))
            .with_body(r#"{"win":true,"total_score":165}"#)
            .expect(2)
            .create();
        let session =
            super::super::check_in::GameSession::new(&format!("{}/index", server.url())).unwrap();
        for _ in 0..2 {
            play(
                &session.web,
                &format!("{}/math", server.url()),
                &session.csrf,
                |_| (),
                |_| panic!("No prompt expected"),
            )
            .unwrap();
        }
        page.assert();
        start.assert();
        question.assert();
    }

    #[test]
    fn solves_captured_question_types_without_guessing() {
        for (question, answer) in [
            ("6 + 6", 12),
            ("6 - 4", 2),
            ("11 + 3 × 9", 38),
            ("120 ÷ 10", 12),
            ("(5 × 10) + (4 × 3)", 62),
            ("25% of 8", 2),
            ("avg(22, 26, 12)", 20),
        ] {
            assert_eq!(
                solve(&json!({"question":question,"options":[answer,999]})).unwrap(),
                answer.to_string()
            );
        }
        for question in ["1 / 0", "unknown(5)", "1 + 1"] {
            assert!(solve(&json!({"question":question,"options":[999]})).is_err());
        }
    }

    #[test]
    fn unsupported_question_submits_manual_answer_and_continues() {
        let mut server = mockito::Server::new();
        let start = server
            .mock("POST", "/math")
            .match_body(mockito::Matcher::Regex(
                "(?s)name=\"action\".*start_game".into(),
            ))
            .with_body(r#"{"success":true}"#)
            .expect(1)
            .create();
        let question = server
            .mock("POST", "/math")
            .match_body(mockito::Matcher::Regex(
                "(?s)name=\"action\".*get_question".into(),
            ))
            .with_body(r#"{"question":"unknown(5)","options":[2,4],"level":20}"#)
            .expect(1)
            .create();
        let answer = server
            .mock("POST", "/math")
            .match_body(mockito::Matcher::Regex(
                "(?s)name=\"action\".*submit_answer.*name=\"answer\"\r\n\r\n2\r\n".into(),
            ))
            .with_body(r#"{"result":"win","total_score":155}"#)
            .expect(1)
            .create();
        let response = play(
            &web_client().unwrap(),
            &format!("{}/math", server.url()),
            "csrf",
            |_| (),
            |question| {
                assert_eq!(question["question"], "unknown(5)");
                Ok("2".into())
            },
        )
        .unwrap();
        assert_eq!(response["result"], "win");
        start.assert();
        question.assert();
        answer.assert();
    }

    #[test]
    fn plays_in_order_and_preserves_win_without_replaying_answers() {
        let mut server = mockito::Server::new();
        let mut mocks = Vec::new();
        for (action, response) in [
            ("start_game", json!({"success":true,"balance":100})),
            (
                "get_question",
                json!({"level":20,"question":"25% of 8","options":[2,10,4,7]}),
            ),
            (
                "submit_answer",
                json!({"result":"win","bg_reward":10,"total_score":155}),
            ),
        ] {
            mocks.push(
                server
                    .mock("POST", "/math")
                    .match_header("user-agent", mockito::Matcher::Missing)
                    .match_header("origin", mockito::Matcher::Missing)
                    .match_header("referer", mockito::Matcher::Missing)
                    .match_body(mockito::Matcher::Regex(format!(
                        "(?s)name=\"action\".*{action}.*name=\"csrf_token\".*csrf"
                    )))
                    .with_body(response.to_string())
                    .expect(1)
                    .create(),
            );
        }
        let result = play(
            &web_client().unwrap(),
            &format!("{}/math", server.url()),
            "csrf",
            |item| {
                assert_eq!(item["answer"], "2");
            },
            |_| panic!("Supported questions should not prompt"),
        )
        .unwrap();
        assert_eq!(result["total_score"], 155);
        for mock in mocks {
            mock.assert();
        }
    }
}
