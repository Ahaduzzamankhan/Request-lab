//! RequestLab command line entry point.

use std::process::ExitCode;

use requestlab_cli::{args, commands, CliError, Command};

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("REQUESTLAB_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("requestlab=warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    let invocation = match args::parse(std::env::args().skip(1)) {
        Ok(invocation) => invocation,
        Err(message) => {
            eprintln!("error: {message}\n\nRun 'requestlab-cli --help' for usage.");
            return ExitCode::from(2);
        }
    };

    // `help` and `version` never touch the database, so they stay fast and work
    // even when the data directory is unavailable.
    match &invocation.command {
        Command::Help => {
            println!("{}", args::USAGE);
            return ExitCode::SUCCESS;
        }
        Command::Version => {
            println!("requestlab-cli {}", requestlab_cli::VERSION);
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
