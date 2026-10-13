mod script;
use clap::builder::styling::{AnsiColor, Style};
use clap::{Parser, Subcommand};
use ncell_cli::{
    api::{Client, DEFAULT_PLAN_CATEGORY},
    auth::{AuthService, normalize_number},
    error::{Error, Result},
    session::{Account, KeyringStore, SessionStore},
};
use serde_json::{Value, json};
use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

#[derive(Parser, Clone)]
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

#[derive(Subcommand, Clone)]
enum Command {
    /// Create, manage and run saved command scripts.
    Script {
        #[command(subcommand)]
        action: script::ScriptCommand,
    },
    /// Request an SMS code and log in. Use --verify to resume a pending login.
    Login {
        number: String,
        #[arg(long)]
        verify: bool,
    },
    /// List saved accounts or switch the active account.
    Account {
        #[command(subcommand)]
        action: AccountCommand,
    },
    /// Show local session status (does not check token validity).
    Status,
    /// Remove the active account locally. Does not revoke server tokens.
    Logout,
    /// Query credit, data, voice and SMS balances.
    Balance,
    /// Query the subscriber profile; use --json for all details.
    Profile,
    /// Game status, rewards and leaderboard.
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

#[derive(Subcommand, Clone)]
enum AccountCommand {
    /// Save an account without switching. Use --verify to resume its pending login.
    Add {
        number: String,
        #[arg(long)]
        verify: bool,
    },
    /// Remove a specific saved account locally.
    Remove { number: String },
    /// List saved accounts.
    List,
    /// Switch to a saved account without requesting an OTP.
    Switch { number: String },
}

#[derive(clap::Args, Clone)]
struct PlanOptions {
    #[arg(long, default_value_t = DEFAULT_PLAN_CATEGORY)]
    category: u64,
    /// Ordered fields: price or validity, optionally followed by :asc or :desc.
    #[arg(long, value_delimiter = ',', value_parser = parse_sort, default_value = "price", value_name = "FIELD[:asc|desc],...")]
    sort: Vec<(Sort, bool)>,
}

#[derive(Subcommand, Clone)]
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
        #[arg(long, default_value_t = DEFAULT_PLAN_CATEGORY)]
        category: u64,
    },
}

#[derive(Subcommand, Clone)]
enum SmsCommand {
    /// Send a free SMS to a Nepal mobile number.
    SendFree { number: String, text: String },
}

#[derive(Subcommand, Clone)]
enum GameCommand {
    /// Spin the wheel. Each spin costs game tokens; stops on the first failure.
    Spin(GamePlayOptions),
    /// Scratch a card. Each card costs game tokens; stops on the first failure.
    Scratch(GamePlayOptions),
    /// Automatically solve a math challenge. Each game costs game tokens.
    Math(GamePlayOptions),
    /// Show weekly ranks, performance and prizes.
    Ranks,
    /// List game tasks, rewards and current progress.
    Status,
    /// Claim the daily check-in reward.
    CheckIn,
    /// Claim tasks by ID, or all completed unclaimed tasks.
    ClaimTask {
        #[arg(required = true, value_delimiter = ',', num_args = 1.., value_parser = parse_task_id)]
        ids: Vec<String>,
    },
}

#[derive(clap::Args, Clone)]
struct GamePlayOptions {
    /// Number of games to play, or all until tokens are insufficient.
    #[arg(long, default_value = "1", value_parser = parse_count)]
    count: String,
    /// Wait between games: seconds by default, or use s, m, h (e.g. 1.5s).
    #[arg(long, default_value = "0", value_parser = parse_delay)]
    delay: Duration,
}

