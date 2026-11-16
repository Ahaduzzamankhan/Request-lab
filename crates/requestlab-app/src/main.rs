//! RequestLab desktop entry point.

// Release builds are GUI applications: they must not open a console window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod application;
mod lifecycle;
mod window;

fn main() -> std::process::ExitCode {
    // A single executable: with arguments it behaves exactly like the CLI, with
    // none it opens the desktop window.
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if !arguments.is_empty() {
        #[cfg(windows)]
        console::attach_to_parent();
        return requestlab_cli::entry::run_cli(arguments, "requestlab");
    }

    lifecycle::init_tracing();
    match application::RequestLabApplication::bootstrap().and_then(|app| app.run()) {
        Ok(()) => {
            lifecycle::shutdown();
            std::process::ExitCode::SUCCESS
        }
        Err(error) => {
            lifecycle::report_startup_failure(&error);
            std::process::ExitCode::FAILURE
        }
    }
}

/// A GUI-subsystem Windows binary has no console of its own. When the executable
/// is invoked from a terminal with arguments, reattach to that terminal so the
/// CLI can read and write standard streams.
#[cfg(windows)]
mod console {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        AttachConsole, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE, STD_INPUT_HANDLE,
        STD_OUTPUT_HANDLE,
    };

    fn wide(name: &str) -> Vec<u16> {
        std::ffi::OsStr::new(name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn attach_to_parent() {
        unsafe {
            if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
                return;
            }
            let out = CreateFileW(
                wide("CONOUT$").as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );
            if out != INVALID_HANDLE_VALUE {
                SetStdHandle(STD_OUTPUT_HANDLE, out);
                SetStdHandle(STD_ERROR_HANDLE, out);
            }
            let input = CreateFileW(
                wide("CONIN$").as_ptr(),
                FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );
            if input != INVALID_HANDLE_VALUE {
                SetStdHandle(STD_INPUT_HANDLE, input);
            }
        }
    }
}
