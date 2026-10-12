use super::*;

#[test]
fn scratch_reuses_cookie_and_sends_only_csrf_without_retrying_errors() {
    let mut server = mockito::Server::new();
    let page = server
        .mock("GET", "/index")
        .with_header("set-cookie", "PHPSESSID=scratch; Path=/")
        .with_body("<input name='csrf_token' value='csrf'><div id='tokens-display'>100</div>")
        .expect(1)
        .create();
    let game = GameSession::new(&format!("{}/index", server.url())).unwrap();
    for (body, valid) in [
        (
            r#"{"value":"50","points_remaining":75,"new_week_points":100}"#,
            true,
        ),
        (r#"{"success":false,"error":"Insufficient tokens"}"#, false),
        (r#"{"success":true}"#, false),
    ] {
        let reward = server
            .mock("POST", "/scratch")
            .match_header("cookie", "PHPSESSID=scratch")
            .match_header("user-agent", mockito::Matcher::Missing)
            .match_header("origin", mockito::Matcher::Missing)
            .match_header("referer", mockito::Matcher::Missing)
            .match_header("content-type", "application/x-www-form-urlencoded")
            .match_body("csrf_token=csrf")
            .with_body(body)
            .expect(1)
            .create();
        assert_eq!(
            game.scratch_at(&format!("{}/scratch", server.url()))
                .is_ok(),
            valid
        );
        reward.assert();
        reward.remove();
    }
    page.assert();
}

#[test]
fn readonly_session_does_not_require_csrf_and_never_submits_rewards_without_it() {
    let mut server = mockito::Server::new();
    let page = server
        .mock("GET", "/index")
        .with_body("<div id='tokens-display'>50</div>")
        .expect(1)
        .create();
    let query = server
        .mock("GET", "/status")
        .with_body(r#"{"success":true,"challenges":[]}"#)
        .expect(1)
        .create();
    let mut game = GameSession::open(&format!("{}/index", server.url())).unwrap();
    assert_eq!(
        game.tasks_at(&format!("{}/status", server.url())).unwrap()["success"],
        true
    );
    assert!(game.spin().is_err());
    assert!(game.scratch().is_err());
    assert!(game.claim(&[1], |_| panic!("Must not submit")).is_err());
    assert!(
        game.math(|_| panic!("Must not submit"), |_| panic!("Must not prompt"))
            .is_err()
    );
    page.assert();
    query.assert();
}

#[test]
fn status_claim_and_spin_share_one_bootstrap_and_stream_in_order() {
    let mut server = mockito::Server::new();
    let page = server
        .mock("GET", "/index")
        .with_header("set-cookie", "PHPSESSID=shared; Path=/")
        .with_body("<input name='csrf_token' value='shared-csrf'><div id='tokens-display'>50</div>")
        .expect(1)
        .create();
    let status = server
        .mock("GET", "/status")
        .match_header("cookie", "PHPSESSID=shared")
        .with_body(r#"{"success":true,"challenges":[{"id":1,"completed":true,"rewarded":false}]}"#)
        .expect(1)
        .create();
    let claim = server
        .mock("POST", "/api/record_activity.php")
        .match_header("cookie", "PHPSESSID=shared")
        .match_body(mockito::Matcher::Regex("(?s)activity_id.*1".into()))
        .with_body(r#"{"success":true,"new_token_balance":75}"#)
        .expect(1)
        .create();
    let spin = server
        .mock("POST", "/spin")
        .match_header("cookie", "PHPSESSID=shared")
        .with_body(r#"{"value":"50","new_token_balance":50,"new_week_points":100}"#)
        .expect(1)
        .create();
    let mut game = GameSession::new(&format!("{}/index", server.url())).unwrap();
    assert_eq!(game.tokens, Some(50));
    let tasks = game.tasks_at(&format!("{}/status", server.url())).unwrap();
    assert_eq!(tasks["challenges"][0]["id"], 1);
    let mut streamed = false;
    game.claim_at(&[1], &format!("{}/api", server.url()), &mut |item| {
        assert_eq!(item["id"], 1);
        streamed = true;
        assert!(!spin.matched());
    })
    .unwrap();
    assert!(streamed);
    assert_eq!(game.tokens, Some(75));
    game.spin_at(&format!("{}/spin", server.url())).unwrap();
    for request in [page, status, claim, spin] {
        request.assert();
    }
}

#[test]
fn already_claimed_keeps_balance_but_unknown_success_invalidates_it() {
    let mut server = mockito::Server::new();
    let page = server
        .mock("GET", "/index")
        .with_body("<input name='csrf_token' value='csrf'><div id='tokens-display'>50</div>")
        .expect(1)
        .create();
    let mut game = GameSession::new(&format!("{}/index", server.url())).unwrap();
    for (body, expected) in [
        (
            r#"{"success":false,"message":"Task already completed today"}"#,
            Some(50),
        ),
        (
            r#"{"success":false,"error":"Reward already claimed"}"#,
            Some(50),
        ),
        (r#"{"success":true}"#, None),
    ] {
        let request = server
            .mock("POST", "/api/record_activity.php")
            .with_body(body)
            .expect(1)
            .create();
        game.claim_at(&[1], &format!("{}/api", server.url()), &mut |_| ())
            .unwrap();
        assert_eq!(game.tokens, expected);
        request.assert();
        request.remove();
    }
    page.assert();
}

#[test]
fn reads_bootstrap_balance_including_zero_and_grouped_numbers() {
    for (text, expected) in [("0", 0), ("85", 85), ("1,250", 1250)] {
        let document = Html::parse_document(&format!("<div id='tokens-display'>{text}</div>"));
        assert_eq!(token_balance(&document), Some(expected));
    }
    assert_eq!(token_balance(&Html::parse_document("")), None);
}

#[test]
fn spin_uses_only_csrf_and_cookie_and_preserves_success_and_errors_without_retry() {
    let mut server = mockito::Server::new();
    for (body, valid) in [
        (
            r#"{"name":"50","description":"50 Points","value":"50","new_token_balance":235,"new_week_points":6615}"#,
            true,
        ),
        (r#"{"success":false,"error":"Insufficient tokens"}"#, false),
        (r#"{"success":true}"#, false),
    ] {
        let page = server
            .mock("GET", "/index.php")
            .match_header("user-agent", "wv")
            .with_header("set-cookie", "PHPSESSID=spin-session; Path=/")
            .with_body("<input name='csrf_token' value='spin-csrf'>")
            .expect(1)
            .create();
        let claim = server
            .mock("POST", "/fetchSpinReward.php")
            .match_header("cookie", "PHPSESSID=spin-session")
            .match_header("user-agent", mockito::Matcher::Missing)
            .match_header("referer", mockito::Matcher::Missing)
            .match_header("origin", mockito::Matcher::Missing)
            .match_body(mockito::Matcher::Regex(
                "(?s)name=\"csrf_token\".*spin-csrf".into(),
            ))
            .with_body(body)
            .expect(1)
            .create();
        let result = GameSession::new(&format!("{}/index.php", server.url()))
            .unwrap()
            .spin_at(&format!("{}/fetchSpinReward.php", server.url()));
        assert_eq!(result.is_ok(), valid);
        page.assert();
        claim.assert();
    }
}

#[test]
fn tasks_use_minimal_headers_without_csrf_or_body() {
    let mut server = mockito::Server::new();
    let page = server
        .mock("GET", "/index.php")
        .with_header("set-cookie", "PHPSESSID=tasks-session; Path=/")
        .with_body("No CSRF input needed")
        .expect(1)
        .create();
    let query = server
        .mock("GET", "/api/challenge_status.php")
        .match_header("cookie", "PHPSESSID=tasks-session")
        .match_body("")
        .with_body(r#"{"success":true,"challenges":[],"summary":{"completed_today":0}}"#)
        .expect(1)
        .create();
    let response = tasks(
        &web_client().unwrap(),
        &format!("{}/index.php", server.url()),
        &format!("{}/api/challenge_status.php", server.url()),
    )
    .unwrap();
    assert_eq!(response["summary"]["completed_today"], 0);
    page.assert();
    query.assert();
}

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
        &mut |item| {
            assert_eq!(claim.matched(), item["id"] == 2);
        },
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
    let response = submit(
        &web,
        &format!("{}/index.php", server.url()),
        &[],
        &format!("{}/api", server.url()),
        &mut |_| panic!("Daily check-in must not emit task output"),
    )
    .unwrap();
    assert_eq!(response["message"], "Already checked in today");
    page.assert();
    claim.assert();
}
