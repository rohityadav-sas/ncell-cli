use mockito::{Matcher, Server};
use ncell_cli::api::Client;
use serde_json::json;

#[test]
fn profile_keeps_required_version_and_omits_optional_fields() {
    let mut server = Server::new();
    let request = server.mock("POST", "/v1.0/subscriber/profile/query")
        .match_header("authorization", "Bearer test-token")
        .match_header("content-type", "application/json")
        .match_header("user-agent", Matcher::Missing)
        .match_header("connection", Matcher::Missing)
        .match_body(Matcher::PartialJson(json!({
            "querySubscriberProfileRequest": {},
            "requestHeader": { "action": "PROFILE", "appVersion": "8.2.1.0" },
        })))
        .match_request(|request| {
            let body: serde_json::Value = serde_json::from_slice(request.body().unwrap()).unwrap();
            body["querySubscriberProfileRequest"].as_object().unwrap().is_empty()
                && body["requestHeader"].as_object().unwrap().len() == 11
        })
        .with_status(200).with_body(r#"{"responseHeader":{"responseCode":"200"},"querySubscriberProfileResponse":{"accountName":"Test","simType":"4G","status":"B04"}}"#)
        .expect(1).create();
    let profile = Client::with_base_url(&server.url())
        .unwrap()
        .profile("9812345678", "test-token")
        .unwrap();
    assert_eq!(profile["accountName"], "Test");
    request.assert();
}
