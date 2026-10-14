use super::*;

#[test]
fn app_codes_have_meanings_without_exposing_server_data() {
    for (code, expected) in [
        ("BIL2007", "insufficient"),
        ("OTP2006", "Expired"),
        ("SMS2000", "failed"),
        ("CL3001", "Loan"),
        ("BSAM5001", "Try again"),
    ] {
        assert!(api_error_detail(code).contains(expected));
    }
    assert!(api_error_detail("NEW9999").contains("unavailable"));
    // Removed codes (DVS2008 and friends) fall through to the same fallback.
    assert!(api_error_detail("DVS2008").contains("unavailable"));
    assert!(api_error_detail("BIL1000").contains("Subscription success"));
}

#[test]
fn authentication_errors_have_safe_labels_and_preserve_codes() {
    for code in [
        "LGN2000", "LGN2001", "LGN2002", "LGN2003", "LGN2004", "LGN2005", "LGN2007", "LGN2009",
        "LGN3000",
    ] {
        let error = Error::Api {
            status: 401,
            code: code.into(),
            response: Box::new(serde_json::json!({"private": "secret"})),
        };
        let message = error.to_string();
        assert!(message.contains(code));
        assert!(!api_error_detail(code).is_empty());
        assert!(!message.contains("secret"));
    }
    assert!(api_error_detail("NEW9999").contains("unavailable"));
    assert!(api_error_detail("LGN2003").contains("unavailable"));
}
