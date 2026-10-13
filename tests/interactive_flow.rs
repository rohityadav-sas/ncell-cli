use std::process::Command;

#[test]
fn interactive_help_remains_available_without_a_terminal() {
    let output = Command::new(env!("CARGO_BIN_EXE_ncell"))
        .args(["interactive", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("interactive")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn redirected_interactive_mode_fails_without_drawing_or_reading_commands() {
    let output = Command::new(env!("CARGO_BIN_EXE_ncell"))
        .arg("interactive")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("requires a terminal")
    );
}
