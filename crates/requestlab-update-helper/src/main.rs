//! RequestLab update helper and release tooling.

mod apply;
mod args;
mod manifest_tool;

use std::process::ExitCode;

use args::Mode;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("REQUESTLAB_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("requestlab=warn")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    match args::parse(arguments) {
        Ok(Mode::Help) => {
            println!(
                "{}",
                args::USAGE.replace("--version VERSION", "--version-for VERSION")
            );
            ExitCode::SUCCESS
        }
        Ok(Mode::Version) => {
            println!("requestlab-update-helper {}", requestlab_manager::VERSION);
            ExitCode::SUCCESS
        }
        Ok(Mode::Apply(options)) => match apply::run(&options) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        },
        Ok(Mode::BuildManifest(options)) => match manifest_tool::build(&options) {
            Ok(_) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        },
        Ok(Mode::Sign(options)) => match manifest_tool::sign(&options) {
            Ok(_) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        },
        Ok(Mode::PublicKey { key_file }) => match manifest_tool::public_key(key_file.as_deref()) {
            Ok(key) => {
                println!("{key}");
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("error: {message}");
                ExitCode::FAILURE
            }
        },
        Ok(Mode::GenerateKey(mut options)) => {
            if options.trust_dir.is_none() {
                options.trust_dir = Some(manifest_tool::default_trust_dir());
            }
            match manifest_tool::generate_key(&options) {
                Ok(_) => ExitCode::SUCCESS,
                Err(message) => {
                    eprintln!("error: {message}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(message) => {
            eprintln!("error: {message}\n\nRun 'requestlab-update-helper --help' for usage.");
            ExitCode::from(2)
        }
    }
}
