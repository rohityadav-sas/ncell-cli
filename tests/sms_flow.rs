use mockito::{Matcher, Server};
use ncell_cli::{api::Client, error::Error};
use serde_json::json;

#[test]
fn free_sms_matches_capture_and_preserves_text() {
    let mut server = Server::new();
    let response = json!({
        "responseHeader": {"responseCode": "200", "responseDescDisplay": "SMS1000"},
        "sendFreeSMSResponse": {"description": "Operation succeeded", "status": "success", "statusCode": "0"},
    });
    let request = server.mock("POST", "/v1.0/smsmgt/free/sms/send")
        .match_header("authorization", "Bearer test-token")
        .match_header("content-type", "application/json")
        .match_header("user-agent", Matcher::Missing)
        .match_body(Matcher::PartialJson(json!({
            "sendSMSFreeRequest": {"destination": "9812345679", "content": "Hello \"friend\"\nनमस्ते", "isConfirm": 0},
            "requestHeader": {"action": "MY_SERVICES"},
        })))
        .match_request(|request| {
            let body: serde_json::Value = serde_json::from_slice(request.body().unwrap()).unwrap();
            body["sendSMSFreeRequest"].as_object().unwrap().len() == 3
                && body["requestHeader"].as_object().unwrap().len() == 10
        })
        .with_status(200).with_body(response.to_string()).expect(1).create();
    let client = Client::with_base_url(&server.url()).unwrap();
    assert_eq!(
        client
            .send_free_sms(
                "9812345678",
                "test-token",
                "+9779812345679",
                "Hello \"friend\"\nनमस्ते"
            )
            .unwrap(),
        response
    );
    assert!(matches!(
        client.send_free_sms("9812345678", "test-token", "9812345679", "  "),
        Err(Error::InvalidInput(_))
    ));
    request.assert();
}

#[test]
fn free_sms_rejection_keeps_server_response_without_retry() {
    let mut server = Server::new();
    let response =
        json!({"responseHeader": {"responseCode": "400", "responseDescDisplay": "SMS2001"}});
    let request = server
        .mock("POST", "/v1.0/smsmgt/free/sms/send")
        .with_status(400)
        .with_body(response.to_string())
        .expect(1)
        .create();
    let client = Client::with_base_url(&server.url()).unwrap();
    match client.send_free_sms("9812345678", "test-token", "9812345679", "Hi") {
        Err(Error::Api { response: body, .. }) => assert_eq!(*body, response),
        _ => panic!("Expected server rejection"),
    }
    request.assert();
}
