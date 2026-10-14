mod cli;
use clap::Parser;
fn main() -> std::process::ExitCode {
    let args = match cli::Args::try_parse() {
        Ok(args) => args,
        Err(error) if error.kind() == clap::error::ErrorKind::DisplayHelp => {
            print!("{}", error.to_string().replace("\n      --", "\n  --"));
            return std::process::ExitCode::SUCCESS;
        }
        Err(error) => error.exit(),
    };
    let json = args.json;
    match cli::run(args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(ncell_cli::error::Error::GameLost) => std::process::ExitCode::FAILURE,
        Err(error) => {
            // Every --json failure is one envelope on stderr: scripts only watch one stream.
            if json {
                match &error {
                    ncell_cli::error::Error::Api {
                        status,
                        code,
                        response,
                    } => eprintln!(
                        "{}",
                        serde_json::json!({"error": error.to_string(), "status": status, "code": code, "response": response})
                    ),
                    ncell_cli::error::Error::CheckIn {
                        message,
                        status,
                        response,
                    } => eprintln!(
                        "{}",
                        serde_json::json!({"error": message, "status": status, "response": response})
                    ),
                    ncell_cli::error::Error::GameTasks { response } => eprintln!(
                        "{}",
                        serde_json::json!({"error": error.to_string(), "response": response})
                    ),
                    _ => eprintln!("{}", serde_json::json!({"error": error.to_string()})),
                }
            } else {
                eprintln!("Error: {error}");
            }
            std::process::ExitCode::FAILURE
        }
    }
}
