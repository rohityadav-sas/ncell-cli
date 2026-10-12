use std::{fs, path::PathBuf, process::Command};
struct Workspace {
    root: PathBuf,
}
impl Workspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("ncell-scripts-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self { root }
    }
    fn cli(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ncell"));
        command
            .current_dir(&self.root)
            .env("APPDATA", &self.root)
            .env("HOME", &self.root)
            .env("XDG_DATA_HOME", &self.root);
        command
    }
    fn library(&self) -> PathBuf {
        #[cfg(target_os = "macos")]
        return self
            .root
            .join("Library/Application Support/ncell-cli/scripts");
        #[cfg(not(target_os = "macos"))]
        self.root.join("ncell-cli/scripts")
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        assert!(self.root.is_absolute() && self.root.starts_with(std::env::temp_dir()));
        fs::remove_dir_all(&self.root).unwrap();
    }
}
#[test]
fn create_saves_in_library_and_never_overwrites() {
    let workspace = Workspace::new();
    fs::create_dir_all(workspace.library()).unwrap();
    let saved = workspace.library().join("daily.ncell");
    fs::write(&saved, "# existing script").unwrap();
    let original = fs::read(&saved).unwrap();
    assert!(
        !workspace
            .cli()
            .args(["script", "create", "daily.ncell"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(fs::read(saved).unwrap(), original);
    for invalid in ["../escape.ncell", "folder/file.ncell", "other.txt"] {
        assert!(
            !workspace
                .cli()
                .args(["script", "create", invalid])
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
#[test]
fn list_shows_only_saved_scripts_and_needs_no_folder() {
    let workspace = Workspace::new();
    let empty = workspace
        .cli()
        .args(["script", "list", "--json"])
        .output()
        .unwrap();
    assert!(empty.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&empty.stdout).unwrap()["scripts"],
        serde_json::json!([])
    );
    fs::create_dir_all(workspace.library()).unwrap();
    for name in ["z.ncell", "Alpha.NCELL", "notes.txt"] {
        fs::write(workspace.library().join(name), "").unwrap();
    }
    fs::write(workspace.root.join("local.ncell"), "").unwrap();
    fs::create_dir(workspace.library().join("directory.ncell")).unwrap();
    let output = workspace
        .cli()
        .args(["script", "list", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["scripts"],
        serde_json::json!(["Alpha.NCELL", "z.ncell"])
    );
    let output = workspace.cli().args(["script", "list"]).output().unwrap();
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Script\n-----------\nAlpha.NCELL\nz.ncell\n"
    );
    assert!(
        !workspace
            .cli()
            .args(["script", "list", "."])
            .output()
            .unwrap()
            .status
            .success()
    );
}
#[test]
fn run_prefers_saved_script_and_falls_back_to_local_file() {
    let workspace = Workspace::new();
    fs::create_dir_all(workspace.library()).unwrap();
    fs::write(workspace.library().join("daily.ncell"), "# saved script\n").unwrap();
    fs::write(workspace.root.join("daily.ncell"), "not-a-command").unwrap();
    let output = workspace
        .cli()
        .args(["script", "run", "daily.ncell", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["event"],
        "complete"
    );
    assert!(
        !workspace
            .cli()
            .args(["script", "run", "./daily.ncell"])
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::remove_file(workspace.library().join("daily.ncell")).unwrap();
    assert!(
        !workspace
            .cli()
            .args(["script", "run", "daily.ncell"])
            .output()
            .unwrap()
            .status
            .success()
    );
    fs::write(workspace.root.join("daily.ncell"), "# local script\n").unwrap();
    assert!(
        workspace
            .cli()
            .args(["script", "run", "daily.ncell"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !workspace
            .cli()
            .args(["script", "run", "missing.ncell"])
            .output()
            .unwrap()
            .status
            .success()
    );
}
#[test]
fn validates_all_lines_before_starting() {
    let workspace = Workspace::new();
    fs::write(workspace.root.join("bad.ncell"), "logout\nnot-a-command\n").unwrap();
    let output = workspace
        .cli()
        .args(["script", "run", "bad.ncell"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr).unwrap().contains("Line 2"));
}
#[test]
fn delete_removes_only_the_named_saved_script() {
    let workspace = Workspace::new();
    fs::create_dir_all(workspace.library()).unwrap();
    fs::write(workspace.library().join("daily.ncell"), "saved").unwrap();
    fs::write(workspace.library().join("keep.ncell"), "keep").unwrap();
    fs::write(workspace.root.join("daily.ncell"), "local").unwrap();
    let output = workspace
        .cli()
        .args(["script", "delete", "daily.ncell", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["deleted"],
        "daily.ncell"
    );
    assert!(!workspace.library().join("daily.ncell").exists());
    assert!(workspace.library().join("keep.ncell").exists());
    assert_eq!(
        fs::read_to_string(workspace.root.join("daily.ncell")).unwrap(),
        "local"
    );
    let output = workspace
        .cli()
        .args(["script", "delete", "daily.ncell"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("was not found")
    );
    for invalid in [
        "../daily.ncell",
        "./daily.ncell",
        "folder/daily.ncell",
        "daily.txt",
    ] {
        assert!(
            !workspace
                .cli()
                .args(["script", "delete", invalid])
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    fs::create_dir(workspace.library().join("directory.ncell")).unwrap();
    assert!(
        !workspace
            .cli()
            .args(["script", "delete", "directory.ncell"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(workspace.library().join("directory.ncell").is_dir());
    let output = workspace
        .cli()
        .args(["script", "delete", "keep.ncell"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Deleted keep.ncell.\n"
    );
}
#[test]
fn edit_rejects_missing_scripts_and_paths() {
    let workspace = Workspace::new();
    fs::create_dir_all(workspace.library()).unwrap();
    let file = workspace.library().join("daily.ncell");
    fs::write(&file, "# saved").unwrap();
    assert!(
        !workspace
            .cli()
            .args(["script", "edit", "missing.ncell"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !workspace
            .cli()
            .args(["script", "edit", "../daily.ncell"])
            .output()
            .unwrap()
            .status
            .success()
    );
    assert_eq!(fs::read_to_string(file).unwrap(), "# saved");
}
