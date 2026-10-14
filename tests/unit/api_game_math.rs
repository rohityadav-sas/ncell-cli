use super::*;
use crate::api::check_in::web_client;

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
                .match_header("user-agent", "wv")
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
