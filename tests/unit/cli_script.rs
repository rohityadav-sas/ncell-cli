use super::*;
#[test]
fn uses_system_editor_and_preserves_created_file_on_launch_failure() {
    #[cfg(target_os = "windows")]
    assert_eq!(editor_arguments(), ["notepad.exe"]);
    #[cfg(target_os = "macos")]
    assert_eq!(editor_arguments(), ["open", "-t", "-W"]);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        // Which binary exists depends on the machine (CI images ship nano
        // and vi), so only the probe order is asserted, not one fixed editor.
        let actual = editor_arguments();
        assert_eq!(actual.len(), 1);
        assert!(
            ["nano", "vi", "xdg-open"].contains(&actual[0].as_str()),
            "unexpected editor: {actual:?}"
        );
    }
    let file = std::env::temp_dir().join(format!("ncell-{}.ncell", uuid::Uuid::new_v4()));
    let mut opened = false;
    assert!(
        create_file(&file, |saved| {
            opened = true;
            assert_eq!(fs::read_to_string(saved).unwrap(), EXAMPLE);
            Err(fail("Simulated editor launch failure"))
        })
        .is_err()
    );
    assert!(opened);
    assert!(create_file(&file, |_| panic!("Must not open an overwritten script")).is_err());
    assert_eq!(fs::read_to_string(&file).unwrap(), EXAMPLE);
    fs::remove_file(file).unwrap();
}

#[test]
fn parses_every_command_family_and_literal_quoted_arguments() {
    let source = "# comment\n\nlogin 9812345678 --verify\naccount list\nstatus\nlogout\nbalance\nprofile\nplans\nplans categories\nplans search \"SMS pack\"\nplans subscribe 123\nsms send-free 9812345678 \"hello $HOME | literal\"\nrefresh\ngame status\ngame spin --count all\ngame claim-task all\n";
    let steps = parse(source).unwrap();
    assert_eq!(steps.len(), 15);
    assert!(
        matches!(&steps[10].args.command, Some(Command::Sms { action: super::super::SmsCommand::SendFree { text, .. } }) if text == "hello $HOME | literal")
    );
    assert_eq!(
        words("sms send-free 9812345678 'C:\\my files\\hi'").unwrap()[3],
        "C:\\my files\\hi"
    );
    assert_eq!(words("sms send-free 9812345678 \"\"").unwrap()[3], "");
    assert_eq!(parse(EXAMPLE).unwrap().len(), 14);
}
#[test]
fn validates_every_line_before_execution() {
    for bad in [
        "script run other",
        "script create other",
        "interactive",
        "ncell interactive --json",
        "balance | more",
        "balance > file",
        "$(bad)",
        "game spin --count 0",
        "game claim-task all,1",
        "sms send-free 9812345678 'unclosed",
        "unknown",
    ] {
        let error = match parse(&format!("balance\n{bad}")) {
            Err(e) => e,
            Ok(_) => panic!("Accepted {bad}"),
        };
        assert!(error.to_string().contains("Line 2"));
    }
}
#[test]
fn stops_on_first_failed_step() {
    let steps = parse("balance\nprofile\nstatus").unwrap();
    let mut calls = 0;
    let error = execute(&steps, true, |_, _, _, _| {
        calls += 1;
        Err(Error::Network)
    })
    .unwrap_err();
    assert_eq!(calls, 1);
    assert!(error.to_string().contains("Step 1 of 3 (line 1)"));
}
#[test]
fn runner_orders_mocked_sms_and_purchase_and_stops_on_rejection() {
    let mut server = mockito::Server::new();
    let sms = server.mock("POST", "/v1.0/smsmgt/free/sms/send")
            .match_body(mockito::Matcher::PartialJson(json!({"sendSMSFreeRequest":{"content":"hello friend"}})))
            .with_body(json!({"responseHeader":{"responseCode":"200"},"sendFreeSMSResponse":{"status":"success"}}).to_string())
            .expect(1).create();
    let purchase = server
        .mock("POST", "/v1.0/billingmgt/product/subscribe")
        .with_status(400)
        .with_body(
            json!({"responseHeader":{"responseCode":"400","responseDescDisplay":"BIL2007"}})
                .to_string(),
        )
        .expect(1)
        .create();
    let client = ncell_cli::api::Client::with_base_url(&server.url()).unwrap();
    let steps =
        parse("sms send-free 9812345679 'hello friend'\nplans subscribe 123\nbalance").unwrap();
    let mut calls = 0;
    let result = execute(&steps, true, |step, _, _, _| {
        calls += 1;
        let args = step.args.clone();
        match args.command {
            Some(Command::Sms {
                action: super::super::SmsCommand::SendFree { number, text },
            }) => {
                client.send_free_sms("9812345678", "test-token", &number, &text)?;
            }
            Some(Command::Plans {
                action: Some(super::super::PlanCommand::Subscribe { .. }),
                ..
            }) => {
                let plan = json!({"techInfo":{"subscriptionCode":"123"},"ncellName":"Test pack","productOfferingPrice":{"price":"1.00"}});
                client.subscribe("9812345678", "test-token", "test-device", &plan)?;
            }
            _ => panic!("Later command must not execute"),
        }
        Ok(())
    });
    assert_eq!(calls, 2);
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Step 2 of 3 (line 2)")
    );
    sms.assert();
    purchase.assert();
}
