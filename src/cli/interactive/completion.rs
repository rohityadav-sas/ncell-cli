use super::super::{Args, Sort, script};
use clap::{Arg, Command, CommandFactory, ValueEnum};
use std::{collections::HashSet, ops::Range, path::Path};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Suggestion {
    pub value: String,
    pub description: String,
    pub kind: char,
    pub continue_input: bool,
    pub open_subcommands: bool,
    pub append_space: bool,
    pub subscription_category: Option<u64>,
    pub pack_categories: Vec<u64>,
}
impl Suggestion {
    fn new(value: impl Into<String>, description: impl Into<String>, kind: char) -> Self {
        Self {
            value: value.into(),
            description: description.into(),
            kind,
            continue_input: false,
            open_subcommands: false,
            append_space: true,
            subscription_category: None,
            pack_categories: Vec::new(),
        }
    }
}

#[derive(Clone, Default, Debug)]
pub(super) struct Completion {
    pub suggestions: Vec<Suggestion>,
    pub replace: Range<usize>,
    pub hint: Option<String>,
    pub sort_hint: bool,
    pub needs_categories: bool,
    pub needs_packs: bool,
    pub pack_query: String,
    pub pack_category: Option<u64>,
    pub category_status: Option<String>,
}

#[derive(Clone, Debug)]
struct Token {
    value: String,
    start: usize,
    end: usize,
    quoted: bool,
    closed: bool,
}

// Match script::words quoting without requiring the word under construction to be closed.
fn tokens(input: &str) -> Vec<Token> {
    let mut output = Vec::new();
    let mut value = String::new();
    let mut start = None;
    let mut quote = None;
    let mut quoted = false;
    for (index, ch) in input.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                value.push(ch);
            }
        } else if ch == '\'' || ch == '"' {
            start.get_or_insert(index);
            quote = Some(ch);
            quoted = true;
        } else if ch.is_whitespace() {
            if let Some(start) = start.take() {
                output.push(Token {
                    value: std::mem::take(&mut value),
                    start,
                    end: index,
                    quoted,
                    closed: true,
                });
                quoted = false;
            }
        } else {
            start.get_or_insert(index);
            value.push(ch);
        }
    }
    if let Some(start) = start {
        output.push(Token {
            value,
            start,
            end: input.len(),
            quoted,
            closed: quote.is_none(),
        });
    }
    output
}

pub(super) struct Completer {
    root: Command,
}
impl Completer {
    pub fn new() -> Self {
        let mut root = Args::command();
        root.build();
        Self { root }
    }

