use super::*;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}
fn editor(input: &str) -> Editor {
    Editor {
        input: input.into(),
        cursor: input.len(),
        ..Editor::default()
    }
}
fn completion(editor: &Editor) -> Completion {
    editor.completion(&Completer::new(), &[])
}

#[test]
fn enter_accepts_partial_then_executes_an_exact_complete_command_once() {
    let mut editor = editor("bal");
    assert_eq!(
        editor.key(key(KeyCode::Enter), &completion(&editor)),
        Action::Continue
    );
    assert_eq!(editor.input, "balance ");
    assert_eq!(
        editor.key(key(KeyCode::Enter), &completion(&editor)),
        Action::Submit
    );
    let mut editor = self::editor("balance");
    assert_eq!(
        editor.key(key(KeyCode::Enter), &completion(&editor)),
        Action::Submit
    );
    let mut editor = self::editor("game");
    assert_eq!(
        editor.key(key(KeyCode::Enter), &completion(&editor)),
        Action::Continue
    );
    assert_eq!(editor.input, "game ");
    assert!(!completion(&editor).suggestions.is_empty());
}

#[test]
fn enter_submits_an_exact_sort_completion_instead_of_reaccepting_forever() {
    let mut editor = editor("plans --sort price:asc,validity:desc");
    assert_eq!(
        editor.key(key(KeyCode::Enter), &completion(&editor)),
        Action::Submit
    );
    assert!(parse_line(&editor.input).is_ok());
}

#[test]
fn clear_empties_the_screen_without_executing() {
    let root = Completer::new().complete("", 0, true, &[]);
    assert!(root.suggestions.iter().any(|item| item.value == "clear"));
    let mut editor = self::editor("clear");
    assert_eq!(
        editor.key(key(KeyCode::Enter), &completion(&editor)),
        Action::Clear
    );
    assert!(parse_line("clear").is_err());
}

#[test]
fn sort_acceptance_keeps_cursor_in_value_for_comma_continuation() {
    let mut editor = editor("plans --sort pri");
    editor.key(key(KeyCode::Tab), &completion(&editor));
    assert_eq!(editor.input, "plans --sort price:asc");
    editor.insert(",v");
    assert_eq!(
        completion(&editor).suggestions[0].value,
        "price:asc,validity:asc"
    );
}

#[test]
fn escape_only_dismisses_suggestions_and_keeps_input_and_required_hint() {
    let mut editor = editor("sms send-free ");
    editor.explicit = true;
    assert!(!completion(&editor).suggestions.is_empty());
    assert_eq!(
        editor.key(key(KeyCode::Esc), &completion(&editor)),
        Action::Continue
    );
    assert_eq!(editor.input, "sms send-free ");
    assert!(completion(&editor).suggestions.is_empty());
    assert!(completion(&editor).hint.unwrap().contains("<number>"));
}

