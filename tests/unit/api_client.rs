use super::*;

#[test]
fn response_codes_with_short_and_long_prefixes_keep_their_meanings() {
    let mut server = mockito::Server::new();
    let client = Client::with_base_url(&server.url()).unwrap();
    for (code, meaning) in [("CL3001", "Loan request failed"), ("BSAM5001", "Try again")] {
        let request = server
            .mock("POST", "/test")
            .with_status(400)
            .with_body(
                serde_json::json!({"responseHeader": {
                    "responseCode": "08", "responseDescDisplay": code
                }})
                .to_string(),
            )
            .create();
        let error = client
            .post("/test", &serde_json::json!({}), None)
            .unwrap_err();
        assert!(error.to_string().contains(code));
        assert!(error.to_string().contains(meaning));
        request.assert();
    }
}
