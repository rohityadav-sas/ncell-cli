mod account;
mod auth;
mod client;
pub mod models;
pub use client::Client;

mod check_in;
pub use check_in::{GameSession, already_checked_in, already_claimed};

mod game_ranks;

mod game_math;

/// Default data catalog used by both CLI arguments and API requests.
pub const DEFAULT_PLAN_CATEGORY: u64 = 66;

/// App version string sent to Ncell (mimics the Android app).
pub const APP_VERSION: &str = "8.2.1.0";

/// Third-party client ID used for the encrypted-MSISDN handshake.
pub const GAME_CLIENT_ID: &str = "47c0b8f2-1766-c12a-5c88-519e40af83b4";

/// Tokens charged per game entry (spin, scratch and math all charge this).
pub const GAME_TOKEN_COST: u64 = 25;

/// How long category and game-session caches are trusted.
pub const CATEGORY_TTL_SECS: u64 = 600;

const GAME_BASE_URL: &str = "https://sajeeb.com.np/ncellapp/checkin";

fn game_endpoint(path: &str) -> String {
    format!("{GAME_BASE_URL}/{path}")
}