#[test]
fn ctrl_space_null_and_arrow_keys_open_select_and_accept_suggestions() {
    let mut editor = Editor::default();
    editor.key(ctrl(KeyCode::Char(' ')), &completion(&editor));
    assert!(!completion(&editor).suggestions.is_empty());
    editor.key(key(KeyCode::Down), &completion(&editor));
    assert_eq!(editor.selected, 1);
    let value = completion(&editor).suggestions[1].value.clone();
    editor.key(key(KeyCode::Tab), &completion(&editor));
    assert!(editor.input.starts_with(&value));
    let mut editor = Editor::default();
    editor.key(key(KeyCode::Null), &completion(&editor));
    assert!(editor.explicit);
    for shortcut in [
        KeyEvent::new(
            KeyCode::Char('2'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ),
        KeyEvent::new(KeyCode::Char('\0'), KeyModifiers::CONTROL),
    ] {
        let mut editor = Editor::default();
        editor.key(shortcut, &completion(&editor));
        assert!(editor.explicit);
        assert!(!completion(&editor).suggestions.is_empty());
    }
}

#[test]
fn ctrl_a_replaces_or_deletes_the_whole_command_and_cursor_keys_preserve_it() {
    let mut editor = editor("game spin --count all");
    editor.key(ctrl(KeyCode::Char('a')), &completion(&editor));
    editor.key(key(KeyCode::Char('b')), &completion(&editor));
    assert_eq!(editor.input, "b");
    editor.key(ctrl(KeyCode::Char('a')), &completion(&editor));
    editor.key(key(KeyCode::Left), &completion(&editor));
    assert_eq!(editor.input, "b");
    assert_eq!(editor.cursor, 0);
    editor.key(ctrl(KeyCode::Char('a')), &completion(&editor));
    editor.key(key(KeyCode::Backspace), &completion(&editor));
    assert!(editor.input.is_empty());
}

#[test]
fn ctrl_backspace_and_unicode_editing_preserve_utf8_boundaries() {
    let mut editor = editor("sms send-free 9812345678 \"नमस्ते 👋\"");
    for _ in 0..4 {
        editor.key(key(KeyCode::Left), &completion(&editor));
    }
    editor.key(key(KeyCode::Backspace), &completion(&editor));
    editor.key(key(KeyCode::Delete), &completion(&editor));
    assert!(editor.input.is_char_boundary(editor.cursor));
    let mut editor = self::editor("game spin --count all");
    editor.key(ctrl(KeyCode::Backspace), &completion(&editor));
    assert_eq!(editor.input, "game spin --count ");
    editor.key(ctrl(KeyCode::Char('w')), &completion(&editor));
    assert_eq!(editor.input, "game spin ");
}

#[test]
fn history_keeps_draft_and_ignores_key_release_events() {
    let mut editor = editor("balance");
    editor.submitted();
    editor.insert("sms ");
    editor.dismissed = true;
    editor.key(key(KeyCode::Up), &completion(&editor));
    assert_eq!(editor.input, "balance");
    editor.key(key(KeyCode::Down), &completion(&editor));
    assert_eq!(editor.input, "sms ");
    editor.key(
        KeyEvent::new_with_kind(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ),
        &completion(&editor),
    );
    assert_eq!(editor.input, "sms ");
}

#[test]
fn parsing_uses_real_clap_and_shell_free_script_tokenizer() {
    assert!(parse_line("sms send-free 9812345678 \"नमस्ते 👋\"").is_ok());
    assert!(parse_line("ncell game spin --count all").is_ok());
    for command in [
        "status; logout",
        "status | more",
        "status $(whoami)",
        "interactive",
        "game claim-task 1 all",
    ] {
        assert!(
            matches!(parse_line(command), Err(ParseError::Message(_))),
            "{command}"
        );
    }
    assert!(matches!(
        parse_line("status --help"),
        Err(ParseError::Display(_))
    ));
    assert!(matches!(
        parse_line("--version"),
        Err(ParseError::Display(_))
    ));
}

#[test]
fn unicode_viewport_and_dropdown_fit_terminal_cells_and_align_descriptions() {
    let editor = editor("sms send-free 9812345678 \"नमस्ते 👋\"");
    let (offset, displayed) = viewport(&editor.input, editor.cursor, 12);
    assert!(editor.input.is_char_boundary(offset));
    assert!(width(&displayed) <= 12);
    let result = Completer::new().complete("", 0, true, &[]);
    for columns in [12, 20, 40, 80] {
        let rows = dropdown(&result, 4, "", 0, columns, 8);
        assert!(rows.len() <= 8);
        for row in rows {
            assert!(
                width(&strip_ansi(&row)) <= columns,
                "{columns}: {}",
                strip_ansi(&row)
            );
        }
    }
    let result = Completer::new().complete("game sp", 7, false, &[]);
    let rows = dropdown(&result, 0, "game sp", 0, 79, 24);
    assert!(strip_ansi(&rows[0]).starts_with(&" ".repeat(9)));
}

fn strip_ansi(input: &str) -> String {
    let mut output = String::new();
    let mut escape = false;
    for ch in input.chars() {
        if ch == '\x1b' {
            escape = true;
        } else if escape {
            if ch == 'm' {
                escape = false;
            }
        } else {
            output.push(ch);
        }
    }
    output
}

#[test]
fn production_banner_has_single_line_shortcuts_and_no_preview_status() {
    let banner = strip_ansi(&welcome(80));
    assert!(banner.contains("interactive · live commands"));
    assert!(
        banner
            .lines()
            .any(|line| line.contains("Ctrl+Space suggest   ·   Tab / Enter accept"))
    );
    assert!(!banner.contains("preview"));
    assert!(!banner.contains("Esc"));
    assert!(!banner.contains("Ln "));
    for columns in [12, 20, 40, 80] {
        for line in strip_ansi(&welcome(columns)).lines() {
            assert!(width(line) <= columns, "{columns}: {line}");
        }
    }
}

#[test]
fn cache_invalidation_discards_values_failures_and_inflight_receiver() {
    let (sender, receiver) = mpsc::channel();
    let mut cache = CategoryCache {
        pending: Some(receiver),
        ..CategoryCache::default()
    };
    sender.send(CacheEvent::Finished(Ok(vec![]))).unwrap();
    cache.poll();
    assert!(cache.loaded.is_some());
    cache.invalidate();
    assert!(cache.loaded.is_none());
    assert!(cache.values.is_empty());
    assert!(cache.error.is_none());
}

#[test]
fn acceptance_only_reopens_subcommands_not_arguments_or_option_values() {
    for input in ["sms send-fr", "game spin --co", "log"] {
        let mut editor = self::editor(input);
        editor.key(key(KeyCode::Tab), &completion(&editor));
        let result = completion(&editor);
        assert!(result.suggestions.is_empty(), "{}", editor.input);
        assert!(result.hint.is_some(), "{}", editor.input);
        assert!(!editor.explicit);
    }
}

#[test]
fn category_loading_and_results_keep_the_same_panel_geometry() {
    let input = "plans --category ";
    let completer = Completer::new();
    let mut loading = completer.complete(input, input.len(), true, &[]);
    loading.category_status = Some("Loading categories…".into());
    let tags = super::completion::categories(&serde_json::json!({"tags":[
        {"id":50,"name":"SMS","categories":[]},
        {"id":66,"name":"Data and voice packs with a longer description","categories":[]}
    ]}))
    .unwrap();
    let loaded = completer.complete(input, input.len(), true, &tags);
    for (columns, available) in [(80, 12), (40, 8), (24, 5)] {
        let before = dropdown(&loading, 0, input, 0, columns, available);
        let after = dropdown(&loaded, 1, input, 0, columns, available);
        assert_eq!(before.len(), after.len());
        assert_eq!(before.first(), after.first());
        assert_eq!(
            width(&strip_ansi(before.last().unwrap())),
            width(&strip_ansi(after.last().unwrap()))
        );
        assert_eq!(
            before
                .iter()
                .map(|row| width(&strip_ansi(row)))
                .collect::<Vec<_>>(),
            after
                .iter()
                .map(|row| width(&strip_ansi(row)))
                .collect::<Vec<_>>()
        );
        assert!(before.iter().all(|row| width(&strip_ansi(row)) <= columns));
    }
    assert!(
        dropdown(&loading, 0, input, 0, 80, 12)
            .iter()
            .any(|row| row.contains("Loading categories…"))
    );
}

#[test]
fn category_footer_reports_total_and_scrolled_range() {
    let input = "plans --category ";
    let tags = super::completion::categories(&serde_json::json!({"tags": (1..=20).map(|id|serde_json::json!({"id":id,"name":"Category"})).collect::<Vec<_>>()})).unwrap();
    let result = Completer::new().complete(input, input.len(), true, &tags);
    assert!(
        strip_ansi(dropdown(&result, 0, input, 0, 80, 12).last().unwrap()).contains("1–10 of 20")
    );
    let last = dropdown(&result, 19, input, 0, 80, 12);
    assert!(strip_ansi(last.last().unwrap()).contains("11–20 of 20"));
    assert!(last.iter().any(|row| row.contains("20")));
}

#[test]
fn pack_cache_expires_after_five_minutes_and_acceptance_routes_sms_catalog() {
    let cache = CategoryCache {
        loaded: Some(Instant::now() - Duration::from_secs(301)),
        ..CategoryCache::default()
    };
    assert!(!cache.fresh(true));
    assert!(cache.fresh(false));
    let input = "plans subscribe 984";
    let packs = super::completion::unique_packs(
        &serde_json::json!({"tags":[{"id":50,"categories":[{"name":"GSM_SMS"}]}]}),
        |_| Ok(serde_json::json!({"availablePackages":[{"id":"984","ncellName":"SMS pack"}]})),
    )
    .unwrap();
    let mut editor = self::editor(input);
    let mut choices = completion(&editor);
    choices.suggestions = super::completion::pack_suggestions(&packs, "984", None);
    assert_eq!(editor.key(key(KeyCode::Enter), &choices), Action::Continue);
    assert_eq!(editor.input, "plans subscribe 984 --category 50 ");
    let parsed =
        parse_line(&editor.input).unwrap_or_else(|_| panic!("Invalid accepted subscription"));
    assert!(matches!(
        parsed.command,
        Command::Plans {
            action: Some(super::super::PlanCommand::Subscribe { category: 50, .. }),
            ..
        }
    ));
}
