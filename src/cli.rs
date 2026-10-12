use clap::{Parser, Subcommand};
use ncell_cli::{
    api::Client,
    auth::{AuthService, normalize_number},
    error::{Error, Result},
    session::{Account, KeyringStore, SessionStore},
};
use serde_json::{Value, json};
use std::io::{self, Write};

#[derive(Parser)]
#[command(
    name = "ncell-cli",
    bin_name = "ncell",
    version,
    about = "Ncell account CLI",
    disable_help_flag = true,
    disable_version_flag = true
)]
pub struct Args {
    /// Print help.
    #[arg(long, global = true, action = clap::ArgAction::Help)]
    pub help: Option<bool>,
    /// Print version.
    #[arg(long, action = clap::ArgAction::Version)]
    pub version: Option<bool>,
    /// Print structured JSON output.
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Request an SMS code and log in. Use --verify to resume a pending login.
    Login {
        number: String,
        #[arg(long)]
        verify: bool,
    },
    /// Show local session status (does not check token validity).
    Status,
    /// Remove the local session, including the device ID. Does not revoke server tokens.
    Logout,
    /// Query credit, data, voice and SMS balances.
    Balance,
    /// Query the subscriber profile; use --json for all details.
    Profile,
    /// Daily check-in and game task rewards.
    Game {
        #[command(subcommand)]
        action: GameCommand,
    },
    /// Send SMS messages.
    Sms {
        #[command(subcommand)]
        action: SmsCommand,
    },
    /// Browse, search and subscribe to plans.
    #[command(args_conflicts_with_subcommands = true)]
    Plans {
        #[command(flatten)]
        options: PlanOptions,
        #[command(subcommand)]
        action: Option<PlanCommand>,
    },
    /// Rotate saved tokens. Synchronize them to any app sharing this session.
    Refresh,
}

#[derive(clap::Args)]
struct PlanOptions {
    #[arg(long, default_value_t = 66)]
    category: u64,
    /// Ordered fields: price or validity, optionally followed by :asc or :desc.
    #[arg(long, value_delimiter = ',', value_parser = parse_sort, default_value = "price", value_name = "FIELD[:asc|desc],...")]
    sort: Vec<(Sort, bool)>,
}

#[derive(Subcommand)]
enum PlanCommand {
    /// List catalog categories.
    Categories,
    /// Search plans on the server within a catalog category.
    Search {
        keyword: String,
        #[command(flatten)]
        options: PlanOptions,
    },
    /// Subscribe to a listed plan. Paid packs charge your Ncell account.
    Subscribe {
        id: String,
        #[arg(long, default_value_t = 66)]
        category: u64,
    },
}

#[derive(Subcommand)]
enum SmsCommand {
    /// Send a free SMS to a Nepal mobile number.
    SendFree { number: String, text: String },
}

#[derive(Subcommand)]
enum GameCommand {
    /// List game tasks, rewards and current progress.
    Tasks,
    /// Claim the daily check-in reward.
    CheckIn,
    /// Claim completed tasks in order; accepts comma-separated IDs.
    ClaimTask {
        #[arg(required = true, value_delimiter = ',', num_args = 1.., value_parser = clap::value_parser!(u64).range(1..))]
        ids: Vec<u64>,
    },
}

#[derive(Clone, Copy)]
enum Sort {
    Price,
    Validity,
}

fn parse_sort(value: &str) -> std::result::Result<(Sort, bool), String> {
    let (field, direction) = value.split_once(':').unwrap_or((value, "asc"));
    let field = match field {
        "price" => Sort::Price,
        "validity" => Sort::Validity,
        _ => return Err("Sort field must be price or validity".into()),
    };
    match direction {
        "asc" => Ok((field, false)),
        "desc" => Ok((field, true)),
        _ => Err("Sort direction must be asc or desc".into()),
    }
}

