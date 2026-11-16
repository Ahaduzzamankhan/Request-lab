//! Platform integrations: free space, shortcuts, uninstall registration,
//! process waiting and launching.
//!
//! Windows is the supported target today. Elsewhere these return
//! [`ManagerError::Unsupported`] instead of pretending to have done something.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{ManagerError, ManagerResult};

/// Where an installation's shortcuts live.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShortcutPaths {
    pub desktop: Option<PathBuf>,
    pub start_menu: Option<PathBuf>,
}

/// Free bytes available to the current user, or `None` when unknown.
pub fn free_space(directory: &Path) -> Option<u64> {
    imp::free_space(directory)
}

/// Shortcut locations for a user installation.
pub fn shortcut_paths(install_dir: &Path) -> ShortcutPaths {
    imp::shortcut_paths(install_dir)
}

/// Create the desktop and start menu shortcuts; returns the ones created.
pub fn create_shortcuts(install_dir: &Path) -> ManagerResult<Vec<PathBuf>> {
    let targets = imp::shortcut_paths(install_dir);
    let app = crate::paths::app_path(install_dir);
    let mut created = Vec::new();
    for shortcut in [targets.desktop.as_ref(), targets.start_menu.as_ref()]
        .into_iter()
        .flatten()
    {
        imp::create_shortcut(&app, install_dir, shortcut, "Launch RequestLab")?;
        created.push(shortcut.clone());
    }
    Ok(created)
}

/// Delete shortcuts created by the installer.
pub fn remove_shortcuts(install_dir: &Path) -> Vec<ManagerError> {
    let targets = imp::shortcut_paths(install_dir);
    [targets.desktop, targets.start_menu]
        .into_iter()
        .flatten()
        .filter_map(|shortcut| crate::paths::remove_file_if_exists(&shortcut).err())
        .collect()
}

/// Register the installation under Add/Remove Programs.
pub fn register_uninstall(install_dir: &Path, version: &str) -> ManagerResult<()> {
    imp::register_uninstall(install_dir, version)
}

/// Remove the Add/Remove Programs entry.
pub fn unregister_uninstall() -> ManagerResult<()> {
    imp::unregister_uninstall()
}

/// Launch an executable without waiting for it.
pub fn launch(executable: &Path) -> ManagerResult<()> {
    std::process::Command::new(executable)
        .current_dir(executable.parent().unwrap_or_else(|| Path::new(".")))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            ManagerError::Other(format!(
                "could not launch {}: {error}",
                executable.display()
            ))
        })
}

/// Wait until `pid` exits, up to `timeout`; `Ok(false)` means it timed out.
pub fn wait_for_process_exit(pid: u32, timeout: Duration) -> ManagerResult<bool> {
    imp::wait_for_process_exit(pid, timeout)
}

