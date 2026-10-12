use mockito::{Matcher, Server};
use ncell_cli::{api::Client, error::Error};
use serde_json::{Value, json};

#[test]
fn subscribe_uses_saved_device_and_catalog_metadata_and_never_retries_rejection() {
    let mut server = Server::new();
    let plan = json!({"techInfo":{"subscriptionCode":"5021027"},"ncellName":"FREE 100MB_NcellApp","productOfferingPrice":{"price":"0.00"}});
    let client = Client::with_base_url(&server.url()).unwrap();
    for (reply, success) in [
        (
            json!({"responseHeader":{"responseCode":"07","responseDescDisplay":"PSU2001"},"productSubscriptionSummaryResponse":{"status":"error"}}),
            false,
        ),
        (
            json!({"responseHeader":{"responseCode":"08","responseDescDisplay":"BIL2006","responseDesc":"Duplicate subscription request. Please wait before retrying.","requestId":"captured-request","timestamp":"2026-10-11T22:58:43.995"},"productSubscriptionSummaryResponse":{"product":"5021027","status":"error"}}),
            false,
        ),
        (
            json!({"responseHeader":{"responseCode":"00","responseDescDisplay":"BIL1000"},"productSubscriptionSummaryResponse":{"product":"5021027","status":"success"}}),
            true,
        ),
        (
            json!({"responseHeader":{"responseCode":"200"},"productSubscriptionSummaryResponse":{"status":"success"}}),
            true,
        ),
    ] {
        let request = server.mock("POST", "/v1.0/billingmgt/product/subscribe")
            .match_header("authorization", "Bearer test-token")
            .match_header("user-agent", Matcher::Missing)
            .match_header("connection", Matcher::Missing)
            .match_body(Matcher::PartialJson(json!({
                "productSubscriptionSummaryRequest":{
                    "deviceId":"saved-device", "msisdn":"9812345678", "subscriptionCode":"5021027",
                    "productName":"FREE 100MB_NcellApp", "ncellProductName":"FREE 100MB_NcellApp", "productPrice":"0.00"
                },
                "requestHeader":{"action":"SUBSCRIBE","deviceId":"saved-device","appVersion":"8.2.1.0"}
            })))
            .with_status(if reply["responseHeader"]["responseDescDisplay"] == "BIL2006" { 400 } else { 200 })
            .with_body(reply.to_string()).expect(1).create();
        let result = client.subscribe("9812345678", "test-token", "saved-device", &plan);
        if success {
            assert_eq!(
                result.unwrap()["productSubscriptionSummaryResponse"]["status"],
                "success"
            );
        } else {
            let Err(Error::Api {
                code,
                response,
                status,
            }) = result
            else {
                panic!("Expected API rejection")
            };
            assert_eq!(code, reply["responseHeader"]["responseDescDisplay"]);
            assert_eq!(*response, reply);
            assert_eq!(status, if code == "BIL2006" { 400 } else { 200 });
        }
        request.assert();
    }
    assert!(
        client
            .subscribe("9812345678", "test-token", "saved-device", &Value::Null)
            .is_err()
    );
    assert!(
        client
            .subscribe("9812345678", "test-token", "", &plan)
            .is_err()
    );
}

#[test]
fn billing_success_code_does_not_override_http_errors_or_other_endpoints() {
    let mut server = Server::new();
    let plan = json!({"techInfo":{"subscriptionCode":"5021027"},"ncellName":"Test","productOfferingPrice":{"price":"0.00"}});
    let client = Client::with_base_url(&server.url()).unwrap();
    let response = r#"{"responseHeader":{"responseCode":"00","responseDescDisplay":"BIL1000"},"productSubscriptionSummaryResponse":{"status":"success"}}"#;
    let subscribe = server
        .mock("POST", "/v1.0/billingmgt/product/subscribe")
        .with_status(400)
        .with_body(response)
        .create();
    assert!(
        client
            .subscribe("9812345678", "token", "device", &plan)
            .is_err()
    );
    subscribe.assert();
    let balance = server
        .mock("POST", "/v1.0/accountmgt/balance/query")
        .with_status(200)
        .with_body(response)
        .create();
    assert!(client.balance("9812345678", "token").is_err());
    balance.assert();
}
