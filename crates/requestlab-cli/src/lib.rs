//! RequestLab's command line interface.
//!
//! The CLI is a thin layer over [`requestlab_core::AppService`]: argument
//! parsing and rendering live here, every piece of application logic (HTTP
//! execution, history, collections, environments, settings) is shared with the
//! desktop UI.

pub mod args;
pub mod commands;
pub mod entry;
pub mod output;

pub use args::{parse, Command, GlobalOptions, Invocation};
pub use commands::{run, CliError, CommandOutput};

/// Version reported by `--version`, taken from the crate version which CI keeps
/// in sync with the repository's `version.json`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
