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
        Err(error) => {
            if json {
                if let ncell_cli::error::Error::Api { response, .. }
                | ncell_cli::error::Error::CheckIn { response, .. } = &error
                {
                    println!("{response:#}");
                } else {
                    eprintln!("{}", serde_json::json!({"error": error.to_string()}));
                }
            } else {
                eprintln!("Error: {error}");
            }
            std::process::ExitCode::FAILURE
        }
    }
}
