//! Thin process entry point — all real logic lives in `mini_cli::run` so it
//! stays directly testable without spawning subprocesses.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    match mini_cli::run(&args) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            if json {
                println!(
                    "{}",
                    mini_cli::json_error_envelope(&mini_cli::command_kind(&args), &e)
                );
            } else {
                eprintln!("error: {e}");
            }
            ExitCode::FAILURE
        }
    }
}
