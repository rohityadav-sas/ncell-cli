use crate::error::{Error, Result};
use scraper::{ElementRef, Html, Selector};
use serde_json::{Value, json};

pub(super) fn fetch(web: &reqwest::blocking::Client) -> Result<Value> {
    let html = web
        .get(super::game_endpoint("ranks.php"))
        .send()
        .map_err(Error::from_http)?
        .error_for_status()
        .map_err(Error::from_http)?
        .text()
        .map_err(Error::from_http)?;
    parse(&html)
}

macro_rules! sel {
    ($css:literal) => {{
        static S: std::sync::LazyLock<scraper::Selector> =
            std::sync::LazyLock::new(|| scraper::Selector::parse($css).unwrap());
        &*S
    }};
}

fn text(root: ElementRef<'_>, selector: &Selector) -> String {
    root.select(selector)
        .next()
        .map(|element| {
            element
                .text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn number(text: &str) -> Value {
    text.trim()
        .trim_start_matches('#')
        .replace(',', "")
        .parse::<u64>()
        .map(Value::from)
        .unwrap_or(Value::Null)
}

fn parse(html: &str) -> Result<Value> {
    let document = Html::parse_document(html);
    let root = document.root_element();
    if text(root, sel!("h1")) != "Leaderboard" {
        return Err(Error::InvalidResponse);
    }
    let performance = root
        .select(sel!(".glass-card"))
        .find(|card| card.text().any(|value| value.trim() == "Your Performance"))
        .map(|card| {
            let stats: Vec<_> = card
                .select(sel!(".font-800"))
                .map(|element| number(&element.text().collect::<String>()))
                .collect();
            json!({
                "rank": number(&text(card, sel!(".rank-hero"))),
                "points": stats.first(),
                "tokens": stats.get(1),
                "streak_days": stats.get(2),
                "period": text(card, sel!(".justify-between > span:last-child")),
            })
        });
    let last_week = root.select(sel!(".last-week-card")).next().map(|card| {
        json!({
            "rank": number(&text(card, sel!(".rank-hero-purple"))),
            "points": number(&text(card, sel!(".font-800"))),
            "prize": text(card, sel!(".rank-hero")),
            "period": text(card, sel!(".justify-between > span:last-child")),
        })
    });
    let players: Vec<_> = root
        .select(sel!(".space-y-2 > div"))
        .filter_map(|row| {
            let rank = number(&text(row, sel!("span.font-900")));
            if rank.is_null() {
                return None;
            }
            Some(json!({
                "rank": rank,
                "number": text(row, sel!(".truncate")),
                "activity": text(row, sel!(".text-xs.text-gray-400")),
                "points": number(&text(row, sel!(".text-right .font-900"))),
            }))
        })
        .collect();
    let prizes: Vec<_> = root
        .select(sel!(".divide-y > div"))
        .map(|row| {
            json!({
                "ranks": text(row, sel!(".text-white.font-semibold")),
                "reward": text(row, sel!(".text-right .leading-none")),
            })
        })
        .collect();
    Ok(json!({
        "week": text(root, sel!(".sticky p")).split('•').next().unwrap_or("").trim(),
        "performance": performance,
        "last_week": last_week,
        "prizes": prizes,
        "players": players,
    }))
}

#[cfg(test)]
#[path = "../../tests/unit/api_game_ranks.rs"]
mod tests;
