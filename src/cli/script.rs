use super::{Args, Command, validate_args};
use clap::{Parser, Subcommand};
use ncell_cli::error::{Error, Result};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

thread_local! {
    static CONTEXT: std::cell::RefCell<Option<Value>> = const { std::cell::RefCell::new(None) };
}
pub(super) fn emit_json(result: Value) {
    CONTEXT.with(|context| {
        let output = if let Some(context) = context.borrow().as_ref() {
            let mut output = context.clone();
            output["result"] = result;
            output
        } else {
            result
        };
        println!("{output:#}");
        let _ = io::stdout().flush();
    });
}

#[derive(Subcommand, Clone)]
pub(super) enum ScriptCommand {
    /// Save an example in the CLI library and open it in your editor.
    Create { file: PathBuf },
    /// Open a saved script in your editor.
    Edit { file: PathBuf },
    /// Delete a script saved in the CLI library.
    Delete { file: PathBuf },
    /// List scripts saved in the CLI.
    List,
    /// Validate a command file, then run its commands in order.
    Run { file: PathBuf },
}
const EXAMPLE: &str = r#"# Daily rewards
ncell game check-in
ncell game claim-task 1,2,3,4,5,6,7,8

# Spinner reward
ncell game spin --count 5
ncell game claim-task 22

# Scratch card reward
ncell game scratch --count 4
ncell game claim-task 23

# Math reward
ncell game math --count 1
ncell game claim-task 26

# Game Warrior reward
ncell game claim-task 18

# Math Genius reward
ncell game math --count 4
ncell game claim-task 24

# Spend remaining tokens
ncell game spin --count all

# Final checks
ncell game status
ncell game ranks
"#;
struct Step {
    line: usize,
    command: String,
    args: Args,
}
fn fail(message: impl std::fmt::Display) -> Error {
    Error::Script(message.to_string())
}

fn script_directory() -> Result<PathBuf> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    base.filter(|p| p.is_absolute())
        .map(|p| p.join("ncell-cli/scripts"))
        .ok_or_else(|| fail("Could not locate the CLI script storage"))
}
fn script_name(file: &std::path::Path) -> Result<()> {
    if file.components().count() != 1
        || !matches!(
            file.components().next(),
            Some(std::path::Component::Normal(_))
        )
        || file.to_string_lossy().contains(['/', '\\', ':'])
        || !file
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("ncell"))
    {
        return Err(fail(
            "Use a script name ending in .ncell, such as daily.ncell",
        ));
    }
    Ok(())
}
fn resolve_script(file: PathBuf) -> Result<PathBuf> {
    if script_name(&file).is_ok() {
        let saved = script_directory()?.join(&file);
        match fs::metadata(&saved) {
            Ok(_) => return Ok(saved),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(fail(format!("Could not read {}: {error}", saved.display()))),
        }
    }
    Ok(file)
}

fn editor_arguments() -> Vec<&'static str> {
    #[cfg(target_os = "windows")]
    return vec!["notepad.exe"];
    #[cfg(target_os = "macos")]
    return vec!["open", "-t"];
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    vec!["xdg-open"]
}
fn create_file(
    file: &std::path::Path,
    open: impl FnOnce(&std::path::Path) -> Result<()>,
) -> Result<()> {
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(file)
        .map_err(|e| fail(format!("Could not create {}: {e}", file.display())))?;
    output.write_all(EXAMPLE.as_bytes()).map_err(fail)?;
    drop(output);
    open(file)
}
fn edit_script(file: &std::path::Path, json_output: bool) -> Result<()> {
    if !fs::metadata(file)
        .map_err(|e| fail(format!("Could not open {}: {e}", file.display())))?
        .is_file()
    {
        return Err(fail("The saved script is not a file"));
    }
    let arguments = editor_arguments();
    let status = std::process::Command::new(arguments[0])
        .args(&arguments[1..])
        .arg(file)
        .stdin(std::process::Stdio::inherit())
        .stdout(if json_output {
            std::process::Stdio::from(io::stderr())
        } else {
            std::process::Stdio::inherit()
        })
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|e| {
            fail(format!(
                "Script is saved at {} but the editor could not open: {e}",
                file.display()
            ))
        })?;
    if !status.success() {
        return Err(fail(format!(
            "Editor exited unsuccessfully. Script remains saved at {}",
            file.display()
        )));
    }
    Ok(())
}

