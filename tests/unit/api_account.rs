use super::*;

#[test]
fn expired_catalog_cache_is_refetched() {
    let mut server = mockito::Server::new();
    let tags = server
        .mock("POST", "/v1.0/product-catalog/tags")
        .with_body(r#"{"responseHeader":{"responseCode":"200"},"tags":[]}"#)
        .expect(1)
        .create();
    let client = Client::with_base_url(&server.url()).unwrap();
    client.categories_cache.lock().unwrap().insert(
        "9811111111".into(),
        (
            std::time::Instant::now() - std::time::Duration::from_secs(601),
            json!({"tags":[{"id":999}]}),
        ),
    );
    assert_eq!(
        client.categories("9811111111", "token").unwrap()["tags"],
        json!([])
    );
    tags.assert();
}

#[test]
fn catalog_cache_is_shared_by_client_clones_but_isolated_by_account() {
    let mut server = mockito::Server::new();
    let tags = server
        .mock("POST", "/v1.0/product-catalog/tags")
        .with_body(r#"{"responseHeader":{"responseCode":"200"},"tags":[]}"#)
        .expect(2)
        .create();
    let client = Client::with_base_url(&server.url()).unwrap();
    client.categories("9811111111", "token-a").unwrap();
    client.clone().categories("9811111111", "token-a").unwrap();
    client.categories("9822222222", "token-b").unwrap();
    tags.assert();
}

#[test]
fn rejected_catalog_responses_are_not_cached() {
    let mut server = mockito::Server::new();
    let tags = server
        .mock("POST", "/v1.0/product-catalog/tags")
        .with_status(401)
        .with_body(r#"{"responseHeader":{"responseCode":"401","responseDescDisplay":"LGN2001"}}"#)
        .expect(2)
        .create();
    let client = Client::with_base_url(&server.url()).unwrap();
    assert!(client.categories("9811111111", "expired").is_err());
    assert!(client.categories("9811111111", "expired").is_err());
    tags.assert();
}

#[test]
fn check_in_preserves_encoded_identity_and_rejects_missing_identity() {
    let mut server = mockito::Server::new();
    let client = Client::with_base_url(&server.url()).unwrap();
    for encrypted in [json!("abc%2Bdef%2Fghi%3D"), Value::Null] {
        let request = server
            .mock("POST", "/v1.0/utilitymgt/third-party-encryption")
            .match_header("authorization", "Bearer test-token")
            .match_body(mockito::Matcher::Json(json!({
                "thirdPartyClientRequest": {"clientId": "47c0b8f2-1766-c12a-5c88-519e40af83b4"},
                "requestHeader": {"msisdn": "9800000000"},
            })))
            .with_body(
                json!({
                    "responseHeader": {"responseCode": "200"},
                    "encryptionResponse": {"msisdn": encrypted},
                })
                .to_string(),
            )
            .create();
        let result = client.check_in_url("9800000000", "test-token");
        if let Some(encrypted) = encrypted.as_str() {
            assert_eq!(
                result.unwrap(),
                format!("https://sajeeb.com.np/ncellapp/checkin/index.php?MSISDN={encrypted}")
            );
        } else {
            assert!(matches!(result, Err(Error::InvalidResponse)));
        }
        request.assert();
    }
}
