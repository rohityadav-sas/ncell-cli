use mockito::{Matcher, Server};
use ncell_cli::{
    api::Client,
    auth::{AuthService, normalize_number},
    error::{Error, Result},
    session::{Session, SessionStore},
};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};

fn saved_account() -> MemoryStore {
    let store = MemoryStore::default();
    store
        .save(&Session {
            device_id: "same-device".into(),
            account: Some(ncell_cli::session::Account {
                msisdn: "9812345678".into(),
                access_token: "old-access".into(),
                refresh_token: Some("old-refresh".into()),
                authenticated_at: chrono::Utc::now(),
                token_saved_at: None,
                access_token_expires_at: None,
            }),
            ..Session::default()
        })
        .unwrap();
    store
}

#[test]
fn automatic_refresh_saves_rotated_tokens_and_retries_once() {
    let code = "LGN2001";
    let mut server = Server::new();
    let store = saved_account();
    let expired = server
        .mock("POST", "/v1.0/accountmgt/balance/query")
        .match_header("authorization", "Bearer old-access")
        .with_status(401)
        .with_body(
            json!({"responseHeader":{"responseCode":"401","responseDescDisplay":code}}).to_string(),
        )
        .expect(1)
        .create();
    let refresh = server.mock("POST", "/v1.0/user/refresh/token")
            .match_body(Matcher::PartialJson(json!({"requestHeader":{"deviceId":"same-device"},"refreshTokenRequest":{"refreshToken":"old-refresh"}})))
            .with_body(r#"{"responseHeader":{"responseCode":"200"},"userAuthResponse":{"accessToken":"new-access","refreshToken":"new-refresh"}}"#)
            .expect(1).create();
    let retried = server.mock("POST", "/v1.0/accountmgt/balance/query")
            .match_header("authorization", "Bearer new-access")
            .with_body(r#"{"responseHeader":{"responseCode":"200"},"queryBalanceResponse":{"paidMode":"Prepaid"}}"#)
            .expect(1).create();
    let auth = AuthService::new(Client::with_base_url(&server.url()).unwrap(), store.clone());
    let result = auth
        .authenticated(|client, account, _| {
            // The retry must see tokens already persisted in the store.
            assert_eq!(
                store.load().unwrap().unwrap().account.unwrap().access_token,
                account.access_token
            );
            client.balance(&account.msisdn, &account.access_token)
        })
        .unwrap();
    assert_eq!(result["paidMode"], "Prepaid");
    assert_eq!(
        store
            .load()
            .unwrap()
            .unwrap()
            .account
            .unwrap()
            .refresh_token
            .as_deref(),
        Some("new-refresh")
    );
    expired.assert();
    refresh.assert();
    retried.assert();
}

#[test]
fn automatic_refresh_does_not_loop_or_retry_other_rejections() {
    for (status, code, should_refresh) in [
        (401, "LGN2001", true),
        (401, "LGN2002", false),
        (401, "LGN2003", false),
        (400, "BIL2006", false),
        (500, "LGN2001", false),
    ] {
        let mut server = Server::new();
        let store = saved_account();
        let query = server.mock("POST", "/v1.0/accountmgt/balance/query")
            .with_status(status)
            .with_body(json!({"responseHeader":{"responseCode":status.to_string(),"responseDescDisplay":code}}).to_string())
            .expect(if should_refresh {2} else {1}).create();
        let refresh = server.mock("POST", "/v1.0/user/refresh/token")
            .with_body(r#"{"responseHeader":{"responseCode":"200"},"userAuthResponse":{"accessToken":"new-access","refreshToken":"new-refresh"}}"#)
            .expect(usize::from(should_refresh)).create();
        assert!(
            AuthService::new(Client::with_base_url(&server.url()).unwrap(), store)
                .authenticated(
                    |client, account, _| client.balance(&account.msisdn, &account.access_token)
                )
                .is_err()
        );
        query.assert();
        refresh.assert();
    }
}

#[test]
fn automatic_refresh_failure_preserves_session_and_does_not_replay() {
    let mut server = Server::new();
    let store = saved_account();
    let before = store.0.borrow().clone();
    let query = server
        .mock("POST", "/v1.0/accountmgt/balance/query")
        .with_status(401)
        .with_body(r#"{"responseHeader":{"responseCode":"401","responseDescDisplay":"LGN2001"}}"#)
        .expect(1)
        .create();
    let refresh = server
        .mock("POST", "/v1.0/user/refresh/token")
        .with_status(401)
        .with_body(r#"{"responseHeader":{"responseCode":"401","responseDescDisplay":"LGN2004"}}"#)
        .expect(1)
        .create();
    assert!(
        AuthService::new(Client::with_base_url(&server.url()).unwrap(), store.clone())
            .authenticated(
                |client, account, _| client.balance(&account.msisdn, &account.access_token)
            )
            .is_err()
    );
    assert_eq!(*store.0.borrow(), before);
    query.assert();
    refresh.assert();
}

#[derive(Clone, Default)]
struct MemoryStore(Rc<RefCell<Option<String>>>);
impl SessionStore for MemoryStore {
    fn load(&self) -> Result<Option<Session>> {
        Ok(self
            .0
            .borrow()
            .as_ref()
            .map(|s| serde_json::from_str(s).unwrap()))
    }
    fn save(&self, session: &Session) -> Result<()> {
        *self.0.borrow_mut() = Some(serde_json::to_string(session).unwrap());
        Ok(())
    }
    fn clear(&self) -> Result<()> {
        *self.0.borrow_mut() = None;
        Ok(())
    }
}
fn generated(server: &mut Server) -> mockito::Mock {
    server.mock("POST", "/v1.0/user/otp/generate")
        .match_header("content-type", "application/json")
        .match_header("user-agent", Matcher::Missing)
        .match_header("connection", Matcher::Missing)
        .match_request(|request| {
            let body: serde_json::Value = serde_json::from_slice(request.body().unwrap()).unwrap();
            body["requestHeader"].as_object().is_some_and(|h| h.len() == 10)
                && body["generateOTPRequest"].as_object().is_some_and(|r| r.len() == 3)
        })
        .match_body(Matcher::PartialJson(json!({"generateOTPRequest":{"msisdn":"9812345678","action":"LOGIN"},"requestHeader":{"action":"LOGIN","channel":"sca","msisdn":"9812345678"}})))
        .with_status(200).with_body(r#"{"responseHeader":{"responseCode":"200"},"generateOTPResponse":{}}"#)
        .expect(1).create()
}

#[test]
fn login_uses_nested_otp_and_persists_tokens_with_same_device() {
    let mut server = Server::new();
    let generate = generated(&mut server);
    let store = MemoryStore::default();
    let auth = AuthService::new(Client::with_base_url(&server.url()).unwrap(), store.clone());
    auth.request_otp("+9779812345678").unwrap();
    let device = store.load().unwrap().unwrap().device_id;
    assert_eq!(device.len(), 16);
    let validate = server.mock("POST", "/v1.0/user/otp/validate")
        .match_request(|request| {
            let body: serde_json::Value = serde_json::from_slice(request.body().unwrap()).unwrap();
            body["requestHeader"].as_object().is_some_and(|h| h.len() == 10)
        })
        .match_body(Matcher::PartialJson(json!({"validateOTPRequest":{"msisdn":"9812345678","deviceId":device,"otpDetail":{"action":"LOGIN","otp":"001234"}},"requestHeader":{"action":"LOGIN","msisdn":"9812345678"}})))
        .with_status(200).with_body(r#"{"responseHeader":{"responseCode":200},"validateOTPResponse":{"accessToken":"test-access","refreshToken":"test-refresh"}}"#)
        .expect(1).create();
    auth.verify_otp("9812345678", "001234").unwrap();
    let session = store.load().unwrap().unwrap();
    assert_eq!(session.device_id, device);
    assert!(session.pending_msisdn.is_none());
    let account = session.account.unwrap();
    assert_eq!(account.access_token, "test-access");
    assert_eq!(account.refresh_token.as_deref(), Some("test-refresh"));
    generate.assert();
    validate.assert();
}

#[test]
fn alternate_user_auth_response_is_supported() {
    let mut server = Server::new();
    let validate = server.mock("POST", "/v1.0/user/otp/validate")
        .with_status(200)
        .with_body(r#"{"responseHeader":{"responseCode":"200"},"userAuthResponse":{"accessToken":"test-token"}}"#)
        .create();
    let tokens = Client::with_base_url(&server.url())
        .unwrap()
        .validate_otp("9812345678", "device", "123456")
        .unwrap();
    assert_eq!(tokens.access_token, "test-token");
    validate.assert();
}

#[test]
fn business_rejection_on_http_200_preserves_pending_session() {
    let mut server = Server::new();
    let generate = generated(&mut server);
    let validate = server
        .mock("POST", "/v1.0/user/otp/validate")
        .with_status(200)
        .with_body(r#"{"responseHeader":{"responseCode":"401","responseDescDisplay":"expired"}}"#)
        .expect(1)
        .create();
    let store = MemoryStore::default();
    let auth = AuthService::new(Client::with_base_url(&server.url()).unwrap(), store.clone());
    auth.request_otp("9812345678").unwrap();
    assert!(matches!(
        auth.verify_otp("9812345678", "123456"),
        Err(Error::Api { .. })
    ));
    let session = store.load().unwrap().unwrap();
    assert!(session.account.is_none());
    assert_eq!(session.pending_msisdn.as_deref(), Some("9812345678"));
    generate.assert();
    validate.assert();
}

#[test]
fn failed_validation_does_not_overwrite_existing_account() {
    let mut server = Server::new();
    let validate = server
        .mock("POST", "/v1.0/user/otp/validate")
        .with_status(400)
        .with_body(r#"{"responseHeader":{"responseCode":"400"}}"#)
        .expect(1)
        .create();
    let store = MemoryStore::default();
    store
        .save(&Session {
            device_id: "0123456789abcdef".into(),
            other_accounts: Vec::new(),
            pending_msisdn: Some("9812345678".into()),
            account: Some(ncell_cli::session::Account {
                msisdn: "9823456789".into(),
                access_token: "old-access".into(),
                refresh_token: None,
                authenticated_at: chrono::Utc::now(),
                token_saved_at: None,
                access_token_expires_at: None,
            }),
        })
        .unwrap();
    let auth = AuthService::new(Client::with_base_url(&server.url()).unwrap(), store.clone());
    assert!(auth.verify_otp("9812345678", "123456").is_err());
    assert_eq!(
        store.load().unwrap().unwrap().account.unwrap().access_token,
        "old-access"
    );
    validate.assert();
}

#[test]
fn missing_or_empty_tokens_are_not_a_successful_login() {
    for response in [
        r#"{"responseHeader":{"responseCode":"200"}}"#,
        r#"{"responseHeader":{"responseCode":"200"},"validateOTPResponse":{"accessToken":""}}"#,
    ] {
        let mut server = Server::new();
        let mock = server
            .mock("POST", "/v1.0/user/otp/validate")
            .with_status(200)
            .with_body(response)
            .create();
        assert!(matches!(
            Client::with_base_url(&server.url()).unwrap().validate_otp(
                "9812345678",
                "device",
                "123456"
            ),
            Err(Error::InvalidResponse)
        ));
        mock.assert();
    }
}

#[test]
fn invalid_input_and_wrong_account_never_send_requests() {
    let server = Server::new();
    let store = MemoryStore::default();
    let auth = AuthService::new(Client::with_base_url(&server.url()).unwrap(), store.clone());
    assert!(auth.request_otp("bad").is_err());
    assert!(matches!(
        auth.verify_otp("9812345678", "123456"),
        Err(Error::NoPendingLogin)
    ));
    store
        .save(&Session {
            device_id: "device".into(),
            other_accounts: Vec::new(),
            pending_msisdn: Some("9823456789".into()),
            account: None,
        })
        .unwrap();
    assert!(matches!(
        auth.verify_otp("9812345678", "123456"),
        Err(Error::NoPendingLogin)
    ));
    assert!(auth.verify_otp("9823456789", "abc").is_err());
    assert_eq!(normalize_number("+9779812345678").unwrap(), "9812345678");
}

#[test]
fn malformed_response_and_connection_failure_are_safe_errors() {
    let mut server = Server::new();
    let mock = server
        .mock("POST", "/v1.0/user/otp/generate")
        .with_status(502)
        .with_body("<html>private diagnostics</html>")
        .create();
    let error = Client::with_base_url(&server.url())
        .unwrap()
        .generate_otp("9812345678", "device")
        .unwrap_err();
    assert!(!error.to_string().contains("private"));
    mock.assert();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    assert!(matches!(
        Client::with_base_url(&format!("http://{address}"))
            .unwrap()
            .generate_otp("9812345678", "device"),
        Err(Error::Network | Error::Connection | Error::Timeout)
    ));
}

#[test]
fn refresh_rotates_tokens_and_keeps_device_identity() {
    let mut server = Server::new();
    let store = MemoryStore::default();
    store
        .save(&Session {
            device_id: "same-device".into(),
            other_accounts: Vec::new(),
            pending_msisdn: None,
            account: Some(ncell_cli::session::Account {
                msisdn: "9812345678".into(),
                access_token: "old-access".into(),
                refresh_token: Some("old-refresh".into()),
                authenticated_at: chrono::Utc::now(),
                token_saved_at: None,
                access_token_expires_at: None,
            }),
        })
        .unwrap();
    let request = server.mock("POST", "/v1.0/user/refresh/token")
        .match_body(Matcher::PartialJson(json!({
            "refreshTokenRequest": { "refreshToken": "old-refresh" },
            "requestHeader": { "deviceId": "same-device", "action": "LOGOUT_ACTION", "msisdn": "9812345678" },
        })))
        .with_status(200).with_body(r#"{"responseHeader":{"responseCode":"200"},"userAuthResponse":{"accessToken":"new-access","refreshToken":"new-refresh"}}"#)
        .expect(1).create();
    AuthService::new(Client::with_base_url(&server.url()).unwrap(), store.clone())
        .refresh()
        .unwrap();
    let session = store.load().unwrap().unwrap();
    assert_eq!(session.device_id, "same-device");
    let account = session.account.unwrap();
    assert_eq!(account.access_token, "new-access");
    assert_eq!(account.refresh_token.as_deref(), Some("new-refresh"));
    request.assert();
}

#[test]
fn failed_refresh_preserves_saved_tokens() {
    let mut server = Server::new();
    let store = MemoryStore::default();
    store
        .save(&Session {
            device_id: "same-device".into(),
            other_accounts: Vec::new(),
            pending_msisdn: None,
            account: Some(ncell_cli::session::Account {
                msisdn: "9812345678".into(),
                access_token: "old-access".into(),
                refresh_token: Some("old-refresh".into()),
                authenticated_at: chrono::Utc::now(),
                token_saved_at: None,
                access_token_expires_at: None,
            }),
        })
        .unwrap();
    let before = store.0.borrow().clone();
    let request = server
        .mock("POST", "/v1.0/user/refresh/token")
        .with_status(401)
        .with_body(r#"{"responseHeader":{"responseCode":"401","responseDescDisplay":"LGN2004"}}"#)
        .expect(1)
        .create();
    assert!(
        AuthService::new(Client::with_base_url(&server.url()).unwrap(), store.clone())
            .refresh()
            .is_err()
    );
    assert_eq!(*store.0.borrow(), before);
    request.assert();
}

fn add_other_account(store: &MemoryStore) {
    let mut session = store.load().unwrap().unwrap();
    session
        .other_accounts
        .push(ncell_cli::session::SavedAccount {
            device_id: "other-device".into(),
            account: ncell_cli::session::Account {
                msisdn: "9822222222".into(),
                access_token: "other-access".into(),
                refresh_token: Some("other-refresh".into()),
                authenticated_at: chrono::Utc::now(),
                token_saved_at: None,
                access_token_expires_at: None,
            },
        });
    store.save(&session).unwrap();
}

#[test]
fn refresh_only_rotates_active_account_and_preserves_other_identity() {
    let mut server = Server::new();
    let store = saved_account();
    add_other_account(&store);
    let refresh = server.mock("POST", "/v1.0/user/refresh/token")
        .match_body(Matcher::PartialJson(json!({"refreshTokenRequest":{"refreshToken":"old-refresh"},"requestHeader":{"msisdn":"9812345678","deviceId":"same-device"}})))
        .with_body(json!({"responseHeader":{"responseCode":"200"},"userAuthResponse":{"accessToken":"rotated-access","refreshToken":"rotated-refresh","expiresIn":"2026-10-13T01:00:00"}}).to_string()).expect(1).create();
    let auth = AuthService::new(Client::with_base_url(&server.url()).unwrap(), store.clone());
    auth.refresh().unwrap();
    let mut session = store.load().unwrap().unwrap();
    assert_eq!(
        session.account.as_ref().unwrap().access_token,
        "rotated-access"
    );
    assert_eq!(
        session.other_accounts[0].account.access_token,
        "other-access"
    );
    session.switch("9822222222").unwrap();
    store.save(&session).unwrap();
    auth.authenticated(|_, account, device| {
        assert_eq!(account.access_token, "other-access");
        assert_eq!(device, "other-device");
        Ok(())
    })
    .unwrap();
    let mut session = store.load().unwrap().unwrap();
    session.switch("9812345678").unwrap();
    assert_eq!(
        session.account.unwrap().refresh_token.as_deref(),
        Some("rotated-refresh")
    );
    refresh.assert();
}

#[test]
fn failed_refresh_preserves_all_accounts() {
    let mut server = Server::new();
    let store = saved_account();
    add_other_account(&store);
    let before = store.0.borrow().clone();
    let request = server
        .mock("POST", "/v1.0/user/refresh/token")
        .with_status(401)
        .with_body(r#"{"responseHeader":{"responseCode":"401","responseDescDisplay":"LGN2004"}}"#)
        .expect(1)
        .create();
    assert!(
        AuthService::new(Client::with_base_url(&server.url()).unwrap(), store.clone())
            .refresh()
            .is_err()
    );
    assert_eq!(*store.0.borrow(), before);
    request.assert();
}

#[test]
fn adding_login_keeps_old_account_and_failed_verification_preserves_it() {
    let mut server = Server::new();
    let store = saved_account();
    add_other_account(&store);
    let generate = server
        .mock("POST", "/v1.0/user/otp/generate")
        .with_body(r#"{"responseHeader":{"responseCode":"200"}}"#)
        .expect(1)
        .create();
    let auth = AuthService::new(Client::with_base_url(&server.url()).unwrap(), store.clone());
    auth.request_otp("9833333333").unwrap();
    let before = store.0.borrow().clone();
    let failed = server
        .mock("POST", "/v1.0/user/otp/validate")
        .with_status(400)
        .with_body(r#"{"responseHeader":{"responseCode":"400","responseDescDisplay":"OTP2006"}}"#)
        .expect(1)
        .create();
    assert!(auth.verify_otp("9833333333", "123456").is_err());
    assert_eq!(*store.0.borrow(), before);
    failed.assert();
    failed.remove();
    let valid = server.mock("POST", "/v1.0/user/otp/validate")
        .with_body(r#"{"responseHeader":{"responseCode":"200"},"validateOTPResponse":{"accessToken":"third-access","refreshToken":"third-refresh"}}"#).expect(1).create();
    auth.verify_otp("9833333333", "123456").unwrap();
    let mut session = store.load().unwrap().unwrap();
    assert_eq!(session.account.as_ref().unwrap().msisdn, "9833333333");
    assert_eq!(session.other_accounts.len(), 2);
    assert!(session.pending_msisdn.is_none());
    session.switch("9812345678").unwrap();
    assert_eq!(session.account.unwrap().access_token, "old-access");
    generate.assert();
    valid.assert();
}

#[test]
fn add_verification_preserves_active_and_resumes_after_wrong_otp() {
    let mut server = Server::new();
    let store = saved_account();
    add_other_account(&store);
    let mut session = store.load().unwrap().unwrap();
    session.pending_msisdn = Some("9833333333".into());
    store.save(&session).unwrap();
    let auth = AuthService::new(Client::with_base_url(&server.url()).unwrap(), store.clone());
    let before = store.0.borrow().clone();
    let wrong = server
        .mock("POST", "/v1.0/user/otp/validate")
        .with_status(400)
        .with_body(r#"{"responseHeader":{"responseCode":"400"}}"#)
        .expect(1)
        .create();
    assert!(auth.verify_account("9833333333", "123456", false).is_err());
    assert_eq!(*store.0.borrow(), before);
    wrong.assert();
    wrong.remove();
    let valid=server.mock("POST","/v1.0/user/otp/validate").with_body(r#"{"responseHeader":{"responseCode":"200"},"validateOTPResponse":{"accessToken":"new-access","refreshToken":"new-refresh"}}"#).expect(1).create();
    auth.verify_account("9833333333", "654321", false).unwrap();
    let session = store.load().unwrap().unwrap();
    assert_eq!(session.account.unwrap().msisdn, "9812345678");
    assert_eq!(session.other_accounts.len(), 2);
    assert!(session.pending_msisdn.is_none());
    valid.assert();
}
