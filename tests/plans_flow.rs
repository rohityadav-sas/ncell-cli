use mockito::{Matcher, Server};
use ncell_cli::api::Client;
use serde_json::{Value, json};

#[test]
fn search_sends_keyword_to_server_without_extra_metadata() {
    let mut server = Server::new();
    let request = server
        .mock("POST", "/v1.0/product/data-plans")
        .match_header("authorization", "Bearer test-token")
        .match_body(Matcher::PartialJson(
            json!({"keyword":"Super", "tagId":66, "sortby":{"name":"price","isDescending":false}}),
        ))
        .match_request(|r| {
            let body: Value = serde_json::from_slice(r.body().unwrap()).unwrap();
            body.as_object().unwrap().len() == 6
                && body["requestHeader"].as_object().unwrap().len() == 10
        })
        .with_body(r#"{"responseHeader":{"responseCode":"200"},"availablePackages":[]}"#)
        .expect(1)
        .create();
    let response = Client::with_base_url(&server.url())
        .unwrap()
        .search_plans("9812345678", "test-token", 66, "Super")
        .unwrap();
    assert_eq!(response["availablePackages"], json!([]));
    request.assert();
}

#[test]
fn plans_preserve_catalog_selection_and_reject_missing_packages() {
    let mut server = Server::new();
    for (response, valid) in [
        (
            json!({"responseHeader":{"responseCode":"200"},"availablePackages":[],"defaultCategory":{"id":"2"}}),
            true,
        ),
        (json!({"responseHeader":{"responseCode":"200"}}), false),
    ] {
        let request = server
            .mock("POST", "/v1.0/product/data-plans")
            .match_header("authorization", "Bearer test-token")
            .match_header("user-agent", Matcher::Missing)
            .match_header("connection", Matcher::Missing)
            .match_body(Matcher::PartialJson(json!({
                "msisdn":"9812345678", "languageCode":"1", "tagId":66,
                "requestHeader":{"action":"SHOP"}
            })))
            .match_request(|r| {
                let body: Value = serde_json::from_slice(r.body().unwrap()).unwrap();
                body.as_object().unwrap().len() == 5
                    && body["requestHeader"].as_object().unwrap().len() == 10
                    && body["sortby"] == json!({"name":"price","isDescending":false})
            })
            .with_status(200)
            .with_body(response.to_string())
            .create();
        let result =
            Client::with_base_url(&server.url())
                .unwrap()
                .plans("9812345678", "test-token", 66);
        assert_eq!(result.is_ok(), valid);
        request.assert();
    }
}

#[test]
fn tags_omit_version_and_route_sms_and_voice_without_data_fallback() {
    let mut server = Server::new();
    let tags = server
        .mock("POST", "/v1.0/product-catalog/tags")
        .match_header("authorization", "Bearer test-token")
        .match_body(Matcher::PartialJson(
            json!({"requestHeader":{"action":"N/A"}}),
        ))
        .match_request(|r| {
            let b: Value = serde_json::from_slice(r.body().unwrap()).unwrap();
            b.as_object().unwrap().len() == 1 && b["requestHeader"].as_object().unwrap().len() == 10
        })
        .with_body(
            json!({"responseHeader":{"responseCode":"200"},"tags":[
                {"id":50,"categories":[{"name":"GSM_SMS"}]},
                {"id":61,"categories":[{"name":"GSM_VOICE"}]}
            ]})
            .to_string(),
        )
        .expect(1)
        .create();
    let client = Client::with_base_url(&server.url()).unwrap();
    for (tag, catalog) in [(50, "sms"), (61, "voice")] {
        let request = server
            .mock("POST", format!("/v1.0/product/{catalog}-plans").as_str())
            .match_body(Matcher::PartialJson(
                json!({"tagId":tag,"sortby":{"name":"price","isDescending":false}}),
            ))
            .with_body(r#"{"responseHeader":{"responseCode":"200"},"availablePackages":[]}"#)
            .create();
        client.plans("9812345678", "test-token", tag).unwrap();
        request.assert();
    }
    assert!(client.plans("9812345678", "test-token", 9999).is_err());
    tags.assert();
}
