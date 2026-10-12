mod account;
mod auth;
mod client;
pub mod models;
pub use client::Client;

mod check_in;
pub use check_in::{check_in, game_scratch, game_spin, game_tasks};

mod game_ranks;
pub use game_ranks::game_ranks;

mod game_math;
pub use game_math::game_math;
