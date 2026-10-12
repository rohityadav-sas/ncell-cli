use super::*;
#[test]
fn parses_masked_players_and_optional_performance_without_fabricating_numbers() {
    let response = parse(r#"<h1>Leaderboard</h1><div class="space-y-2"><div><span class="font-900">#1</span><div class="truncate">9827***481</div><div class="text-xs text-gray-400">2 days active</div><div class="text-right"><div class="font-900">11,840</div></div></div></div>"#).unwrap();
    assert_eq!(response["players"][0]["points"], 11840);
    assert_eq!(response["players"][0]["number"], "9827***481");
    assert!(response["performance"].is_null());
    assert!(parse("<h1>Login required</h1>").is_err());
}