    pub fn complete(
        &self,
        input: &str,
        cursor: usize,
        explicit: bool,
        categories: &[Suggestion],
    ) -> Completion {
        let before = &input[..cursor];
        let mut words = tokens(before);
        let active = if words.last().is_some_and(|word| word.end == cursor) {
            words.pop()
        } else {
            None
        };
        let prefix = active.as_ref().map_or("", |word| word.value.as_str());
        let start = active.as_ref().map_or(cursor, |word| word.start);
        let end = tokens(input)
            .into_iter()
            .find(|word| word.start == start)
            .map_or(cursor, |word| word.end);
        let mut result = Completion {
            replace: start..end,
            ..Completion::default()
        };
        if words.first().is_some_and(|word| word.value == "ncell") {
            words.remove(0);
        }
        let mut command = &self.root;
        let mut path = Vec::new();
        let mut used = HashSet::new();
        let mut positional = 0usize;
        let mut expecting: Option<&Arg> = None;
        let mut local_options = false;
        let mut end_options = false;
        let mut variadic_used = false;
        for word in &words {
            if let Some(arg) = expecting.take() {
                if !word.closed || !valid_value(arg, &word.value) {
                    return result;
                }
                continue;
            }
            if word.value == "--" {
                end_options = true;
                continue;
            }
            if !end_options && word.value.starts_with('-') {
                let (flag, inline) = word
                    .value
                    .split_once('=')
                    .map_or((word.value.as_str(), None), |(a, b)| (a, Some(b)));
                let Some(arg) = find_flag(command, flag) else {
                    return result;
                };
                if used.contains(arg.get_id().as_str()) && !repeatable(arg) {
                    return result;
                }
                used.insert(arg.get_id().as_str().to_owned());
                if !arg.is_global_set() {
                    local_options = true;
                }
                if matches!(
                    arg.get_action(),
                    clap::ArgAction::Help | clap::ArgAction::Version
                ) {
                    return result;
                }
                if arg.get_action().takes_values() {
                    if let Some(value) = inline {
                        if !valid_value(arg, value) {
                            return result;
                        }
                    } else {
                        expecting = Some(arg);
                    }
                } else if inline.is_some() {
                    return result;
                }
                continue;
            }
            if !end_options
                && !local_options
                && positional == 0
                && let Some(child) = command.find_subcommand(&word.value)
            {
                command = child;
                path.push(child.get_name());
                local_options = false;
                continue;
            }
            let args = positionals(command);
            let Some(arg) = args.get(positional).copied() else {
                return result;
            };
            if !word.closed || !valid_value(arg, &word.value) {
                return result;
            }
            if !repeatable(arg) || (arg.get_id() == "ids" && word.value == "all") {
                positional += 1;
            } else {
                variadic_used = true;
            }
        }

        let visible = explicit || !prefix.is_empty();
        let mut inline_flag = None;
        if expecting.is_none()
            && !end_options
            && prefix.starts_with("--")
            && let Some((flag, value)) = prefix.split_once('=')
            && let Some(arg) = find_flag(command, flag)
            && arg.get_action().takes_values()
            && (!used.contains(arg.get_id().as_str()) || repeatable(arg))
        {
            expecting = Some(arg);
            inline_flag = Some((flag, value));
        }
        if let Some(arg) = expecting {
            let value = inline_flag.map_or(prefix, |(_, value)| value);
            result.needs_categories = (explicit || !value.is_empty()) && arg.get_id() == "category";
            result.sort_hint = arg.get_id() == "sort";
            result.hint = Some(value_hint(arg));
            if explicit || !value.is_empty() {
                result.suggestions = value_suggestions(arg, value, &path, categories);
                if let Some((flag, _)) = inline_flag {
                    for suggestion in &mut result.suggestions {
                        suggestion.value = format!("{flag}={}", suggestion.value);
                    }
                }
            }
            return result;
        }

        let positional_args = positionals(command);
        let current_arg = positional_args.get(positional).copied();
        let can_subcommand = !end_options
            && positional == 0
            && (!local_options || !command.is_args_conflicts_with_subcommands_set());
        let mut missing_required = false;
        if let Some(arg) = current_arg {
            let complete_value = active.as_ref().is_some_and(|word| {
                word.closed
                    && ((arg.get_id() == "number"
                        && ncell_cli::auth::normalize_number(&word.value).is_ok())
                        || (word.quoted && !word.value.is_empty()))
            });
            let remaining = positional_args
                .iter()
                .skip(positional + usize::from(complete_value))
                .filter(|arg| arg.is_required_set())
                .map(|arg| format!("<{}>", arg.get_id()))
                .collect::<Vec<_>>();
            missing_required = !remaining.is_empty();
            if missing_required {
                let hint_arg = positional_args
                    .get(positional + usize::from(complete_value))
                    .unwrap_or(&arg);
                result.hint = Some(format!(
                    "{}  ·  {}",
                    remaining.join(" "),
                    argument_description(hint_arg)
                ));
            }
            if visible && !prefix.starts_with('-') && !complete_value {
                result
                    .suggestions
                    .extend(value_suggestions(arg, prefix, &path, categories));
                if variadic_used && arg.get_id() == "ids" {
                    result.suggestions.retain(|item| item.value != "all");
                }
            }
        } else if command.is_subcommand_required_set() && !path.is_empty() && prefix.is_empty() {
            result.hint = Some("Choose a subcommand".into());
        }
        if current_arg.is_some_and(|arg| arg.get_id() == "id")
            && path == ["plans", "subscribe"]
            && visible
            && !prefix.starts_with('-')
        {
            result.needs_packs = true;
            result.pack_query = prefix.into();
            let all = tokens(input);
            result.pack_category = all.iter().enumerate().find_map(|(index, word)| {
                if word.value == "--category" {
                    all.get(index + 1).and_then(|next| next.value.parse().ok())
                } else {
                    word.value
                        .strip_prefix("--category=")
                        .and_then(|value| value.parse().ok())
                }
            });
        }
        if visible {
            if can_subcommand && !prefix.starts_with('-') {
                result.suggestions.extend(
                    command
                        .get_subcommands()
                        .filter(|child| {
                            !child.is_hide_set()
                                && !matches!(child.get_name(), "help" | "interactive")
                                && child.get_name().starts_with(prefix)
                        })
                        .map(|child| {
                            let mut item = Suggestion::new(
                                child.get_name(),
                                child
                                    .get_about()
                                    .map(ToString::to_string)
                                    .unwrap_or_default(),
                                '›',
                            );
                            item.open_subcommands = child.is_subcommand_required_set();
                            item.continue_input = child.is_subcommand_required_set()
                                || positionals(child).iter().any(|arg| arg.is_required_set());
                            item
                        }),
                );
                if path.is_empty() && "exit".starts_with(prefix) {
                    result.suggestions.push(Suggestion::new(
                        "exit",
                        "Exit the interactive shell",
                        '›',
                    ));
                }
            }
            if !end_options && (prefix.is_empty() || prefix.starts_with('-')) {
                let mut flags = command
                    .get_arguments()
                    .filter(|arg| {
                        !arg.is_hide_set()
                            && (!missing_required || arg.get_long() == Some("help"))
                            && arg.get_long().is_some()
                            && (!used.contains(arg.get_id().as_str()) || repeatable(arg))
                    })
                    .filter(|arg| format!("--{}", arg.get_long().unwrap()).starts_with(prefix))
                    .collect::<Vec<_>>();
                flags.sort_by_key(|arg| match arg.get_id().as_str() {
                    "json" => 1,
                    "help" | "version" => 2,
                    _ => 0,
                });
                result.suggestions.extend(flags.into_iter().map(|arg| {
                    let mut item = Suggestion::new(
                        format!("--{}", arg.get_long().unwrap()),
                        arg.get_help().map(ToString::to_string).unwrap_or_default(),
                        '◇',
                    );
                    item.continue_input = arg.get_action().takes_values();
                    item
                }));
            }
        }
        result
    }
}

