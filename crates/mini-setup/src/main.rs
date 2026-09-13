//! `mininet-setup` --- the Windows client's installer.
//!
//! Double-clicked, it opens a five-page wizard. Given a mode flag, it runs
//! the same operations with no window at all, which is what
//! `packaging/windows/`, a managed deployment, and CI use. Both front ends
//! go through `crate::run`, so they cannot disagree.
//!
//! What this program will never do, by construction rather than by policy:
//! open a network connection (no networking dependency exists in its
//! dependency tree), require an administrator (per-user directories and
//! `HKCU` only), install a service or scheduled task, or install anything
//! whose bytes do not match the manifest it was handed.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod args;
mod flow;
mod gui;
mod json;
mod payload;
mod run;

use args::Mode;
use std::process::ExitCode;

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match args::parse(&raw) {
        Ok(parsed) => parsed,
        Err(error) => {
            // A usage error is reported in whichever form the caller asked
            // for, so a script that passes --json never has to parse English.
            if raw.iter().any(|token| token == "--json") {
                println!("{}", json::err("setup.args", "bad_arguments", &error));
            } else {
                eprintln!("mininet-setup: {error}\n\n{}", args::USAGE);
            }
            return ExitCode::from(2);
        }
    };

    match parsed.mode {
        Mode::Help => {
            print!("{}", args::USAGE);
            ExitCode::SUCCESS
        }
        Mode::Version => {
            if parsed.json {
                println!(
                    "{}",
                    json::ok(
                        "setup.version",
                        &[(
                            "version",
                            mini_windows_setup::Field::Text(env!("CARGO_PKG_VERSION").to_string())
                        )]
                    )
                );
            } else {
                println!("mininet-setup {}", env!("CARGO_PKG_VERSION"));
            }
            ExitCode::SUCCESS
        }
        Mode::Wizard => match gui::run_wizard(parsed) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("mininet-setup: could not open the installer window: {error}");
                eprintln!("Run `mininet-setup --silent` to install without a window.");
                ExitCode::FAILURE
            }
        },
        Mode::Uninstall if match run::handed_off_uninstall(&parsed) {
            Ok(value) => value,
            Err(failure) => {
                if parsed.json {
                    println!("{}", json::err("setup.uninstall", &failure.code, &failure.message));
                } else {
                    eprintln!("mininet-setup: {}", failure.message);
                }
                return ExitCode::FAILURE;
            }
        } => {
            // A copy of this program, outside the directory being removed, is
            // finishing the job. Saying so matters: the window closes
            // immediately and a user who saw nothing would reasonably retry.
            if parsed.json {
                println!(
                    "{}",
                    json::ok(
                        "setup.uninstall",
                        &[("handed_off", mini_windows_setup::Field::Flag(true))]
                    )
                );
            } else {
                println!(
                    "Removing Mininet from a temporary copy of this program, because the \
                     installed copy is inside the folder being deleted."
                );
            }
            ExitCode::SUCCESS
        }
        mode => {
            let action = match mode {
                Mode::Silent => run::install,
                Mode::Verify => run::verify,
                Mode::Status => run::status,
                Mode::Rollback => run::rollback,
                Mode::Uninstall => run::uninstall,
                Mode::DryRun => run::plan,
                Mode::Help | Mode::Version | Mode::Wizard => unreachable!("handled above"),
            };
            match action(&parsed) {
                Ok(outcome) => {
                    if parsed.json {
                        println!("{}", json::ok(outcome.kind, &outcome.fields));
                    } else {
                        print!("{}", outcome.human);
                    }
                    // A verification that found problems is a failure to the
                    // shell even though the check itself ran fine: a script
                    // that ignores this would treat "tampered" as "ok".
                    let intact = outcome.fields.iter().any(|(name, value)| {
                        *name == "intact" && *value == mini_windows_setup::Field::Flag(false)
                    });
                    if intact {
                        ExitCode::FAILURE
                    } else {
                        ExitCode::SUCCESS
                    }
                }
                Err(failure) => {
                    if parsed.json {
                        println!(
                            "{}",
                            json::err(mode.kind(), &failure.code, &failure.message)
                        );
                    } else {
                        eprintln!("mininet-setup: {}", failure.message);
                    }
                    ExitCode::FAILURE
                }
            }
        }
    }
}
