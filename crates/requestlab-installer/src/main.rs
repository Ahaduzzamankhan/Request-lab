//! RequestLab installer.

// The window build must not open a console; `--silent` is documented as a
// console mode and reattaches to the invoking terminal on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod flow;
mod ui;

use std::process::ExitCode;
use std::sync::Arc;

use cli::Mode;
use requestlab_manager::events::ManagerEvent;
use requestlab_manager::installer::InstallOptions;
use tokio::runtime::Runtime;

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
            println!("requestlab-installer {}", requestlab_manager::VERSION);
            ExitCode::SUCCESS
        }
        Mode::Silent(options) => silent(options),
        Mode::Window => window(),
    }
}

fn window() -> ExitCode {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("requestlab=warn")
        .try_init();
    let runtime = match Runtime::new() {
        Ok(runtime) => Arc::new(runtime),
        Err(error) => {
            eprintln!("error: could not start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match ui::run(runtime) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn silent(options: cli::SilentOptions) -> ExitCode {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("requestlab=warn")
        .try_init();
    let runtime = match Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("error: could not start the async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let install_options = InstallOptions {
        install_dir: options.install_dir.clone(),
        channel: options.channel,
        shortcuts: options.shortcuts,
        register_uninstall: options.register_uninstall,
    };
    let result = runtime.block_on(flow::install(install_options, options.launch, |event| {
        println!("{}", describe(&event))
    }));
    match result {
        Ok(metadata) => {
            println!(
                "RequestLab {} installed in {}",
                metadata.version,
                options.install_dir.display()
            );
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
        ManagerEvent::Transfer(transfer) => {
            let total = transfer
                .total
                .map(requestlab_manager::downloader::human_bytes)
                .unwrap_or_else(|| "?".to_string());
            format!(
                "download {} / {}",
                requestlab_manager::downloader::human_bytes(transfer.downloaded),
                total
            )
        }
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