fn positionals(command: &Command) -> Vec<&Arg> {
    let mut args = command.get_positionals().collect::<Vec<_>>();
    args.sort_by_key(|arg| arg.get_index());
    args
}
fn find_flag<'a>(command: &'a Command, flag: &str) -> Option<&'a Arg> {
    command.get_arguments().find(|arg| {
        arg.get_long()
            .is_some_and(|long| flag == format!("--{long}"))
            || arg
                .get_short()
                .is_some_and(|short| flag == format!("-{short}"))
    })
}
fn repeatable(arg: &Arg) -> bool {
    matches!(
        arg.get_action(),
        clap::ArgAction::Append | clap::ArgAction::Count
    ) || (arg.is_positional()
        && arg
            .get_num_args()
            .is_some_and(|range| range.max_values() > 1))
}
fn valid_value(arg: &Arg, value: &str) -> bool {
    match arg.get_id().as_str() {
        "number" => ncell_cli::auth::normalize_number(value).is_ok(),
        "category" => value.parse::<u64>().is_ok(),
        "sort" => value
            .split(',')
            .all(|part| super::super::parse_sort(part).is_ok()),
        "count" => super::super::parse_count(value).is_ok(),
        "delay" => super::super::parse_delay(value).is_ok(),
        "ids" => value
            .split(',')
            .all(|part| super::super::parse_task_id(part).is_ok()),
        _ => !value.is_empty(),
    }
}
fn argument_description(arg: &Arg) -> String {
    arg.get_help()
        .map(ToString::to_string)
        .unwrap_or_else(|| match arg.get_id().as_str() {
            "number" => "Ncell number · Required".into(),
            "text" => "SMS message; use double quotes for spaces · Required".into(),
            "file" => "Script name or path; use double quotes for spaces · Required".into(),
            "keyword" => "Search query; use double quotes for spaces · Required".into(),
            "id" => "Plan ID from the plans list · Required".into(),
            "ids" => "Task IDs separated by commas · Required".into(),
            _ => "Required".into(),
        })
}
fn value_hint(arg: &Arg) -> String {
    match arg.get_id().as_str() {
        "sort" => "add , for another field (price:asc,validity:asc)".into(),
        "category" => "<category> Choose a category".into(),
        "count" => "<count> Positive number, or all".into(),
        "delay" => "<delay> Duration such as 1.5s, 2m, 1h".into(),
        _ => format!("<{}>  ·  {}", arg.get_id(), argument_description(arg)),
    }
}
fn value_suggestions(
    arg: &Arg,
    prefix: &str,
    path: &[&str],
    categories: &[Suggestion],
) -> Vec<Suggestion> {
    match arg.get_id().as_str() {
        "category" => categories
            .iter()
            .filter(|item| item.value.starts_with(prefix))
            .cloned()
            .collect(),
        "sort" => {
            let split = prefix.rfind(',').map_or(0, |index| index + 1);
            let (head, tail) = prefix.split_at(split);
            Sort::value_variants()
                .iter()
                .flat_map(|field| {
                    let name = field.to_possible_value().unwrap().get_name().to_owned();
                    ["asc", "desc"].into_iter().filter_map(move |direction| {
                        let value = format!("{name}:{direction}");
                        value.starts_with(tail).then(|| {
                            let mut item = Suggestion::new(
                                format!("{head}{value}"),
                                if direction == "asc" {
                                    "Ascending"
                                } else {
                                    "Descending"
                                },
                                '›',
                            );
                            item.append_space = false;
                            item
                        })
                    })
                })
                .collect()
        }
        "file" if path.first() == Some(&"script") && path.get(1) != Some(&"create") => {
            file_suggestions(
                prefix,
                path.get(1) == Some(&"run"),
                &std::env::current_dir().unwrap_or_default(),
                script::script_directory().ok().as_deref(),
            )
        }
        "count" | "ids" if "all".starts_with(prefix) => {
            vec![Suggestion::new(
                "all",
                if arg.get_id() == "count" {
                    "Use all game tokens"
                } else {
                    "Claim all completed tasks"
                },
                '›',
            )]
        }
        _ => arg
            .get_possible_values()
            .into_iter()
            .filter(|value| !value.is_hide_set() && value.get_name().starts_with(prefix))
            .map(|value| {
                Suggestion::new(
                    value.get_name(),
                    value
                        .get_help()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                    '›',
                )
            })
            .collect(),
    }
}

