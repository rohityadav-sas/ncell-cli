use super::*;
use serde_json::json;

/// Test helper that fetches catalogs and merges overlapping memberships by pack ID.
fn unique_packs(
    response: &serde_json::Value,
    mut fetch: impl FnMut(u64) -> std::result::Result<serde_json::Value, String>,
) -> std::result::Result<Vec<Suggestion>, String> {
    let fetched = supported_category_ids(response)?
        .into_iter()
        .map(|category| fetch(category).map(|response| (category, response)))
        .collect::<Result<Vec<_>, _>>()?;
    merge_fetched_packs(fetched)
}

fn options(input: &str, explicit: bool) -> Completion {
    Completer::new().complete(input, input.len(), explicit, &[])
}
fn values(completion: &Completion) -> Vec<&str> {
    completion
        .suggestions
        .iter()
        .map(|item| item.value.as_str())
        .collect()
}

#[test]
fn root_is_hidden_until_typing_or_explicit_shortcut_and_schema_stays_authoritative() {
    assert!(options("", false).suggestions.is_empty());
    assert_eq!(values(&options("ga", false)), ["game"]);
    let all = options("", true);
    assert!(values(&all).contains(&"login"));
    assert!(values(&all).contains(&"exit"));
    assert!(!values(&all).contains(&"interactive"));
    for child in Completer::new().root.get_subcommands() {
        if child.get_name() != "help" {
            assert!(
                values(&all).contains(&child.get_name()),
                "{}",
                child.get_name()
            );
        }
    }
}

#[test]
fn suggestions_follow_subcommands_and_global_options_without_invalid_reordering() {
    assert_eq!(values(&options("game sp", false)), ["spin"]);
    assert_eq!(values(&options("game spin --co", false)), ["--count"]);
    assert_eq!(values(&options("--json game sp", false)), ["spin"]);
    assert_eq!(values(&options("account --json a", false)), ["add"]);
    assert!(
        options("account --json --help add ", true)
            .suggestions
            .is_empty()
    );
    assert!(
        options("plans --category 66 s", false)
            .suggestions
            .is_empty()
    );
    assert!(options("-- game sp", false).suggestions.is_empty());
    assert!(options("invalid game sp", false).suggestions.is_empty());
}

#[test]
fn flags_prioritize_action_options_and_hide_flags_already_used() {
    let result = options("plans --", false);
    assert_eq!(
        values(&result),
        ["--category", "--sort", "--json", "--help"]
    );
    let result = options("login 9812345678 --verify --", false);
    assert!(!values(&result).contains(&"--verify"));
    assert!(values(&result).contains(&"--help"));
    assert!(
        options("login 9812345678 --verify --verify --", false)
            .suggestions
            .is_empty()
    );
}

#[test]
fn pending_options_offer_values_and_preserve_equals_form() {
    assert_eq!(values(&options("game spin --count ", true)), ["all"]);
    assert_eq!(
        values(&options("game spin --count=a", false)),
        ["--count=all"]
    );
    assert_eq!(
        values(&options("plans --sort p", false)),
        ["price:asc", "price:desc"]
    );
    assert_eq!(
        values(&options("plans --sort=validity:", false)),
        ["--sort=validity:asc", "--sort=validity:desc"]
    );
    assert!(
        options("game spin --count oops --", false)
            .suggestions
            .is_empty()
    );
}

#[test]
fn sorting_completes_comma_values_and_keeps_the_discovery_hint() {
    let result = options("plans --sort price:asc,v", false);
    assert_eq!(
        values(&result),
        ["price:asc,validity:asc", "price:asc,validity:desc"]
    );
    assert_eq!(
        result.hint.as_deref(),
        Some("add , for another field (price:asc,validity:asc)")
    );
    assert!(result.sort_hint);
    assert_eq!(
        values(&options("plans --sort price:asc,validity:desc", false)),
        ["price:asc,validity:desc"]
    );
}

#[test]
fn duplicate_nonrepeatable_inline_values_are_not_suggested() {
    assert!(
        options("game spin --count 1 --count=a", false)
            .suggestions
            .is_empty()
    );
}

#[test]
fn required_hints_advance_after_complete_numbers_and_closed_quotes() {
    assert!(
        options("sms send-free ", false)
            .hint
            .unwrap()
            .starts_with("<number> <text>")
    );
    for number in ["9812345678", "+9779812345678"] {
        assert!(
            options(&format!("sms send-free {number}"), false)
                .hint
                .unwrap()
                .starts_with("<text>")
        );
        assert!(
            options(&format!("sms send-free {number} \""), false)
                .hint
                .unwrap()
                .starts_with("<text>")
        );
        assert!(
            options(&format!("sms send-free {number} \"hello\""), false)
                .hint
                .is_none()
        );
    }
    assert!(options("login 9812345678", false).hint.is_none());
}

#[test]
fn variadic_task_ids_do_not_offer_all_after_an_id_or_more_after_all() {
    assert_eq!(values(&options("game claim-task a", false)), ["all"]);
    assert!(options("game claim-task 1 a", false).suggestions.is_empty());
    assert!(
        options("game claim-task all a", false)
            .suggestions
            .is_empty()
    );
}

