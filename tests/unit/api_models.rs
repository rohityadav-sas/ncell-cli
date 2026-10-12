use super::*;
#[test]
fn optional_expiry_never_discards_valid_rotated_tokens() {
    for (expiry, expected) in [
        (json!(3600), Some(3600)),
        (json!("3600"), Some(3600)),
        (Value::Null, None),
        (json!("unknown"), None),
    ] {
        let result = tokens(json!({"userAuthResponse":{"accessToken":"test", "refreshToken":"rotated", "expiresIn":expiry}})).unwrap();
        assert_eq!(
            result
                .expires_in
                .as_ref()
                .and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok())),
            expected
        );
        assert_eq!(result.refresh_token.as_deref(), Some("rotated"));
    }
    assert!(
        tokens(json!({"userAuthResponse":{"accessToken":"test"}}))
            .unwrap()
            .expires_in
            .is_none()
    );
}