pub(super) fn file_suggestions(
    prefix: &str,
    paths_allowed: bool,
    cwd: &Path,
    library: Option<&Path>,
) -> Vec<Suggestion> {
    let split = prefix.rfind(['/', '\\']).map_or(0, |index| index + 1);
    if !paths_allowed && (split > 0 || prefix.contains(':')) {
        return Vec::new();
    }
    let (directory, name) = prefix.split_at(split);
    let roots = if split > 0 {
        vec![cwd.join(directory)]
    } else {
        library
            .into_iter()
            .map(Path::to_path_buf)
            .chain(paths_allowed.then(|| cwd.to_path_buf()))
            .collect()
    };
    let mut found = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let label = entry.file_name().to_string_lossy().into_owned();
            if label.chars().any(char::is_control) || label.contains(['\'', '"']) {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if !label.to_lowercase().starts_with(&name.to_lowercase())
                || (label.starts_with('.') && !name.starts_with('.'))
            {
                continue;
            }
            if kind.is_dir() && (split == 0 || !paths_allowed) {
                continue;
            }
            if !kind.is_dir() && (!kind.is_file() || !label.to_lowercase().ends_with(".ncell")) {
                continue;
            }
            let value = format!(
                "{directory}{label}{}",
                if kind.is_dir() {
                    std::path::MAIN_SEPARATOR_STR
                } else {
                    ""
                }
            );
            let value = if value.contains(char::is_whitespace)
                || value.contains(['|', '&', ';', '<', '>', '`', '$'])
            {
                if kind.is_dir() {
                    format!("\"{value}")
                } else {
                    format!("\"{value}\"")
                }
            } else {
                value
            };
            let mut item = Suggestion::new(
                value,
                if kind.is_dir() {
                    "Directory"
                } else {
                    "Saved script"
                },
                if kind.is_dir() { '›' } else { '▤' },
            );
            item.continue_input = kind.is_dir();
            item.append_space = !kind.is_dir();
            found.push(item);
        }
    }
    found.sort_by_key(|item| item.value.to_lowercase());
    found.dedup_by(|a, b| a.value.eq_ignore_ascii_case(&b.value));
    found
}