// Shell-free quoting: backslashes are literal, including in Windows paths.
fn words(line: &str) -> std::result::Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    for ch in line.chars() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else {
                word.push(ch);
            }
        } else {
            match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    started = true;
                }
                '|' | '&' | ';' | '<' | '>' | '`' | '$' => {
                    return Err("Shell operators and substitutions are not supported".into());
                }
                c if c.is_whitespace() => {
                    if started {
                        words.push(std::mem::take(&mut word));
                        started = false;
                    }
                }
                c => {
                    word.push(c);
                    started = true;
                }
            }
        }
    }
    if quote.is_some() {
        return Err("Unclosed quote".into());
    }
    if started {
        words.push(word);
    }
    Ok(words)
}
fn parse(source: &str) -> Result<Vec<Step>> {
    let mut steps = Vec::new();
    for (index, line) in source.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let error = |message: String| fail(format!("Line {}: {message}", index + 1));
        let mut words = words(line).map_err(error)?;
        if words.first().is_some_and(|w| w == "ncell") {
            words.remove(0);
        }
        let args = Args::try_parse_from(std::iter::once("ncell".to_owned()).chain(words.clone()))
            .map_err(|e| error(e.to_string()))?;
        if matches!(args.command, Command::Script { .. }) {
            return Err(error("Nested script commands are not supported".into()));
        }
        validate_args(&args).map_err(error)?;
        steps.push(Step {
            line: index + 1,
            command: line.to_owned(),
            args,
        });
    }
    Ok(steps)
}
fn execute(
    steps: &[Step],
    json_output: bool,
    mut command: impl FnMut(&Step, bool, usize, usize) -> Result<()>,
) -> Result<()> {
    for (index, step) in steps.iter().enumerate() {
        if !json_output {
            if index > 0 {
                println!();
            }
            let progress = clap::builder::styling::AnsiColor::Cyan.on_default();
            let command_style = clap::builder::styling::AnsiColor::Yellow
                .on_default()
                .bold();
            let separator = clap::builder::styling::Style::new().dimmed();
            anstream::println!(
                "{progress}Step {} of {}{progress:#} {separator}·{separator:#} {command_style}{}{command_style:#}",
                index + 1,
                steps.len(),
                step.command
            );
            println!();
            io::stdout().flush().map_err(fail)?;
        }
        command(step, json_output, index + 1, steps.len()).map_err(|e| {
            fail(format!(
                "Step {} of {} (line {}): {e}",
                index + 1,
                steps.len(),
                step.line
            ))
        })?;
    }
    if json_output {
        println!("{:#}", json!({"event":"complete", "steps":steps.len()}));
    }
    Ok(())
}
pub(super) fn run(
    action: ScriptCommand,
    json_output: bool,
    runtime: &mut super::Runtime,
) -> Result<()> {
    match action {
        ScriptCommand::List => {
            let directory = script_directory()?;
            let entries = match fs::read_dir(&directory) {
                Ok(entries) => Some(entries),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(fail(format!("Could not list saved scripts: {error}"))),
            };
            let mut scripts = Vec::new();
            for entry in entries.into_iter().flatten() {
                let entry = entry.map_err(fail)?;
                let path = entry.path();
                if entry.file_type().map_err(fail)?.is_file()
                    && path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("ncell"))
                {
                    scripts.push(path);
                }
            }
            scripts.sort_by_key(|path| path.file_name().unwrap().to_string_lossy().to_lowercase());
            if json_output {
                emit_json(
                    json!({"scripts":scripts.iter().map(|path| path.file_name().unwrap().to_string_lossy()).collect::<Vec<_>>()}),
                );
            } else if scripts.is_empty() {
                println!("No saved scripts.");
            } else {
                println!("Script");
                let width = scripts
                    .iter()
                    .map(|path| path.file_name().unwrap().to_string_lossy().chars().count())
                    .max()
                    .unwrap()
                    .max(6);
                println!("{}", "-".repeat(width));
                for path in scripts {
                    println!("{}", path.file_name().unwrap().to_string_lossy());
                }
            }
            Ok(())
        }
        ScriptCommand::Edit { file } => {
            script_name(&file)?;
            let saved = script_directory()?.join(&file);
            edit_script(&saved, json_output)?;
            if json_output {
                emit_json(json!({"edited":file}));
            }
            Ok(())
        }
        ScriptCommand::Delete { file } => {
            script_name(&file)?;
            let saved = script_directory()?.join(&file);
            fs::remove_file(&saved).map_err(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    fail(format!("Saved script {} was not found", file.display()))
                } else {
                    fail(format!("Could not delete {}: {error}", file.display()))
                }
            })?;
            if json_output {
                emit_json(json!({"deleted":file}));
            } else {
                println!("Deleted {}.", file.display());
            }
            Ok(())
        }
        ScriptCommand::Create { file } => {
            script_name(&file)?;
            let directory = script_directory()?;
            fs::create_dir_all(&directory).map_err(fail)?;
            let file = directory.join(file);
            create_file(&file, |file| edit_script(file, json_output))?;
            if json_output {
                println!("{:#}", json!({"created":file}));
            } else {
                println!(
                    "Created {}. Run it with ncell script run after saving your changes.",
                    file.display()
                );
            }
            Ok(())
        }
        ScriptCommand::Run { file } => {
            let file = resolve_script(file)?;
            let source = fs::read_to_string(&file)
                .map_err(|e| fail(format!("Could not read {}: {e}", file.display())))?;
            let steps = parse(&source).map_err(|e| fail(format!("{}: {e}", file.display())))?;
            execute(&steps, json_output, |step, json_output, index, total| {
                let mut args = step.args.clone();
                args.json |= json_output;
                CONTEXT.with(|context| *context.borrow_mut() = json_output.then(|| json!({
                    "event":"result", "step":index, "total":total, "line":step.line, "command":step.command
                })));
                let result = super::run_with(args, runtime);
                CONTEXT.with(|context| *context.borrow_mut() = None);
                result
            }).map_err(|e| fail(format!("{}: {e}", file.display())))
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/cli_script.rs"]
mod tests;
