use mockito::{Matcher, Server};
use ncell_cli::{api::Client, error::Error};
use serde_json::json;

#[test]
fn balance_uses_bearer_and_minimum_payload() {
    let mut server = Server::new();
    let request = server.mock("POST", "/v1.0/accountmgt/balance/query")
        .match_header("authorization", "Bearer test-token")
        .match_header("content-type", "application/json")
        .match_header("user-agent", Matcher::Missing)
        .match_header("connection", Matcher::Missing)
        .match_body(Matcher::PartialJson(json!({
            "queryBalanceRequest": { "msisdn": "9812345678" },
            "requestHeader": { "msisdn": "9812345678", "action": "MY_SERVICES" },
        })))
        .match_request(|request| {
            let body: serde_json::Value = serde_json::from_slice(request.body().unwrap()).unwrap();
            body["queryBalanceRequest"].as_object().unwrap().len() == 1
                && body["requestHeader"].as_object().unwrap().len() == 10
        })
        .with_status(200).with_body(r#"{"responseHeader":{"responseCode":"200"},"queryBalanceResponse":{"creditBalanceDetail":{"balance":58.42,"creditUom":"Rs."}}}"#)
        .expect(1).create();
    let response = Client::with_base_url(&server.url())
        .unwrap()
        .balance("9812345678", "test-token")
        .unwrap();
    assert_eq!(response["creditBalanceDetail"]["balance"], 58.42);
    request.assert();
}

#[test]
fn expired_token_is_reported_without_retrying_or_rotating_it() {
    let mut server = Server::new();
    let request = server
        .mock("POST", "/v1.0/accountmgt/balance/query")
        .with_status(401)
        .with_body(r#"{"responseHeader":{"responseCode":"401","responseDescDisplay":"LGN2002"}}"#)
        .expect(1)
        .create();
    assert!(matches!(
        Client::with_base_url(&server.url())
            .unwrap()
            .balance("9812345678", "expired-token"),
        Err(Error::Api { status: 401, .. })
    ));
    request.assert();
}
