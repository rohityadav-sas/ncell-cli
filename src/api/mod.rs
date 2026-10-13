mod account;
mod auth;
mod client;
pub mod models;
pub use client::Client;

mod check_in;
pub use check_in::{GameSession, already_checked_in, already_claimed, check_in, game_tasks};

mod game_ranks;
pub use game_ranks::game_ranks;

mod game_math;

/// Default data catalog used by both CLI arguments and API requests.
pub const DEFAULT_PLAN_CATEGORY: u64 = 66;

const GAME_BASE_URL: &str = "https://sajeeb.com.np/ncellapp/checkin";

fn game_endpoint(path: &str) -> String {
    format!("{GAME_BASE_URL}/{path}")
}
