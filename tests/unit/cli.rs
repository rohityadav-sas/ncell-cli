use super::*;
use serde_json::json;

#[test]
fn session_cache_reuses_identity_and_rebuilds_for_account_device_token_age_and_unknown_balance() {
    let mut server = mockito::Server::new();
    let page = server
        .mock("GET", "/index")
        .with_body("<input name='csrf_token' value='csrf'><div id='tokens-display'>100</div>")
        .expect(6)
        .create();
    let mut runtime = Runtime::default();
    let mut creations = 0;
    let mut create = |identity: &str| {
        creations += 1;
        Ok((
            identity.to_owned(),
            ncell_cli::api::GameSession::open(&format!("{}/index", server.url()))?,
        ))
    };
    for identity in [
        "account-a:device-a:token-a",
        "account-a:device-a:token-a",
        "account-b:device-a:token-a",
        "account-b:device-b:token-a",
        "account-b:device-b:token-b",
    ] {
        runtime
            .cached_game(identity.into(), || create(identity))
            .unwrap();
    }
    runtime.game.as_mut().unwrap().1 -= Duration::from_secs(601);
    runtime
        .cached_game("account-b:device-b:token-b".into(), || {
            create("account-b:device-b:token-b")
        })
        .unwrap();
    runtime.game.as_mut().unwrap().2.tokens = None;
    runtime
        .cached_game("account-b:device-b:token-b".into(), || {
            create("account-b:device-b:token-b")
        })
        .unwrap();
    assert_eq!(creations, 6);
    page.assert();
}

#[test]
fn refreshed_identity_is_cached_and_bootstrap_errors_are_not_hidden() {
    let mut server = mockito::Server::new();
    let page = server
        .mock("GET", "/index")
        .with_body("<input name='csrf_token' value='csrf'><div id='tokens-display'>0</div>")
        .expect(1)
        .create();
    let mut runtime = Runtime::default();
    assert!(matches!(
        runtime.cached_game("old".into(), || Err(Error::Network)),
        Err(Error::Network)
    ));
    assert!(runtime.game.is_none());
    runtime
        .cached_game("old".into(), || {
            Ok((
                "rotated".into(),
                ncell_cli::api::GameSession::open(&format!("{}/index", server.url()))?,
            ))
        })
        .unwrap();
    runtime
        .cached_game("rotated".into(), || {
            panic!("Rotated identity must reuse the session")
        })
        .unwrap();
    assert!(matches!(
        runtime.cached_game("different-account".into(), || Err(Error::Connection)),
        Err(Error::Connection)
    ));
    assert_eq!(runtime.game.as_ref().unwrap().0, "rotated");
    page.assert();
}

#[test]
fn all_game_progress_uses_affordable_entries_without_cap() {
    // Games playable from here on: already played plus wallet divided by entry fee.
    let total = |played: u64, tokens: u64| played.saturating_add(tokens / 25);
    assert_eq!(total(0, 85), 3);
    assert_eq!(total(1, 60), 3);
    assert_eq!(total(3, 10), 3);
    assert_eq!(total(0, 0), 0);
    assert_eq!(total(0, 5000), 200);
    assert_eq!(total(1, 85), 4);
}

#[test]
fn completed_tasks_are_normal_results_but_other_failures_are_errors() {
    assert!(!task_failed(&json!({"success": true})));
    assert!(!task_failed(
        &json!({"success": false, "error": "Task already completed today"})
    ));
    assert!(task_failed(
        &json!({"success": false, "error": "Invalid CSRF token"})
    ));
    assert!(task_failed(&json!({})));
}

