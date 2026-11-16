//! Shared command line entry point.
//!
//! Both the dedicated `requestlab-cli` binary and the desktop executable
//! (when started with arguments) run through this function, so there is exactly
//! one implementation of the terminal behaviour.

use std::process::ExitCode;

use crate::{args, commands, CliError, Command};

/// Run the CLI with already-collected arguments (program arguments only).
pub fn run_cli(arguments: Vec<String>, program: &str) -> ExitCode {
    // The desktop executable is a GUI binary on Windows; when it is asked to run
    // a command it attaches to the invoking console first. Logs go to stderr.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("REQUESTLAB_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("requestlab=warn")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let invocation = match args::parse(arguments) {
        Ok(invocation) => invocation,
        Err(message) => {
            let message = message.replace("requestlab-cli", program);
            eprintln!("error: {message}\n\nRun '{program} --help' for usage.");
            return ExitCode::from(2);
        }
    };

    // `help` and `version` never touch the database, so they stay fast and work
    // even when the data directory is unavailable.
    match &invocation.command {
        Command::Help => {
            println!("{}", args::USAGE.replace("requestlab-cli", program));
            return ExitCode::SUCCESS;
        }
        Command::Version => {
            println!("{program} {}", crate::VERSION);
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("error: could not start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(commands::run(invocation)) {
        Ok(output) => {
            if !output.text.is_empty() {
                print!("{}", output.text);
                if !output.text.ends_with('\n') {
                    println!();
                }
            }
            ExitCode::from(u8::try_from(output.exit_code).unwrap_or(1))
        }
        Err(CliError { message, exit_code }) => {
            eprintln!("error: {message}");
            ExitCode::from(u8::try_from(exit_code).unwrap_or(1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_command_reports_the_shipped_version() {
        let status = run_cli(vec!["--version".to_string()], "requestlab");
        assert_eq!(status, ExitCode::SUCCESS);
    }

    #[test]
    fn unknown_arguments_fail_with_a_usage_hint() {
        let status = run_cli(vec!["definitely-not-a-command".to_string()], "requestlab");
        assert_eq!(status, ExitCode::from(2));
    }
}
