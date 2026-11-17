//! RequestLab updater.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod flow;
mod ui;

use std::process::ExitCode;
use std::sync::Arc;

use cli::Mode;
use requestlab_manager::events::ManagerEvent;
use tokio::runtime::Runtime;

/// Exit code used by `--check` when the installation is already current.
const NO_UPDATE: u8 = 3;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let mode = match cli::parse(arguments) {
        Ok(mode) => mode,
        Err(message) => {
            eprintln!("error: {message}\n\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };

    match mode {
        Mode::Help => {
            println!("{}", cli::USAGE);
            ExitCode::SUCCESS
        }
        Mode::Version => {
            println!("requestlab-updater {}", requestlab_manager::VERSION);
            ExitCode::SUCCESS
        }
        Mode::Check(options) => check(options),
        Mode::Silent(options) => silent(options),
        Mode::Window => window(),
    }
}

fn runtime() -> Result<Runtime, ExitCode> {
    Runtime::new().map_err(|error| {
        eprintln!("error: could not start the async runtime: {error}");
        ExitCode::FAILURE
    })
}

fn window() -> ExitCode {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("requestlab=warn")
        .try_init();
    let runtime = match runtime() {
        Ok(runtime) => Arc::new(runtime),
        Err(code) => return code,
    };
    match ui::run(runtime) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn check(options: cli::Options) -> ExitCode {
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(code) => return code,
    };
    match runtime.block_on(flow::check(&options.install_dir)) {
        Ok(Some(update)) => {
            println!(
                "RequestLab {} is available ({} bytes)",
                update.version,
                update.size()
            );
            ExitCode::SUCCESS
        }
        Ok(None) => {
            println!(
                "RequestLab {} is up to date",
                flow::installed_version(&options.install_dir)
            );
            ExitCode::from(NO_UPDATE)
        }
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn silent(options: cli::Options) -> ExitCode {
    let runtime = match runtime() {
        Ok(runtime) => runtime,
        Err(code) => return code,
    };
    let result = runtime.block_on(flow::update(
        &options.install_dir,
        options.pid,
        options.launch,
        |event| println!("{}", describe(&event)),
    ));
    match result {
        Ok(version) => {
            println!("RequestLab {version} is installed");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn describe(event: &ManagerEvent) -> String {
    match event {
        ManagerEvent::Stage { stage, detail } => format!("{}: {detail}", stage.label()),
        ManagerEvent::Transfer(transfer) => format!(
            "download {}",
            requestlab_manager::downloader::human_bytes(transfer.downloaded)
        ),
        ManagerEvent::Extracting { fraction, .. } => match fraction {
            Some(fraction) => format!("extract {:.0}%", fraction * 100.0),
            None => "extract".to_string(),
        },
        ManagerEvent::Warning(message) => format!("warning: {message}"),
        ManagerEvent::Installed { version, directory } => {
            format!("installed {version} in {}", directory.display())
        }
    }
}