#[test]
fn game_arguments_require_one_action_and_task_ids() {
    let args = Args::try_parse_from(["ncell", "game", "claim-task", "1,2,3"]).unwrap();
    assert!(
        matches!(args.command, Command::Game { action: GameCommand::ClaimTask { ids } } if ids == [TaskId::Id(1), TaskId::Id(2), TaskId::Id(3)])
    );
    assert!(Args::try_parse_from(["ncell", "game", "check-in"]).is_ok());
    for flags in [
        vec![],
        vec!["claim-task"],
        vec!["--check-in"],
        vec!["claim-task", "0"],
        vec!["claim-task", "1,,2"],
    ] {
        assert!(Args::try_parse_from(["ncell", "game"].into_iter().chain(flags)).is_err());
    }
    for args in [
        vec!["ncell", "check-in"],
        vec!["ncell", "plans", "--list-categories"],
        vec!["ncell", "plans", "--search", "SMS"],
        vec!["ncell", "subscribe", "--id", "984"],
        vec!["ncell", "-h"],
        vec!["ncell", "-V"],
    ] {
        assert!(Args::try_parse_from(args).is_err());
    }
}

#[test]
fn profile_status_labels_match_captures_and_preserve_unknown_codes() {
    assert_eq!(profile_status("B01"), "Active (B01)");
    assert_eq!(profile_status("B03"), "All Services Blocked (B03)");
    assert_eq!(profile_status("B02"), "B02");
    assert_eq!(profile_status("B04"), "Outgoing Blocked (B04)");
    assert_eq!(profile_status("B99"), "B99");
    assert_eq!(profile_status(""), "");
}

#[test]
fn resource_actions_are_scoped_and_options_stay_with_their_action() {
    for command in [
        vec!["plans"],
        vec!["plans", "--category", "50"],
        vec!["plans", "categories"],
        vec!["docs"],
        vec![
            "plans",
            "search",
            "SMS",
            "--category",
            "50",
            "--sort",
            "validity:desc,price:asc",
        ],
        vec!["plans", "subscribe", "984", "--category", "50"],
        vec!["sms", "send-free", "9812345678", "Hello"],
    ] {
        assert!(Args::try_parse_from(std::iter::once("ncell").chain(command)).is_ok());
    }
    for command in [
        vec!["categories"],
        vec!["search", "SMS"],
        vec!["subscribe", "984"],
        vec!["send-free-sms", "9812345678", "Hello"],
        vec!["plans", "subscribe"],
        vec!["sms"],
        vec!["plans", "--category", "50", "search", "SMS"],
        vec!["plans", "categories", "--sort", "price"],
    ] {
        assert!(Args::try_parse_from(std::iter::once("ncell").chain(command)).is_err());
    }
}

#[test]
fn math_summaries_preserve_zero_scores_and_classify_losses() {
    for response in [
        json!({"result":"game_over","score":0}),
        json!({"game_over":true,"total_score":null,"score":0}),
        json!({"result":"lose","score":0}),
    ] {
        let (won, message) = math_summary(&response);
        assert!(!won);
        assert_eq!(message, "Math game_over | Score 0");
    }
    assert_eq!(
        math_summary(&json!({"result":"win","total_score":165,"score":145,"bg_reward":10})),
        (true, "Math win | Score 165".into())
    );
    assert!(math_summary(&json!({"win":true,"score":165,"bg_reward":10})).0);
}

#[test]
fn manual_math_answer_validates_options_and_stops_on_eof() {
    let question = json!({"question":"unknown(5)","options":[2,4],"time":30});
    let mut prompt = Vec::new();
    assert_eq!(
        manual_math_answer(&question, &mut &b"bad\n99\n2\n"[..], &mut prompt).unwrap(),
        "2"
    );
    assert!(
        String::from_utf8(prompt)
            .unwrap()
            .contains("Choose one of the displayed answer values.")
    );
    assert!(manual_math_answer(&question, &mut &b""[..], &mut Vec::new()).is_err());
}

#[test]
fn spin_count_and_delay_accept_units_and_reject_invalid_values() {
    for (input, seconds) in [
        ("0", 0.0),
        ("1.5", 1.5),
        ("1.5s", 1.5),
        ("2m", 120.0),
        ("1.5h", 5400.0),
    ] {
        assert_eq!(
            parse_delay(input).unwrap(),
            Duration::from_secs_f64(seconds)
        );
        for game in ["spin", "scratch", "math"] {
            assert!(
                Args::try_parse_from(["ncell", "game", game, "--count", "3", "--delay", input])
                    .is_ok()
            );
        }
    }
    for input in ["-1", "NaN", "inf", "1e100", "1ms", "1h30m", "", "s"] {
        assert!(parse_delay(input).is_err());
    }
    for count in ["0", "-1", "1.5"] {
        assert!(Args::try_parse_from(["ncell", "game", "spin", "--count", count]).is_err());
    }
}

