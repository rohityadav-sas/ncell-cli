use super::*;
#[test]
fn preserves_server_expiry_timestamp() {
    assert_eq!(
        token_expiry(Some(serde_json::json!("2026-10-12T21:49:20"))).as_deref(),
        Some("2026-10-12T21:49:20")
    );
    assert!(token_expiry(Some(serde_json::json!("unknown"))).is_none());
}