pub(super) fn categories(response: &serde_json::Value) -> Result<Vec<Suggestion>, String> {
    let tags = response["tags"]
        .as_array()
        .ok_or("Ncell returned invalid categories")?;
    Ok(tags
        .iter()
        .filter_map(|tag| {
            let id = tag["id"].as_u64().map(|id| id.to_string()).or_else(|| {
                tag["id"]
                    .as_str()
                    .filter(|id| id.parse::<u64>().is_ok())
                    .map(str::to_owned)
            })?;
            let name = tag["name"].as_str()?.trim();
            let group = tag["categories"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|category| category["displayName"].as_str())
                .collect::<Vec<_>>()
                .join(", ");
            Some(Suggestion::new(
                id,
                if group.is_empty() {
                    name.to_owned()
                } else {
                    format!("{name} · {group}")
                },
                '›',
            ))
        })
        .collect())
}

#[cfg(test)]
#[path = "../../../tests/unit/interactive_completion.rs"]
mod tests;

/// Test helper that fetches catalogs and merges overlapping memberships by pack ID.
#[cfg(test)]
pub(super) fn unique_packs(
    response: &serde_json::Value,
    mut fetch: impl FnMut(u64) -> Result<serde_json::Value, String>,
) -> Result<Vec<Suggestion>, String> {
    let fetched = supported_category_ids(response)?
        .into_iter()
        .map(|category| fetch(category).map(|response| (category, response)))
        .collect::<Result<Vec<_>, _>>()?;
    merge_fetched_packs(fetched)
}

pub(super) fn supported_category_ids(response: &serde_json::Value) -> Result<Vec<u64>, String> {
    let tags = response["tags"]
        .as_array()
        .ok_or("Invalid category response")?;
    tags.iter()
        .filter(|tag| {
            tag["categories"].as_array().is_some_and(|types| {
                types.iter().any(|kind| {
                    matches!(
                        kind["name"].as_str(),
                        Some("GSM_DATA" | "GSM_VOICE" | "GSM_SMS")
                    )
                })
            })
        })
        .map(|tag| {
            tag["id"]
                .as_u64()
                .ok_or_else(|| "Invalid category ID".to_owned())
        })
        .collect()
}

pub(super) fn merge_fetched_packs(
    fetched: Vec<(u64, serde_json::Value)>,
) -> Result<Vec<Suggestion>, String> {
    let mut packs = std::collections::BTreeMap::<u64, Suggestion>::new();
    for (category, result) in fetched {
        for pack in result["availablePackages"]
            .as_array()
            .ok_or("Invalid plans response")?
        {
            let id = pack["id"]
                .as_u64()
                .or_else(|| pack["id"].as_str().and_then(|id| id.parse().ok()))
                .ok_or("Invalid pack ID")?;
            if let Some(existing) = packs.get_mut(&id) {
                if !existing.pack_categories.contains(&category) {
                    existing.pack_categories.push(category);
                }
                continue;
            }
            let name = pack["displayInfo"]["displayName"]
                .as_str()
                .filter(|name| !name.is_empty())
                .or_else(|| pack["ncellName"].as_str())
                .or_else(|| pack["productName"].as_str())
                .unwrap_or("Pack");
            let price = pack["productOfferingPrice"]["price"].as_str().unwrap_or("");
            let price = price.strip_suffix(".00").unwrap_or(price);
            let mut item = Suggestion::new(
                id.to_string(),
                if price.is_empty() {
                    name.to_owned()
                } else {
                    format!("{name} · Rs. {price}")
                },
                '›',
            );
            item.pack_categories.push(category);
            packs.insert(id, item);
        }
    }
    Ok(packs.into_values().collect())
}

pub(super) fn pack_suggestions(
    packs: &[Suggestion],
    query: &str,
    category: Option<u64>,
) -> Vec<Suggestion> {
    packs
        .iter()
        .filter(|pack| {
            category.is_none_or(|id| pack.pack_categories.contains(&id))
                && (pack.value.starts_with(query)
                    || pack
                        .description
                        .to_lowercase()
                        .contains(&query.to_lowercase()))
        })
        .map(|pack| {
            let mut item = pack.clone();
            if category.is_none() {
                let chosen = if pack.pack_categories.contains(&66) {
                    66
                } else {
                    *pack.pack_categories.iter().min().unwrap_or(&66)
                };
                if chosen != 66 {
                    item.subscription_category = Some(chosen);
                    item.continue_input = true;
                }
            }
            item
        })
        .collect()
}