/// Whether a process with this id is running.
pub fn process_is_running(pid: u32) -> bool {
    imp::process_is_running(pid)
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    };

    use super::ShortcutPaths;
    use crate::error::{ManagerError, ManagerResult};

    const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\RequestLab";

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    pub fn free_space(directory: &Path) -> Option<u64> {
        let mut existing = directory;
        while !existing.exists() {
            existing = existing.parent()?;
        }
        let mut available = 0u64;
        // SAFETY: the path is NUL-terminated and the out-pointer is valid.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide(existing).as_ptr(),
                &mut available,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(available)
    }

    pub fn shortcut_paths(_install_dir: &Path) -> ShortcutPaths {
        let desktop = std::env::var_os("USERPROFILE")
            .map(|profile| PathBuf::from(profile).join("Desktop"))
            .filter(|path| path.is_dir())
            .map(|dir| dir.join("RequestLab.lnk"));
        let start_menu = std::env::var_os("APPDATA")
            .map(|appdata| {
                PathBuf::from(appdata)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Start Menu")
                    .join("Programs")
            })
            .filter(|path| path.is_dir())
            .map(|dir| dir.join("RequestLab.lnk"));
        ShortcutPaths {
            desktop,
            start_menu,
        }
    }

    pub fn create_shortcut(
        target: &Path,
        working_directory: &Path,
        shortcut: &Path,
        description: &str,
    ) -> ManagerResult<()> {
        if let Some(parent) = shortcut.parent() {
            crate::paths::ensure_dir(parent)?;
        }
        // WScript.Shell is the supported way to write a .lnk without a native COM
        // implementation. Every value is ours; nothing comes from an archive.
        let script = format!(
            "$ws = New-Object -ComObject WScript.Shell; \
             $s = $ws.CreateShortcut('{}'); \
             $s.TargetPath = '{}'; \
             $s.WorkingDirectory = '{}'; \
             $s.IconLocation = '{},0'; \
             $s.Description = '{}'; \
             $s.Save()",
            escape(shortcut),
            escape(target),
            escape(working_directory),
            escape(target),
            description.replace('\'', "''")
        );
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ])
            .output()
            .map_err(|error| {
                ManagerError::Other(format!(
                    "could not run PowerShell to create a shortcut: {error}"
                ))
            })?;
        if !output.status.success() {
            return Err(ManagerError::Other(format!(
                "could not create {}: {}",
                shortcut.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        if !shortcut.is_file() {
            return Err(ManagerError::Other(format!(
                "PowerShell reported success but {} was not created",
                shortcut.display()
            )));
        }
        Ok(())
    }

    fn escape(path: &Path) -> String {
        path.display().to_string().replace('\'', "''")
    }

    pub fn register_uninstall(install_dir: &Path, version: &str) -> ManagerResult<()> {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let current_user = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = current_user.create_subkey(UNINSTALL_KEY).map_err(|error| {
            ManagerError::Other(format!("cannot write the uninstall registry key: {error}"))
        })?;

        let app = crate::paths::app_path(install_dir);
        let cli = crate::paths::cli_path(install_dir);
        let uninstall_string = if cli.is_file() {
            format!("\"{}\" uninstall --yes", cli.display())
        } else {
            format!("\"{}\" uninstall --yes", app.display())
        };
        let values: [(&str, String); 7] = [
            ("DisplayName", "RequestLab".to_string()),
            ("DisplayVersion", version.to_string()),
            ("Publisher", "RequestLab".to_string()),
            ("InstallLocation", install_dir.display().to_string()),
            ("DisplayIcon", format!("{},0", app.display())),
            ("UninstallString", uninstall_string),
            (
                "QuietUninstallString",
                format!("{uninstall_string} --quiet"),
            ),
        ];
        for (name, value) in values {
            key.set_value(name, &value).map_err(|error| {
                ManagerError::Other(format!("cannot write registry value {name}: {error}"))
            })?;
        }
        key.set_value("NoModify", &1u32)
            .and_then(|()| key.set_value("NoRepair", &1u32))
            .and_then(|()| key.set_value("EstimatedSize", &directory_size_kb(install_dir)))
            .map_err(|error| ManagerError::Other(format!("cannot write registry values: {error}")))
    }

    pub fn unregister_uninstall() -> ManagerResult<()> {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        match RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(UNINSTALL_KEY) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ManagerError::Other(format!(
                "cannot remove the uninstall registry key: {error}"
            ))),
        }
    }

    fn directory_size_kb(directory: &Path) -> u32 {
        fn size(directory: &Path) -> u64 {
            let mut total = 0;
            if let Ok(entries) = std::fs::read_dir(directory) {
                for entry in entries.flatten() {
                    match entry.metadata() {
                        Ok(metadata) if metadata.is_dir() => total += size(&entry.path()),
                        Ok(metadata) => total += metadata.len(),
                        Err(_) => {}
                    }
                }
            }
            total
        }
        (size(directory) / 1024).min(u32::MAX as u64) as u32
    }

    fn open_for_wait(pid: u32) -> Option<*mut std::ffi::c_void> {
        // SAFETY: OpenProcess only needs a pid; a null handle means "gone".
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        (!handle.is_null()).then_some(handle)
    }

    pub fn wait_for_process_exit(pid: u32, timeout: Duration) -> ManagerResult<bool> {
        let Some(handle) = open_for_wait(pid) else {
            return Ok(true);
        };
        let milliseconds = timeout.as_millis().min(u32::MAX as u128) as u32;
        // SAFETY: the handle came from OpenProcess and is closed below.
        let result = unsafe { WaitForSingleObject(handle, milliseconds) };
        // SAFETY: this function owns the handle.
        unsafe { CloseHandle(handle) };
        match result {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            other => Err(ManagerError::Other(format!(
                "waiting for process {pid} failed with status {other}"
            ))),
        }
    }

    pub fn process_is_running(pid: u32) -> bool {
        let Some(handle) = open_for_wait(pid) else {
            return false;
        };
        // SAFETY: the handle came from OpenProcess.
        let exited = unsafe { WaitForSingleObject(handle, 0) } == WAIT_OBJECT_0;
        // SAFETY: this function owns the handle.
        unsafe { CloseHandle(handle) };
        !exited
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use super::ShortcutPaths;
    use crate::error::{ManagerError, ManagerResult};

    pub fn free_space(_directory: &Path) -> Option<u64> {
        None
    }

    pub fn shortcut_paths(_install_dir: &Path) -> ShortcutPaths {
        let desktop = std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Desktop"))
            .filter(|path| path.is_dir())
            .map(|dir| dir.join("requestlab.desktop"));
        let applications = std::env::var_os("HOME")
            .map(|home| {
                PathBuf::from(home)
                    .join(".local")
                    .join("share")
                    .join("applications")
            })
            .filter(|path| path.is_dir())
            .map(|dir| dir.join("requestlab.desktop"));
        ShortcutPaths {
            desktop,
            start_menu: applications,
        }
    }

    pub fn create_shortcut(
        target: &Path,
        working_directory: &Path,
        shortcut: &Path,
        description: &str,
    ) -> ManagerResult<()> {
        if shortcut.extension().and_then(|value| value.to_str()) != Some("desktop") {
            return Err(ManagerError::Unsupported(format!(
                "creating {} on this platform",
                shortcut.display()
            )));
        }
        if let Some(parent) = shortcut.parent() {
            crate::paths::ensure_dir(parent)?;
        }
        let contents = format!(
            "[Desktop Entry]\nType=Application\nName=RequestLab\nComment={description}\nExec=\"{}\"\nPath={}\nTerminal=false\n",
            target.display(),
            working_directory.display()
        );
        std::fs::write(shortcut, contents).map_err(|error| ManagerError::io(shortcut, error))
    }

    pub fn register_uninstall(_install_dir: &Path, _version: &str) -> ManagerResult<()> {
        Err(ManagerError::Unsupported(
            "registering an uninstaller with the system package database".to_string(),
        ))
    }

    pub fn unregister_uninstall() -> ManagerResult<()> {
        Ok(())
    }

    pub fn wait_for_process_exit(pid: u32, timeout: Duration) -> ManagerResult<bool> {
        let deadline = std::time::Instant::now() + timeout;
        while Path::new(&format!("/proc/{pid}")).exists() {
            if std::time::Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(true)
    }

    pub fn process_is_running(pid: u32) -> bool {
        Path::new(&format!("/proc/{pid}")).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launching_a_missing_executable_fails_clearly() {
        let error = launch(Path::new("/definitely/not/here/requestlab")).unwrap_err();
        assert!(error.to_string().contains("could not launch"), "{error}");
    }

    #[test]
    fn waiting_for_a_reaped_process_and_an_unknown_pid_succeeds() {
        let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" })
            .args(if cfg!(windows) {
                vec!["/C", "exit 0"]
            } else {
                vec![]
            })
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        assert!(wait_for_process_exit(pid, Duration::from_secs(5)).unwrap());
        assert!(!process_is_running(pid));
        assert!(wait_for_process_exit(u32::MAX, Duration::from_secs(1)).unwrap());
    }
}