#[test]
fn categories_come_from_api_tags_and_include_names_and_group_descriptions() {
    let response = json!({"tags":[
        {"id":50,"name":"Data","categories":[{"displayName":"GSM DATA"}]},
        {"id":"60","name":"Voice","categories":[{"displayName":"GSM VOICE"}]},
        {"id":"invalid","name":"Skip"}
    ]});
    let categories = categories(&response).unwrap();
    assert_eq!(categories.len(), 2);
    let result = Completer::new().complete("plans --category ", 17, true, &categories);
    assert_eq!(values(&result), ["50", "60"]);
    assert_eq!(result.suggestions[0].description, "Data · GSM DATA");
    assert!(result.needs_categories);
    assert!(super::categories(&json!({})).is_err());
    assert!(!options("status --category ", false).needs_categories);
}

#[test]
fn cursor_completion_replaces_whole_current_word_including_suffix() {
    let input = "game spxxxxx --count 1";
    let result = Completer::new().complete(input, 7, false, &[]);
    assert_eq!(values(&result), ["spin"]);
    assert_eq!(&input[result.replace], "spxxxxx");
}

#[test]
fn file_completions_reflect_real_edit_delete_and_run_scopes() {
    let base = std::env::temp_dir().join(format!("ncell-completion-{}", uuid::Uuid::new_v4()));
    let library = base.join("library");
    let cwd = base.join("cwd");
    std::fs::create_dir_all(&library).unwrap();
    std::fs::create_dir_all(cwd.join("nested")).unwrap();
    std::fs::create_dir_all(cwd.join("nested/folder name")).unwrap();
    std::fs::write(library.join("daily.ncell"), "status").unwrap();
    std::fs::write(cwd.join("local script.ncell"), "status").unwrap();
    std::fs::write(cwd.join("readme.txt"), "ignored").unwrap();
    std::fs::write(cwd.join("nested/inside.ncell"), "status").unwrap();
    assert_eq!(
        file_suggestions("", false, &cwd, Some(&library))
            .iter()
            .map(|item| item.value.as_str())
            .collect::<Vec<_>>(),
        ["daily.ncell"]
    );
    assert_eq!(
        file_suggestions("", true, &cwd, Some(&library))
            .iter()
            .map(|item| item.value.as_str())
            .collect::<Vec<_>>(),
        ["\"local script.ncell\"", "daily.ncell"]
    );
    assert!(
        file_suggestions("nested/", true, &cwd, Some(&library))
            .iter()
            .any(|item| item.value == "nested/inside.ncell")
    );
    let directory = file_suggestions("nested/folder", true, &cwd, Some(&library)).remove(0);
    assert!(!directory.append_space);
    assert!(directory.value.starts_with('"'));
    assert!(!directory.value.ends_with('"'));
    assert!(file_suggestions("nested/", false, &cwd, Some(&library)).is_empty());
    std::fs::remove_dir_all(&base).unwrap();
}

#[test]
fn required_values_precede_options_until_every_argument_is_complete() {
    for input in [
        "sms send-free ",
        "sms send-free 9829493372",
        "sms send-free 9829493372 ",
        "sms send-free 9829493372 \"hello",
        "login ",
    ] {
        let result = options(input, true);
        assert!(!values(&result).contains(&"--json"), "{input}");
    }
    assert_eq!(values(&options("sms send-free ", true)), ["--help"]);
    assert!(values(&options("sms send-free 9829493372 \"hello\" ", true)).contains(&"--json"));
    assert!(values(&options("login 9829493372 ", true)).contains(&"--verify"));
}

#[test]
fn argument_and_option_slots_stay_quiet_until_typing_or_explicit_request() {
    for input in [
        "sms send-free ",
        "login ",
        "script run ",
        "game spin --count ",
        "plans --sort ",
        "plans --category ",
        "balance ",
    ] {
        let result = options(input, false);
        assert!(result.suggestions.is_empty(), "{input}");
        assert!(!result.needs_categories, "{input}");
    }
    assert!(options("sms send-free ", false).hint.is_some());
    assert_eq!(values(&options("sms send-free ", true)), ["--help"]);
    assert_eq!(values(&options("game spin --count a", false)), ["all"]);
}

#[test]
fn subscribe_catalog_merges_pack_ids_and_preserves_category_membership() {
    let response = json!({"tags":[
        {"id":50,"categories":[{"name":"GSM_SMS"}]},
        {"id":61,"categories":[{"name":"GSM_VOICE"}]},
        {"id":66,"categories":[{"name":"GSM_DATA"}]},
        {"id":29,"categories":[{"name":"GSM_ROAMING"}]}
    ]});
    let mut fetched = vec![];
    let packs = unique_packs(&response, |category| {
        fetched.push(category);
        Ok(json!({"availablePackages": if category == 50 { vec![json!({"id":"984","ncellName":"SMS pack"})] } else { vec![json!({"id":"856","ncellName":"Voice pack"})] }}))
    }).unwrap();
    assert_eq!(fetched, [50, 61, 66]);
    assert_eq!(packs.len(), 2);
    let sms = pack_suggestions(&packs, "SMS", None);
    assert_eq!(sms[0].value, "984");
    assert_eq!(sms[0].subscription_category, Some(50));
    assert_eq!(
        pack_suggestions(&packs, "856", None)[0].subscription_category,
        None
    );
    assert_eq!(pack_suggestions(&packs, "", Some(50)).len(), 1);
    assert!(pack_suggestions(&packs, "856", Some(50)).is_empty());
    assert!(unique_packs(&response, |_| Err("fetch failed".into())).is_err());
    let request = options("plans subscribe ", true);
    assert!(request.needs_packs);
    assert!(!options("plans subscribe ", false).needs_packs);
}
