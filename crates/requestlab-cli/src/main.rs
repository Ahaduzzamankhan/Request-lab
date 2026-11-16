//! RequestLab command line entry point.

use std::process::ExitCode;

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    requestlab_cli::entry::run_cli(arguments, "requestlab-cli")
}
