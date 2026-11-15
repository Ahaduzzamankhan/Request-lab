//! Tracing setup, orderly shutdown and startup failure reporting.

use std::io::Write as _;
use std::path::PathBuf;

use tracing_subscriber::{fmt, layer::SubscriberExt as _, util::SubscriberInitExt as _, EnvFilter};

/// Initialise structured logging.
///
/// Logs go to `requestlab.log` inside the application data directory. Setting
/// `REQUESTLAB_LOG_STDERR` writes to stderr instead, and `REQUESTLAB_LOG`
/// overrides the default `requestlab=info,warn` filter. Nothing below the UI
/// logs credentials: headers are redacted before they reach a log record.
pub fn init_tracing() {
    let filter = EnvFilter::try_from_env("REQUESTLAB_LOG")
        .unwrap_or_else(|_| EnvFilter::new("requestlab=info,warn"));

    let file = if std::env::var_os("REQUESTLAB_LOG_STDERR").is_some() {
        None
    } else {
        open_log_file()
    };

    match file {
        Some(file) => tracing_subscriber::registry()
            .with(filter)
            .with(
                fmt::layer()
                    .with_ansi(false)
                    .with_writer(std::sync::Mutex::new(file)),
            )
            .init(),
        None => tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer())
            .init(),
    }
}

fn log_file_path() -> PathBuf {
    requestlab_storage::default_data_dir().join("requestlab.log")
}

fn open_log_file() -> Option<std::fs::File> {
    let path = log_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok()?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
}

/// Report a failure that happened before the event loop could show a window.
pub fn report_startup_failure(error: &anyhow::Error) {
    let message = format!("RequestLab could not start.\n\n{error:#}\n");
    tracing::error!(%error, "startup failed");
    eprintln!("{message}");
    if let Some(mut file) = open_log_file() {
        let _ = writeln!(file, "{message}");
    }
    std::process::exit(1);
}

/// Clean shutdown. All state is committed as it changes, so this only records
/// the exit in the log.
pub fn shutdown() {
    tracing::info!("RequestLab shut down");
}