pub fn run(args: Args) -> Result<()> {
    match args.command {
        Command::Game { action } => {
            let url = authenticated(|client, account, _| {
                client.check_in_url(&account.msisdn, &account.access_token)
            })?;
            let id = match action {
                GameCommand::Tasks => {
                    let response = ncell_cli::api::game_tasks(&url)?;
                    if args.json {
                        println!("{response:#}");
                    } else {
                        print_tasks(&response)?;
                    }
                    return Ok(());
                }
                GameCommand::CheckIn => Vec::new(),
                GameCommand::ClaimTask { ids } => ids,
            };
            let response = ncell_cli::api::check_in(&url, &id, |item| {
                if !args.json {
                    print_task(item);
                }
            })?;
            if id.is_empty() {
                let message = format!(
                    "Check-in successful. Streak: {} days. Weekly points: {}. Game tokens: {}.",
                    response["data"]["current_streak"],
                    response["data"]["weekly_points"],
                    response["data"]["game_tokens"]
                );
                output(args.json, response, message);
            } else {
                let results = response.as_array().unwrap();
                let failed = results.iter().any(|item| task_failed(&item["response"]));
                if failed {
                    return Err(Error::GameTasks {
                        response: Box::new(response),
                    });
                }
                if args.json {
                    println!("{response:#}");
                }
            }
        }
        Command::Sms {
            action: SmsCommand::SendFree { number, text },
        } => {
            let response = authenticated(|client, account, _| {
                client.send_free_sms(&account.msisdn, &account.access_token, &number, &text)
            })?;
            output(
                args.json,
                response,
                format!("SMS sent to {}.", normalize_number(&number)?),
            );
        }
        Command::Plans {
            action: Some(PlanCommand::Subscribe { id, category }),
            ..
        } => {
            let (response, name) = authenticated(|client, account, device_id| {
                let catalog = client.plans(&account.msisdn, &account.access_token, category)?;
                let plan = catalog["availablePackages"]
                    .as_array()
                    .ok_or(Error::InvalidResponse)?
                    .iter()
                    .find(|p| p["id"].as_str() == Some(&id))
                    .ok_or(Error::InvalidInput(
                        "Plan not found in this category. Run plans --category <ID>",
                    ))?;
                let response =
                    client.subscribe(&account.msisdn, &account.access_token, device_id, plan)?;
                Ok((
                    response,
                    text(&plan["displayInfo"]["displayName"]).trim().to_owned(),
                ))
            })?;
            if args.json {
                println!("{response:#}");
            } else {
                println!("Subscribed to {name}.");
            }
        }
        Command::Refresh => {
            AuthService::new(Client::new()?, KeyringStore::new()?).refresh()?;
            output(
                args.json,
                json!({"refreshed": true, "session_saved": true, "mobile_sync_required": true}),
                "Refreshed tokens saved in your system credential store.\nIf this session is shared with the app, synchronize the rotated tokens back.",
            );
        }
        Command::Login { number, verify } => {
            let auth = AuthService::new(Client::new()?, KeyringStore::new()?);
            if !verify {
                auth.request_otp(&number)?;
                eprintln!(
                    "OTP requested. Check your SMS. To resume later: ncell login {number} --verify"
                );
            }
            eprint!("Enter OTP: ");
            io::stderr().flush().map_err(|_| Error::Prompt)?;
            let mut otp = String::new();
            if io::stdin().read_line(&mut otp).map_err(|_| Error::Prompt)? == 0 {
                return Err(Error::Prompt);
            }
            auth.verify_otp(&number, otp.trim())?;
            output(
                args.json,
                json!({"logged_in": true, "msisdn": normalize_number(&number)?, "session_saved": true}),
                "Logged in. Session saved in your system credential store.",
            );
        }
        Command::Status => {
            let account = KeyringStore::new()?.load()?.and_then(|s| s.account);
            match account {
                Some(a) => output(
                    args.json,
                    json!({"logged_in": true, "msisdn": a.msisdn, "authenticated_at": a.authenticated_at, "token_validity_checked": false}),
                    format!(
                        "Session saved for {} ({}). Token validity not checked.",
                        a.msisdn, a.authenticated_at
                    ),
                ),
                None => output(
                    args.json,
                    json!({"logged_in": false, "token_validity_checked": false}),
                    "Not logged in.",
                ),
            }
        }
        Command::Logout => {
            KeyringStore::new()?.clear()?;
            output(
                args.json,
                json!({"session_removed": true}),
                "Local session removed.",
            );
        }
        command @ (Command::Balance | Command::Profile | Command::Plans { .. }) => {
            let profile = matches!(command, Command::Profile);
            let plans = matches!(command, Command::Plans { .. });
            let list_categories = matches!(
                command,
                Command::Plans {
                    action: Some(PlanCommand::Categories),
                    ..
                }
            );
            let (options, keyword) = match &command {
                Command::Plans {
                    action: Some(PlanCommand::Search { keyword, options }),
                    ..
                } => (Some(options), keyword.as_str()),
                Command::Plans { options, .. } => (Some(options), ""),
                _ => (None, ""),
            };
            let (mut data, number) = authenticated(|client, account, _| {
                let data = if list_categories {
                    client.categories(&account.msisdn, &account.access_token)?
                } else if plans {
                    if matches!(
                        &command,
                        Command::Plans {
                            action: Some(PlanCommand::Search { .. }),
                            ..
                        }
                    ) && keyword.trim().is_empty()
                    {
                        return Err(Error::InvalidInput("Search text cannot be empty"));
                    }
                    client.search_plans(
                        &account.msisdn,
                        &account.access_token,
                        options.unwrap().category,
                        keyword,
                    )?
                } else if profile {
                    client.profile(&account.msisdn, &account.access_token)?
                } else {
                    client.balance(&account.msisdn, &account.access_token)?
                };
                Ok((data, account.msisdn.clone()))
            })?;
            if let Some(options) = options.filter(|_| !list_categories) {
                let packages = data["availablePackages"]
                    .as_array_mut()
                    .ok_or(Error::InvalidResponse)?;
                packages.sort_by(|a, b| {
                    options
                        .sort
                        .iter()
                        .map(|&(field, descending)| {
                            let order = sort_key(a, field).total_cmp(&sort_key(b, field));
                            if descending { order.reverse() } else { order }
                        })
                        .find(|order| !order.is_eq())
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
            if args.json {
                println!("{data:#}");
            } else if list_categories {
                println!("{:<6}  {:<22}  Type", "ID", "Category");
                println!("{}", "-".repeat(58));
                for category in data["tags"].as_array().ok_or(Error::InvalidResponse)? {
                    let categories = category["categories"]
                        .as_array()
                        .ok_or(Error::InvalidResponse)?
                        .iter()
                        .map(|c| text(&c["displayName"]))
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!(
                        "{:<6}  {:<22}  {categories}",
                        category["id"].to_string(),
                        text(&category["name"]).trim()
                    );
                }
            } else if plans {
                let plans = data["availablePackages"]
                    .as_array()
                    .ok_or(Error::InvalidResponse)?;
                if plans.is_empty() {
                    println!("No plans available for this category and account.");
                    return Ok(());
                }
                let width = plans
                    .iter()
                    .map(|p| {
                        text(&p["displayInfo"]["displayName"])
                            .trim()
                            .chars()
                            .count()
                    })
                    .max()
                    .unwrap_or(4)
                    .max(4);
                println!(
                    "{:<6}  {:<width$}  {:>12}  {:>10}",
                    "ID", "Plan", "Price", "Validity"
                );
                println!("{}", "-".repeat(width + 38));
                for plan in plans {
                    let price = &plan["productOfferingPrice"];
                    let price = if text(&price["priceUom"]) == "FREE" {
                        "Free".into()
                    } else {
                        let amount = text(&price["price"]);
                        let amount = amount.strip_suffix(".00").unwrap_or(amount);
                        format!("Rs. {amount}")
                    };
                    println!(
                        "{:<6}  {:<width$}  {:>12}  {:>10}",
                        text(&plan["id"]),
                        text(&plan["displayInfo"]["displayName"]).trim(),
                        price,
                        format!(
                            "{} {}",
                            text(&plan["techInfo"]["validity"]),
                            text(&plan["techInfo"]["validityUom"])
                        )
                    );
                }
            } else if profile {
                println!("Number: {number}");
                for (label, value) in [
                    ("Name", text(&data["accountName"])),
                    ("SIM", text(&data["simType"])),
                    ("Status", profile_status(text(&data["status"]))),
                    (
                        "Plan",
                        text(&data["subscriberDetail"]["ratePlanDisplayName"]),
                    ),
                    ("Payment", text(&data["paymentDetail"]["paidMode"])),
                ] {
                    if !value.is_empty() {
                        println!("{label}: {value}");
                    }
                }
            } else {
                println!("Number: {number}");
                for (label, key, unit) in [
                    ("Credit", "creditBalanceDetail", "creditUom"),
                    ("Data", "aggregateDataBalance", "uom"),
                    ("Voice", "aggregateVoiceBalance", "uom"),
                    ("SMS", "aggregateSmsBalance", "uom"),
                ] {
                    let item = &data[key];
                    let amount = &item["balance"];
                    let amount = amount
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| amount.to_string());
                    println!("{label}: {amount} {}", text(&item[unit]));
                }
            }
        }
    }
    Ok(())
}

fn authenticated<T>(request: impl Fn(&Client, &Account, &str) -> Result<T>) -> Result<T> {
    AuthService::new(Client::new()?, KeyringStore::new()?).authenticated(request)
}

fn print_tasks(response: &Value) -> Result<()> {
    let tasks = response["challenges"]
        .as_array()
        .ok_or(Error::InvalidResponse)?;
    if tasks.is_empty() {
        println!("No game tasks available.");
        return Ok(());
    }
    let width = tasks
        .iter()
        .map(|task| text(&task["title"]).chars().count())
        .max()
        .unwrap_or(4)
        .max(4);
    println!(
        "{:<6}  {:<width$}  {:>9}  {:>6}  {:>6}  {:<8}  Status",
        "ID", "Task", "Progress", "Tokens", "XP", "Reset"
    );
    println!("{}", "-".repeat(width + 53));
    for task in tasks {
        let progress = format!("{}/{}", task["current_progress"], task["target_value"]);
        let status = if task["rewarded"] == 1 || task["rewarded"] == true {
            "Claimed"
        } else if task["completed"] == 1 || task["completed"] == true {
            "Completed"
        } else {
            "Pending"
        };
        println!(
            "{:<6}  {:<width$}  {:>9}  {:>6}  {:>6}  {:<8}  {status}",
            task["id"].to_string(),
            text(&task["title"]),
            progress,
            task["reward_tokens"].to_string(),
            task["reward_xp"].to_string(),
            text(&task["reset_type"])
        );
    }
    Ok(())
}

fn print_task(item: &Value) {
    let data = &item["response"];
    if data["success"] == true {
        println!(
            "Task {} claimed. Tokens awarded: {}. Game tokens: {}. Weekly points: {}.",
            item["id"], data["points_awarded"], data["new_token_balance"], data["new_week_points"]
        );
    } else {
        println!(
            "Task {}: {}",
            item["id"],
            data["message"]
                .as_str()
                .or_else(|| data["error"].as_str())
                .unwrap_or("Claim failed")
        );
    }
}

fn task_failed(response: &Value) -> bool {
    response["success"] != true && response["error"] != "Task already completed today"
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

fn output(json: bool, data: Value, message: impl std::fmt::Display) {
    if json {
        println!("{data}");
    } else {
        println!("{message}");
    }
}

fn profile_status(code: &str) -> &str {
    match code {
        "B01" => "Active (B01)",
        "B03" => "All Services Blocked (B03)",
        "B04" => "Outgoing Blocked (B04)",
        _ => code,
    }
}

fn sort_key(plan: &Value, sort: Sort) -> f64 {
    let (value, multiplier) = match sort {
        Sort::Price => (&plan["productOfferingPrice"]["price"], 1.0),
        Sort::Validity => (
            &plan["techInfo"]["validity"],
            match text(&plan["techInfo"]["validityUom"]) {
                "MINUTE" => 1.0,
                "HOUR" => 60.0,
                "DAY" => 1440.0,
                "WEEK" => 10080.0,
                _ => f64::INFINITY,
            },
        ),
    };
    value
        .as_f64()
        .or_else(|| text(value).parse().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(f64::INFINITY)
        * multiplier
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
            matches!(args.command, Command::Game { action: GameCommand::ClaimTask { ids } } if ids == [1, 2, 3])
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
            vec!["game", "tasks"],
            vec!["game", "claim-task", "1,2,3"],
        ] {
            let args =
                Args::try_parse_from(std::iter::once("ncell").chain(command).chain(["--json"]))
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
}