fn parse_count(value: &str) -> std::result::Result<String, String> {
    if value == "all" || value.parse::<u32>().is_ok_and(|n| n > 0) {
        Ok(value.into())
    } else {
        Err("Count must be a positive integer or all".into())
    }
}
fn parse_task_id(value: &str) -> std::result::Result<String, String> {
    if value == "all" || value.parse::<u64>().is_ok_and(|n| n > 0) {
        Ok(value.into())
    } else {
        Err("Task ID must be a positive integer or all".into())
    }
}
fn validate_args(args: &Args) -> std::result::Result<(), String> {
    if let Command::Game {
        action: GameCommand::ClaimTask { ids },
    } = &args.command
        && ids.iter().any(|id| id == "all")
        && ids.len() != 1
    {
        return Err("Use all alone, without task IDs".into());
    }
    Ok(())
}
fn affordable_total(played: u64, tokens: u64, cost: u64) -> u64 {
    played.saturating_add(tokens / cost)
}

fn insufficient_tokens(error: &Error) -> bool {
    matches!(error, Error::CheckIn { status: 200, message, .. }
        if ["Insufficient tokens", "Not enough points"].iter().any(|known| message.trim().eq_ignore_ascii_case(known)))
}
fn claimable_ids(response: &Value) -> Result<Vec<u64>> {
    response["challenges"]
        .as_array()
        .ok_or(Error::InvalidResponse)?
        .iter()
        .filter(|task| {
            (task["completed"] == true || task["completed"] == 1)
                && (task["rewarded"] == false || task["rewarded"] == 0)
        })
        .map(|task| {
            task["id"]
                .as_u64()
                .or_else(|| task["id"].as_str()?.parse().ok())
                .filter(|id| *id > 0)
                .ok_or(Error::InvalidResponse)
        })
        .collect()
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

fn parse_delay(value: &str) -> std::result::Result<Duration, String> {
    let (number, multiplier) = match value.as_bytes().last() {
        Some(b's') => (&value[..value.len() - 1], 1.0),
        Some(b'm') => (&value[..value.len() - 1], 60.0),
        Some(b'h') => (&value[..value.len() - 1], 3600.0),
        _ => (value, 1.0),
    };
    number
        .parse::<f64>()
        .ok()
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .and_then(|seconds| Duration::try_from_secs_f64(seconds * multiplier).ok())
        .ok_or_else(|| "Delay must be a nonnegative duration: 1.5, 1.5s, 2m, or 1.5h".into())
}

#[derive(Default)]
struct Runtime {
    client: Option<Client>,
    game: Option<(String, Instant, ncell_cli::api::GameSession)>,
}
impl Runtime {
    fn client(&mut self) -> Result<Client> {
        if self.client.is_none() {
            self.client = Some(Client::new()?);
        }
        Ok(self.client.as_ref().unwrap().clone())
    }
    fn game(&mut self) -> Result<&mut ncell_cli::api::GameSession> {
        let client = self.client()?;
        let session = KeyringStore::new()?.load()?.ok_or(Error::NotLoggedIn)?;
        let account = session.account.as_ref().ok_or(Error::NotLoggedIn)?;
        let identity = format!(
            "{}:{}:{}",
            account.msisdn, session.device_id, account.access_token
        );
        self.cached_game(identity, || {
            let (url, identity) = AuthService::new(client, KeyringStore::new()?).authenticated(
                |client, account, device| {
                    Ok((
                        client.check_in_url(&account.msisdn, &account.access_token)?,
                        format!("{}:{}:{}", account.msisdn, device, account.access_token),
                    ))
                },
            )?;
            Ok((identity, ncell_cli::api::GameSession::open(&url)?))
        })
    }
    fn cached_game(
        &mut self,
        identity: String,
        create: impl FnOnce() -> Result<(String, ncell_cli::api::GameSession)>,
    ) -> Result<&mut ncell_cli::api::GameSession> {
        if self.game.as_ref().is_none_or(|(key, created, game)| {
            key != &identity
                || created.elapsed() >= Duration::from_secs(600)
                || game.tokens.is_none()
        }) {
            let (identity, session) = create()?;
            self.game = Some((identity, Instant::now(), session));
        }
        Ok(&mut self.game.as_mut().unwrap().2)
    }
}
pub fn run(args: Args) -> Result<()> {
    run_with(args, &mut Runtime::default())
}
fn run_with(args: Args, runtime: &mut Runtime) -> Result<()> {
    validate_args(&args).map_err(Error::Script)?;
    if matches!(
        &args.command,
        Command::Login { .. }
            | Command::Logout
            | Command::Refresh
            | Command::Account {
                action: AccountCommand::Add { .. }
                    | AccountCommand::Remove { .. }
                    | AccountCommand::Switch { .. }
            }
    ) {
        runtime.game = None;
    }
    match args.command {
        Command::Script { action } => return script::run(action, args.json, runtime),
        Command::Game { action } => {
            let game = runtime.game()?;
            let scratch = matches!(action, GameCommand::Scratch(_));
            let math = matches!(action, GameCommand::Math(_));
            let id = match action {
                GameCommand::Spin(GamePlayOptions { count, delay })
                | GameCommand::Scratch(GamePlayOptions { count, delay })
                | GameCommand::Math(GamePlayOptions { count, delay }) => {
                    let all = count == "all";
                    let count = if all {
                        0
                    } else {
                        count.parse::<u64>().unwrap()
                    };
                    // All three supported games currently charge 25 tokens per entry.
                    let cost = 25;
                    let mut balance = game.tokens;
                    for index in 0u64.. {
                        if !all && index >= count {
                            break;
                        }
                        let total = if all {
                            let remaining = balance.ok_or(Error::InvalidInput(
                                "Game page did not provide the token balance",
                            ))?;
                            let total = affordable_total(index, remaining, cost);
                            if total == index {
                                output(
                                    args.json,
                                    json!({"stop":"insufficient_tokens","games":index}),
                                    "No tokens left for another game.",
                                );
                                return Ok(());
                            }
                            total
                        } else {
                            count
                        };
                        if index > 0 {
                            wait_between_games(delay, !args.json)?;
                        }
                        let repeated = all || count > 1;
                        if repeated {
                            if args.json {
                                script::emit_json(json!({"event":"game_start", "game":index + 1,
                                    "total":total}));
                            } else {
                                if index > 0 {
                                    println!();
                                }
                                let progress = AnsiColor::Cyan.on_default().bold();
                                let label = format!("Game {} of {total}", index + 1);
                                anstream::println!("{progress}{label}{progress:#}\n");
                                io::stdout().flush().map_err(|_| Error::InvalidResponse)?;
                            }
                        }
                        if !math && !args.json {
                            println!(
                                "{:<12}  {:<20}  {:>8}  {:>12}",
                                "Game", "Reward", "Tokens", "Points"
                            );
                            println!("{}", "-".repeat(58));
                        }
                        if math {
                            if !args.json {
                                println!(
                                    "{:>5}  {:<28}  {:>6}  {:<7}  {:>5}",
                                    "Level", "Question", "Answer", "Result", "Score"
                                );
                                println!("{}", "-".repeat(59));
                            }
                            let result = game.math(
                                |item| {
                                    let message = format!(
                                        "{:>5}  {:<28}  {:>6}  {:<7}  {:>5}",
                                        formatted_number(&item["level"]),
                                        text(&item["question"]),
                                        text(&item["answer"]),
                                        text(&item["response"]["result"]),
                                        formatted_number(
                                            item["response"]
                                                .get("score")
                                                .unwrap_or(&item["response"]["total_score"])
                                        )
                                    );
                                    output(args.json, item.clone(), message);
                                    let _ = io::stdout().flush();
                                },
                                |question| {
                                    manual_math_answer(
                                        question,
                                        &mut io::stdin().lock(),
                                        &mut io::stderr().lock(),
                                    )
                                },
                            );
                            let response = match result {
                                Err(error) if all && insufficient_tokens(&error) => {
                                    output(
                                        args.json,
                                        json!({"stop":"insufficient_tokens","games":index}),
                                        "No tokens left for another game.",
                                    );
                                    return Ok(());
                                }
                                result => result?,
                            };
                            // bg_reward is not a token refund; only deduct the entry fee.
                            balance = balance.map(|tokens| tokens.saturating_sub(cost));
                            game.tokens = balance;
                            let (won, message) = math_summary(&response);
                            output(args.json, response, message);
                            io::stdout().flush().map_err(|_| Error::InvalidResponse)?;
                            if !won {
                                return Err(Error::GameLost);
                            }
                            continue;
                        }
                        let response = if scratch { game.scratch() } else { game.spin() };
                        let response = match response {
                            Err(error) if all && insufficient_tokens(&error) => {
                                output(
                                    args.json,
                                    json!({"stop":"insufficient_tokens","games":index}),
                                    "No tokens left for another game.",
                                );
                                return Ok(());
                            }
                            result => result?,
                        };
                        let label = if scratch { "Scratch" } else { "Spin" };
                        let tokens = if scratch {
                            "points_remaining"
                        } else {
                            "new_token_balance"
                        };
                        balance = response[tokens].as_u64();
                        game.tokens = balance;
                        let message = format!(
                            "{:<12}  {:<20}  {:>8}  {:>12}",
                            format!("{label} {}", index + 1),
                            text(&response["description"]),
                            formatted_number(&response[tokens]),
                            formatted_number(&response["new_week_points"])
                        );
                        output(args.json, response, message);
                        io::stdout().flush().map_err(|_| Error::InvalidResponse)?;
                    }
                    return Ok(());
                }
                GameCommand::Ranks => {
                    let response = game.ranks()?;
                    if args.json {
                        script::emit_json(response);
                    } else {
                        print_ranks(&response);
                    }
                    return Ok(());
                }
                GameCommand::Status => {
                    let response = game.tasks()?;
                    if args.json {
                        script::emit_json(response);
                    } else {
                        print_tasks(&response)?;
                    }
                    return Ok(());
                }
                GameCommand::CheckIn => Vec::new(),
                GameCommand::ClaimTask { ids } => {
                    if ids == ["all"] {
                        let ids = claimable_ids(&game.tasks()?)?;
                        if ids.is_empty() {
                            output(
                                args.json,
                                json!({"claimed":[]}),
                                "No tasks available to claim.",
                            );
                            return Ok(());
                        }
                        ids
                    } else {
                        ids.iter().map(|id| id.parse().unwrap()).collect()
                    }
                }
            };
            let response = game.claim(&id, |item| {
                if !args.json {
                    print_task(item);
                }
            })?;
            if id.is_empty() {
                let message = if ncell_cli::api::already_checked_in(&response) {
                    "Already checked in today.".to_owned()
                } else {
                    format!(
                        "Check-in successful. Streak: {} {}. Weekly points: {}. Game tokens: {}.",
                        response["data"]["current_streak"],
                        if response["data"]["current_streak"] == 1 {
                            "day"
                        } else {
                            "days"
                        },
                        response["data"]["weekly_points"],
                        response["data"]["game_tokens"]
                    )
                };
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
                    script::emit_json(response);
                }
            }
        }
        Command::Sms {
            action: SmsCommand::SendFree { number, text },
        } => {
            let response = authenticated(runtime.client()?, |client, account, _| {
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
            let (response, name) =
                authenticated(runtime.client()?, |client, account, device_id| {
                    let catalog = client.plans(&account.msisdn, &account.access_token, category)?;
                    let plan = catalog["availablePackages"]
                        .as_array()
                        .ok_or(Error::InvalidResponse)?
                        .iter()
                        .find(|p| p["id"].as_str() == Some(&id))
                        .ok_or(Error::InvalidInput(
                            "Plan not found in this category. Run plans --category <ID>",
                        ))?;
                    let response = client.subscribe(
                        &account.msisdn,
                        &account.access_token,
                        device_id,
                        plan,
                    )?;
                    Ok((
                        response,
                        text(&plan["displayInfo"]["displayName"]).trim().to_owned(),
                    ))
                })?;
            if args.json {
                script::emit_json(response);
            } else {
                println!("Subscribed to {name}.");
            }
        }
        Command::Refresh => {
            AuthService::new(runtime.client()?, KeyringStore::new()?).refresh()?;
            output(
                args.json,
                json!({"refreshed": true, "session_saved": true, "mobile_sync_required": true}),
                "Refreshed tokens saved in your system credential store.\nIf this session is shared with the app, synchronize the rotated tokens back.",
            );
        }
        Command::Login { number, verify } => {
            login(runtime.client()?, &number, verify, true, args.json)?
        }
        Command::Account { action } => {
            let store = KeyringStore::new()?;
            let mut session = store.load()?.unwrap_or_default();
            match action {
                AccountCommand::Add { number, verify } => {
                    drop(store);
                    login(runtime.client()?, &number, verify, false, args.json)?;
                }
                AccountCommand::Remove { number } => {
                    let number = normalize_number(&number)?;
                    session.remove(number)?;
                    store.save(&session)?;
                    output(
                        args.json,
                        json!({"removed_account":number,"active_account":session.account.as_ref().map(|a| &a.msisdn)}),
                        format!("Removed {number}."),
                    );
                }
                AccountCommand::List => {
                    let accounts: Vec<_> = session
                        .account
                        .iter()
                        .map(|a| json!({"number":a.msisdn,"active":true}))
                        .chain(
                            session
                                .other_accounts
                                .iter()
                                .map(|saved| json!({"number":saved.account.msisdn,"active":false})),
                        )
                        .collect();
                    if args.json {
                        output(true, json!(accounts), "");
                    } else if accounts.is_empty() {
                        println!("No saved accounts.");
                    } else {
                        println!("{:<12}  Status", "Number");
                        println!("{}", "-".repeat(20));
                        for account in accounts {
                            println!(
                                "{:<12}  {}",
                                text(&account["number"]),
                                if account["active"] == true {
                                    "Active"
                                } else {
                                    ""
                                }
                            );
                        }
                    }
                }
                AccountCommand::Switch { number } => {
                    let number = normalize_number(&number)?;
                    session.switch(number)?;
                    store.save(&session)?;
                    output(
                        args.json,
                        json!({"active_account":number}),
                        format!("Switched to {number}."),
                    );
                }
            }
        }
        Command::Status => {
            let account = KeyringStore::new()?.load()?.and_then(|s| s.account);
            match account {
                Some(a) => output(
                    args.json,
                    json!({"logged_in": true, "msisdn": a.msisdn, "authenticated_at": a.authenticated_at, "token_saved_at": a.token_saved_at, "access_token_expires_at": a.access_token_expires_at, "token_validity_checked": false}),
                    format!(
                        "Account:       {}\nLogged in:     {}\nToken saved:   {}\nToken expires: {}",
                        a.msisdn,
                        status_date(&a.authenticated_at.to_rfc3339()),
                        a.token_saved_at
                            .map(|t| status_date(&t.to_rfc3339()))
                            .unwrap_or_else(|| "Unknown".into()),
                        a.access_token_expires_at
                            .as_deref()
                            .map(status_date)
                            .unwrap_or_else(|| "Unknown".into())
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
            let store = KeyringStore::new()?;
            let mut session = store.load()?.unwrap_or_default();
            session.logout();
            store.save(&session)?;
            output(
                args.json,
                json!({"account_removed": true, "active_account": session.account.as_ref().map(|a| &a.msisdn)}),
                "Active account removed. Run ncell account list to see remaining accounts.",
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
            let (mut data, number) = authenticated(runtime.client()?, |client, account, _| {
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
                script::emit_json(data);
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
                print_balance(&data, &number);
            }
        }
    }
    Ok(())
}

fn login(client: Client, number: &str, verify: bool, activate: bool, json: bool) -> Result<()> {
    let auth = AuthService::new(client, KeyringStore::new()?);
    let command = if activate { "login" } else { "account add" };
    if !verify {
        auth.request_otp(number)?;
        eprintln!(
            "OTP requested. Check your SMS. To resume later: ncell {command} {number} --verify"
        );
    }
    eprint!("Enter OTP: ");
    io::stderr().flush().map_err(|_| Error::Prompt)?;
    let mut otp = String::new();
    if io::stdin().read_line(&mut otp).map_err(|_| Error::Prompt)? == 0 {
        return Err(Error::Prompt);
    }
    auth.verify_account(number, otp.trim(), activate)?;
    output(
        json,
        json!({"msisdn":normalize_number(number)?,"session_saved":true,"made_active":activate}),
        if activate {
            "Logged in. Session saved in your system credential store."
        } else {
            "Account saved. Active account unchanged."
        },
    );
    Ok(())
}

fn balance_amount(item: &Value, unit: &str) -> String {
    if item["hasUnlimitedResource"] == true {
        return "Unlimited".into();
    }
    let scalar = |value: &Value| {
        value.as_str().map(str::to_owned).unwrap_or_else(|| {
            if value.is_null() {
                "-".into()
            } else {
                value.to_string()
            }
        })
    };
    if unit == "creditUom" {
        let value = item["balance"]
            .as_f64()
            .or_else(|| item["balance"].as_str()?.parse::<f64>().ok());
        return value
            .filter(|v| v.is_finite())
            .map(|v| format!("Rs. {v:.2}"))
            .unwrap_or_else(|| format!("Rs. {}", scalar(&item["balance"])));
    }
    let mut amount = scalar(&item["balance"]);
    if !item["initialAmount"].is_null() {
        amount.push_str(&format!(" / {}", scalar(&item["initialAmount"])));
    }
    let unit = text(&item[unit]);
    if !unit.is_empty() {
        amount.push_str(&format!(" {unit}"));
    }
    let numeric = |v: &Value| v.as_f64().or_else(|| v.as_str()?.parse::<f64>().ok());
    if let (Some(left), Some(total)) = (numeric(&item["balance"]), numeric(&item["initialAmount"]))
        && left.is_finite()
        && total.is_finite()
        && total > 0.0
    {
        amount.push_str(&format!(
            " ({:.0}% left)",
            (left / total * 100.0).clamp(0.0, 100.0)
        ));
    }
    amount
}

fn status_date(value: &str) -> String {
    if let Ok(date) = chrono::DateTime::parse_from_rfc3339(value) {
        return date
            .with_timezone(&chrono::FixedOffset::east_opt(5 * 3600 + 45 * 60).unwrap())
            .format("%b %d %Y %I:%M:%S %p")
            .to_string();
    }
    chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S")
        .map(|date| date.format("%b %d %Y %I:%M:%S %p").to_string())
        .unwrap_or_else(|_| value.to_owned())
}

fn balance_date(value: &str) -> String {
    chrono::NaiveDateTime::parse_from_str(value, "%b %d %Y %H:%M:%S")
        .map(|date| date.format("%b %d %Y %I:%M:%S %p").to_string())
        .unwrap_or_else(|_| value.to_owned())
}

fn empty_allowance(item: &Value) -> bool {
    let zero = |v: &Value| {
        v.is_null() || v.as_f64().or_else(|| v.as_str()?.parse::<f64>().ok()) == Some(0.0)
    };
    item["hasUnlimitedResource"] != true && zero(&item["balance"]) && zero(&item["initialAmount"])
}

fn print_balance(data: &Value, _number: &str) {
    for (label, key, unit) in [
        ("Balance", "creditBalanceDetail", "creditUom"),
        ("Data", "aggregateDataBalance", "uom"),
        ("Voice", "aggregateVoiceBalance", "uom"),
        ("SMS", "aggregateSmsBalance", "uom"),
    ] {
        let list = match label {
            "Data" => "dataBalanceList",
            "Voice" => "voiceBalanceList",
            "SMS" => "smsBalanceList",
            _ => "",
        };
        if !data[key].is_null()
            && (label == "Balance" || !empty_allowance(&data[key]))
            && !data[list].as_array().is_some_and(|packs| !packs.is_empty())
        {
            println!(
                "{:<15} {}",
                format!("{label}:"),
                balance_amount(&data[key], unit)
            );
        }
    }
    let credit = &data["creditBalanceDetail"];
    for (label, key) in [
        ("Last recharge", "lastRechargeDate"),
        ("Expires", "expiryDate"),
    ] {
        if let Some(value) = credit[key].as_str().filter(|v| !v.is_empty()) {
            println!("{:<15} {}", format!("{label}:"), balance_date(value));
        }
    }
    let packs: Vec<_> = [
        ("Data", "dataBalanceList"),
        ("Voice", "voiceBalanceList"),
        ("SMS", "smsBalanceList"),
        ("Bonus", "bonusBalanceList"),
    ]
    .into_iter()
    .flat_map(|(kind, key)| {
        data[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|pack| !empty_allowance(pack))
            .map(move |pack| (kind, pack))
    })
    .collect();
    if packs.is_empty() {
        return;
    }
    let width = packs
        .iter()
        .map(|(_, pack)| {
            text(&pack["accName"])
                .chars()
                .count()
                .max(text(&pack["ncellName"]).chars().count())
        })
        .max()
        .unwrap_or(4)
        .max(4);
    println!(
        "\n{:<6}  {:<width$}  {:<32}  Expires",
        "Type", "Pack", "Remaining / Initial"
    );
    let expiry_width = packs
        .iter()
        .map(|(_, pack)| {
            balance_date(pack["expDate"].as_str().unwrap_or("-"))
                .chars()
                .count()
        })
        .max()
        .unwrap_or(7)
        .max(7);
    println!("{}", "-".repeat(6 + 2 + width + 2 + 32 + 2 + expiry_width));
    for (kind, pack) in packs {
        let mut details = pack.clone();
        let (aggregate, list) = match kind {
            "Data" => ("aggregateDataBalance", "dataBalanceList"),
            "Voice" => ("aggregateVoiceBalance", "voiceBalanceList"),
            "SMS" => ("aggregateSmsBalance", "smsBalanceList"),
            _ => ("", ""),
        };
        if data[list].as_array().is_some_and(|items| items.len() == 1) {
            for key in ["balance", "uom", "hasUnlimitedResource"] {
                if details[key].is_null() && !data[aggregate][key].is_null() {
                    details[key] = data[aggregate][key].clone();
                }
            }
        }
        let name = pack["accName"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| pack["ncellName"].as_str())
            .unwrap_or("-");
        println!(
            "{kind:<6}  {name:<width$}  {:<32}  {}",
            balance_amount(&details, "uom"),
            balance_date(pack["expDate"].as_str().unwrap_or("-"))
        );
    }
}

fn authenticated<T>(
    client: Client,
    request: impl Fn(&Client, &Account, &str) -> Result<T>,
) -> Result<T> {
    AuthService::new(client, KeyringStore::new()?).authenticated(request)
}

fn print_ranks(response: &Value) {
    let heading = Style::new().bold();
    let title = AnsiColor::Cyan.on_default().bold();
    let dim = Style::new().dimmed();
    anstream::println!("{title}{}{title:#}", text(&response["week"]));
    let current = &response["performance"];
    if !current.is_null() {
        anstream::println!(
            "{heading}{:<17}{heading:#} Rank {:>6} | Points {:>8} | Tokens {} | Streak {} {}",
            "Your performance:",
            format!("#{}", formatted_number(&current["rank"])),
            formatted_number(&current["points"]),
            formatted_number(&current["tokens"]),
            formatted_number(&current["streak_days"]),
            if current["streak_days"] == 1 {
                "day"
            } else {
                "days"
            }
        );
    }
    let last = &response["last_week"];
    if !last["rank"].is_null() {
        anstream::println!(
            "{heading}{:<17}{heading:#} Rank {:>6} | Points {:>8} | Prize {}",
            "Last week:",
            format!("#{}", formatted_number(&last["rank"])),
            formatted_number(&last["points"]),
            text(&last["prize"]).trim_start_matches(|c: char| !c.is_ascii_alphanumeric())
        );
    }
    anstream::println!("\n{heading}Weekly prizes{heading:#}");
    println!("  {:<14}  {:>6}", "Rank range", "Reward");
    anstream::println!("  {dim}{}{dim:#}", "-".repeat(22));
    for prize in response["prizes"].as_array().into_iter().flatten() {
        println!(
            "  {:<14}  {:>6}",
            text(&prize["ranks"]),
            text(&prize["reward"])
        );
    }
    let players = response["players"].as_array();
    anstream::println!("\n{heading}Leaderboard{heading:#}");
    println!(
        "  {:>4}  {:<12}  {:>10}  Activity",
        "Rank", "Player", "Points"
    );
    anstream::println!("  {dim}{}{dim:#}", "-".repeat(48));
    for player in players.into_iter().flatten() {
        println!(
            "  {:>4}  {:<12}  {:>10}  {}",
            formatted_number(&player["rank"]),
            text(&player["number"]),
            formatted_number(&player["points"]),
            format_args!(
                "{}{}",
                text(&player["activity"]),
                if player["rank"] == current["rank"] && !current["rank"].is_null() {
                    "  (you)"
                } else {
                    ""
                }
            )
        );
    }
    println!();
}

fn formatted_number(value: &Value) -> String {
    let Some(number) = value.as_u64() else {
        return "-".into();
    };
    let mut result = number.to_string();
    let mut index = result.len();
    while index > 3 {
        index -= 3;
        result.insert(index, ',');
    }
    result
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
        let style = match status {
            "Claimed" => AnsiColor::Green.on_default(),
            "Completed" => AnsiColor::Yellow.on_default(),
            _ => Style::new(),
        };
        anstream::println!(
            "{style}{:<6}  {:<width$}  {:>9}  {:>6}  {:>6}  {:<8}  {status}{style:#}",
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
            "Task {} claimed. Reward awarded: {}. Game tokens: {}. Weekly points: {}.",
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
    response["success"] != true && !ncell_cli::api::already_claimed(response)
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or("")
}

fn wait_between_games(delay: Duration, show_timer: bool) -> Result<()> {
    let visible = show_timer && io::stderr().is_terminal();
    let start = Instant::now();
    let mut stderr = io::stderr().lock();
    loop {
        let remaining = delay.saturating_sub(start.elapsed());
        if remaining.is_zero() {
            break;
        }
        if visible {
            write!(stderr, "\rNext game in {:>10.1}s", remaining.as_secs_f64())
                .and_then(|_| stderr.flush())
                .map_err(|_| Error::InvalidInput("Could not display game countdown"))?;
        }
        std::thread::sleep(remaining.min(Duration::from_millis(100)));
    }
    if visible && !delay.is_zero() {
        write!(stderr, "\r{:58}\r", "")
            .and_then(|_| stderr.flush())
            .map_err(|_| Error::InvalidInput("Could not clear game countdown"))?;
    }
    Ok(())
}

fn math_summary(response: &Value) -> (bool, String) {
    let won = response["result"] == "win" || response["win"] == true;
    let score = response
        .get("total_score")
        .filter(|value| !value.is_null())
        .unwrap_or(&response["score"]);
    (
        won,
        format!(
            "Math {} | Score {}",
            if won { "win" } else { "game_over" },
            formatted_number(score)
        ),
    )
}

fn manual_math_answer(
    question: &Value,
    input: &mut impl io::BufRead,
    prompt: &mut impl Write,
) -> Result<String> {
    let options = question["options"]
        .as_array()
        .ok_or(Error::InvalidResponse)?;
    let failed = || Error::InvalidInput("Could not read a manual math answer; game stopped");
    writeln!(
        prompt,
        "Cannot solve automatically: {}\nOptions: {}\nTime limit: {} seconds",
        text(&question["question"]),
        options
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join(", "),
        formatted_number(&question["time"])
    )
    .map_err(|_| failed())?;
    loop {
        write!(prompt, "Enter answer value: ")
            .and_then(|_| prompt.flush())
            .map_err(|_| failed())?;
        let mut answer = String::new();
        if input.read_line(&mut answer).map_err(|_| failed())? == 0 {
            return Err(failed());
        }
        if let Ok(answer) = answer.trim().parse::<f64>()
            && let Some(option) = options
                .iter()
                .find(|option| option.as_f64() == Some(answer))
        {
            return Ok(option.to_string());
        }
        writeln!(prompt, "Choose one of the displayed answer values.").map_err(|_| failed())?;
    }
}

fn output(json: bool, data: Value, message: impl std::fmt::Display) {
    if json {
        script::emit_json(data);
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
#[path = "../tests/unit/cli.rs"]
mod tests;
