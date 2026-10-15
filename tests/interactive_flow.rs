use std::process::Command;

#[test]
fn help_remains_available_without_a_terminal() {
    let output = Command::new(env!("CARGO_BIN_EXE_ncell"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("Browse, search and subscribe")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn bare_command_enters_interactive_mode_and_needs_a_terminal() {
    let output = Command::new(env!("CARGO_BIN_EXE_ncell")).output().unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("requires a terminal")
    );
}
