mod cli;
use clap::Parser;
fn main() -> std::process::ExitCode {
    let args = cli::Args::parse();
    let json = args.json;
    match cli::run(args) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            if json {
                if let ncell_cli::error::Error::Api { response, .. } = &error {
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
