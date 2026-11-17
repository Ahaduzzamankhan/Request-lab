//! Where RequestLab lives on disk.
//!
//! The installation directory holds executables and may be replaced wholesale by
//! an update. The user data directory holds the database, settings and trusted
//! keys and is never touched by an update — that is how user data survives.

use std::path::{Component, Path, PathBuf};

use crate::error::{ManagerError, ManagerResult};

/// Directory name under the platform application directory.
pub const APP_DIR_NAME: &str = "RequestLab";
/// Hidden working directory inside an installation.
pub const WORK_DIR: &str = ".requestlab";
/// Installation metadata file.
pub const METADATA_FILE: &str = "install.json";
/// Lock file preventing concurrent operations.
pub const LOCK_FILE: &str = "install.lock";

fn exe_suffix() -> &'static str {
    if cfg!(windows) {
        ".exe"
    } else {
        ""
    }
}

pub fn app_binary() -> String {
    format!("requestlab{}", exe_suffix())
}

pub fn cli_binary() -> String {
    format!("requestlab-cli{}", exe_suffix())
}

pub fn helper_binary() -> String {
    format!("requestlab-update-helper{}", exe_suffix())
}

/// `%APPDATA%\RequestLab` on Windows, the XDG data directory elsewhere.
pub fn user_data_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join(APP_DIR_NAME);
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local).join(APP_DIR_NAME);
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join(APP_DIR_NAME);
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join(APP_DIR_NAME);
    }
    PathBuf::from(APP_DIR_NAME)
}

/// Default per-user installation directory (no elevation required).
pub fn default_install_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(local).join("Programs").join(APP_DIR_NAME);
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("Programs").join(APP_DIR_NAME);
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("requestlab");
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        return PathBuf::from(home)
            .join(".local")
            .join("opt")
            .join("requestlab");
    }
    PathBuf::from(APP_DIR_NAME)
}

pub fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("manager.json")
}

pub fn work_dir(install_dir: &Path) -> PathBuf {
    install_dir.join(WORK_DIR)
}

pub fn staging_dir(install_dir: &Path) -> PathBuf {
    work_dir(install_dir).join("staging")
}

pub fn download_dir(install_dir: &Path) -> PathBuf {
    work_dir(install_dir).join("downloads")
}

pub fn backup_dir(install_dir: &Path) -> PathBuf {
    work_dir(install_dir).join("backup")
}

pub fn lock_path(install_dir: &Path) -> PathBuf {
    work_dir(install_dir).join(LOCK_FILE)
}

pub fn metadata_path(install_dir: &Path) -> PathBuf {
    install_dir.join(METADATA_FILE)
}

pub fn app_path(install_dir: &Path) -> PathBuf {
    install_dir.join(app_binary())
}

pub fn cli_path(install_dir: &Path) -> PathBuf {
    install_dir.join(cli_binary())
}

pub fn helper_path(install_dir: &Path) -> PathBuf {
    install_dir.join(helper_binary())
}

pub fn ensure_dir(path: &Path) -> ManagerResult<()> {
    std::fs::create_dir_all(path).map_err(|error| ManagerError::io(path, error))
}

pub fn remove_dir_if_exists(path: &Path) -> ManagerResult<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ManagerError::io(path, error)),
    }
}

pub fn remove_file_if_exists(path: &Path) -> ManagerResult<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ManagerError::io(path, error)),
    }
}

/// True when `child` is lexically inside `parent`.
pub fn is_within(parent: &Path, child: &Path) -> bool {
    if child
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return false;
    }
    let mut parent_components = parent.components();
    for component in child.components() {
        match parent_components.next() {
            Some(expected) => {
                let matches = match (expected, component) {
                    (Component::CurDir, Component::CurDir) => true,
                    (Component::Prefix(a), Component::Prefix(b)) => {
                        a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
                    }
                    (Component::RootDir, Component::RootDir) => true,
                    _ => expected == component,
                };
                if !matches {
                    return false;
                }
            }
            None => return true,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_stable() {
        let install = PathBuf::from("C:/Users/me/AppData/Local/Programs/RequestLab");
        assert_eq!(metadata_path(&install), install.join("install.json"));
        assert_eq!(
            lock_path(&install),
            install.join(".requestlab").join("install.lock")
        );
        assert!(is_within(&work_dir(&install), &staging_dir(&install)));
        assert!(is_within(&install, &app_path(&install)));
    }

    #[test]
    fn containment_rejects_escapes() {
        let root = PathBuf::from("C:/apps/requestlab");
        assert!(!is_within(
            &root,
            &PathBuf::from("C:/apps/requestlab/../evil")
        ));
        assert!(!is_within(&root, &PathBuf::from("C:/apps/other")));
        assert!(!is_within(&root, &root));
        assert!(is_within(&root, &root.join("bin").join("requestlab.exe")));
    }

    #[test]
    fn default_directories_are_distinct_and_non_empty() {
        assert_ne!(user_data_dir(), default_install_dir());
        assert!(!user_data_dir().as_os_str().is_empty());
    }
}