#[test]
fn all_game_and_task_helpers_are_strict() {
    for game in ["spin", "scratch", "math"] {
        assert!(
            Args::try_parse_from(["ncell", "game", game, "--count", "all", "--delay", "1.5s"])
                .is_ok()
        );
        assert!(
            Args::try_parse_from(["ncell", "game", game, "--count", "all", "--max-games", "2"])
                .is_err()
        );
    }
    assert!(
        validate_args(&Args::try_parse_from(["ncell", "game", "claim-task", "all,1"]).unwrap())
            .is_err()
    );
    let response = json!({"challenges":[
        {"id":22,"completed":1,"rewarded":0},
        {"id":23,"completed":true,"rewarded":true},
        {"id":26,"completed":false,"rewarded":false},
        {"id":"27","completed":true,"rewarded":false}
    ]});
    assert_eq!(claimable_ids(&response).unwrap(), [22, 27]);
    assert!(claimable_ids(&json!({"challenges":[]})).unwrap().is_empty());
    assert!(claimable_ids(&json!({})).is_err());
    let error = |status, message: &str| Error::CheckIn {
        status,
        message: message.into(),
        response: Box::new(json!({})),
    };
    assert!(insufficient_tokens(&error(200, "Insufficient tokens")));
    assert!(insufficient_tokens(&error(200, "Not enough points")));
    assert!(!insufficient_tokens(&error(401, "Not enough points")));
    assert!(!insufficient_tokens(&error(401, "Insufficient tokens")));
    assert!(!insufficient_tokens(&error(200, "Other error")));
}

#[test]
fn json_is_available_on_every_command() {
    for command in [
        vec!["login", "9812345678", "--verify"],
        vec!["status"],
        vec!["logout"],
        vec!["balance"],
        vec!["profile"],
        vec!["plans", "search", "Super", "--category", "66"],
        vec!["plans"],
        vec!["sms", "send-free", "9812345678", "Hello"],
        vec!["plans", "categories"],
        vec!["plans", "subscribe", "1225"],
        vec!["refresh"],
        vec!["game", "check-in"],
        vec!["game", "status"],
        vec!["game", "ranks"],
        vec!["game", "spin"],
        vec!["game", "scratch"],
        vec!["game", "math"],
        vec!["game", "claim-task", "1,2,3"],
    ] {
        let args = Args::try_parse_from(std::iter::once("ncell").chain(command).chain(["--json"]))
            .unwrap();
        assert!(args.json);
    }
    assert!(
        Args::try_parse_from(["ncell", "--json", "status"])
            .unwrap()
            .json
    );
}

#[test]
fn sort_arguments_accept_multiple_keys_and_reject_invalid_values() {
    let args =
        Args::try_parse_from(["ncell", "plans", "--sort", "validity:desc,price:asc"]).unwrap();
    let Command::Plans { options, .. } = args.command else {
        panic!("expected plans")
    };
    assert!(matches!(
        options.sort.as_slice(),
        [(Sort::Validity, true), (Sort::Price, false)]
    ));
    assert!(matches!(parse_sort("price"), Ok((Sort::Price, false))));
    for value in ["price:up", "name", "price:asc:desc", ""] {
        assert!(Args::try_parse_from(["ncell", "plans", "--sort", value]).is_err());
    }
}

#[test]
fn sorting_compares_numeric_prices_and_normalized_durations() {
    let hourly = json!({"productOfferingPrice":{"price":"35.50"},"techInfo":{"validity":"3","validityUom":"HOUR"}});
    let daily = json!({"productOfferingPrice":{"price":"100.00"},"techInfo":{"validity":"1","validityUom":"DAY"}});
    assert!(sort_key(&hourly, Sort::Price) < sort_key(&daily, Sort::Price));
    assert!(sort_key(&hourly, Sort::Validity) < sort_key(&daily, Sort::Validity));
    assert_eq!(sort_key(&json!({}), Sort::Price), f64::INFINITY);
}
