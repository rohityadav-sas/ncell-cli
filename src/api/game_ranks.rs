use super::check_in::{WEB_UA, web_client};
use crate::error::{Error, Result};
use scraper::{ElementRef, Html, Selector};
use serde_json::{Value, json};

pub fn game_ranks(url: &str) -> Result<Value> {
    let web = web_client()?;
    web.get(url)
        .header("User-Agent", WEB_UA)
        .send()
        .map_err(Error::from_http)?
        .error_for_status()
        .map_err(Error::from_http)?;
    fetch(&web)
}

pub(super) fn fetch(web: &reqwest::blocking::Client) -> Result<Value> {
    let html = web
        .get("https://sajeeb.com.np/ncellapp/checkin/ranks.php")
        .header("User-Agent", WEB_UA)
        .send()
        .map_err(Error::from_http)?
        .error_for_status()
        .map_err(Error::from_http)?
        .text()
        .map_err(Error::from_http)?;
    parse(&html)
}

fn select(selector: &'static str) -> &'static Selector {
    static SELECTORS: std::sync::LazyLock<std::collections::HashMap<&'static str, Selector>> =
        std::sync::LazyLock::new(|| {
            [
                ".divide-y > div",
                ".font-800",
                ".glass-card",
                ".justify-between > span:last-child",
                ".last-week-card",
                ".rank-hero",
                ".rank-hero-purple",
                ".space-y-2 > div",
                ".sticky p",
                ".text-right .font-900",
                ".text-right .leading-none",
                ".text-white.font-semibold",
                ".text-xs.text-gray-400",
                ".truncate",
                "h1",
                "span.font-900",
            ]
            .into_iter()
            .map(|s| (s, Selector::parse(s).unwrap()))
            .collect()
        });
    &SELECTORS[selector]
}
fn text(root: ElementRef<'_>, selector: &'static str) -> String {
    root.select(select(selector))
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
    if text(root, "h1") != "Leaderboard" {
        return Err(Error::InvalidResponse);
    }
    let performance = root.select(select(".glass-card")).find(|card| card.text().any(|value| value.trim() == "Your Performance"))
        .map(|card| {
            let stats: Vec<_> = card.select(select(".font-800")).map(|element| number(&element.text().collect::<String>())).collect();
            json!({"rank": number(&text(card, ".rank-hero")), "points": stats.first(), "tokens": stats.get(1), "streak_days": stats.get(2), "period": text(card, ".justify-between > span:last-child")})
        });
    let last_week = root.select(select(".last-week-card")).next().map(|card| json!({
        "rank": number(&text(card, ".rank-hero-purple")), "points": number(&text(card, ".font-800")),
        "prize": text(card, ".rank-hero"), "period": text(card, ".justify-between > span:last-child")
    }));
    let players: Vec<_> = root.select(select(".space-y-2 > div")).filter_map(|row| {
        let rank = number(&text(row, "span.font-900"));
        if rank.is_null() { return None; }
        Some(json!({"rank": rank, "number": text(row, ".truncate"), "activity": text(row, ".text-xs.text-gray-400"), "points": number(&text(row, ".text-right .font-900"))}))
    }).collect();
    let prizes: Vec<_> = root.select(select(".divide-y > div")).map(|row| json!({
        "ranks": text(row, ".text-white.font-semibold"), "reward": text(row, ".text-right .leading-none")
    })).collect();
    Ok(
        json!({"week": text(root, ".sticky p").split('•').next().unwrap_or("").trim(), "performance": performance, "last_week": last_week, "prizes": prizes, "players": players}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_masked_players_and_optional_performance_without_fabricating_numbers() {
        let response = parse(r#"<h1>Leaderboard</h1><div class="space-y-2"><div><span class="font-900">#1</span><div class="truncate">9827***481</div><div class="text-xs text-gray-400">2 days active</div><div class="text-right"><div class="font-900">11,840</div></div></div></div>"#).unwrap();
        assert_eq!(response["players"][0]["points"], 11840);
        assert_eq!(response["players"][0]["number"], "9827***481");
        assert!(response["performance"].is_null());
        assert!(parse("<h1>Login required</h1>").is_err());
    }
}
