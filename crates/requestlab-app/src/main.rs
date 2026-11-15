//! RequestLab desktop entry point.

// Release builds are GUI applications: they must not open a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod application;
mod lifecycle;
mod window;

fn main() {
    lifecycle::init_tracing();
    match application::RequestLabApplication::bootstrap().and_then(|app| app.run()) {
        Ok(()) => lifecycle::shutdown(),
        Err(error) => lifecycle::report_startup_failure(&error),
    }
}
