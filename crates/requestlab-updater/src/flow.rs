//! Checking for and applying updates, shared by the window and the console modes.

use std::path::{Path, PathBuf};

use requestlab_manager::downloader::Downloader;
use requestlab_manager::events::{ManagerEvent, Stage};
use requestlab_manager::github::GitHubClient;
use requestlab_manager::installer::InstallOptions;
use requestlab_manager::manifest::Channel;
use requestlab_manager::state::InstallationMetadata;
use requestlab_manager::updater::{self, AvailableUpdate};
use requestlab_manager::version::Version;
use requestlab_manager::{paths, platform};

/// The installation directory the updater assumes when none is given.
pub fn suggested_install_dir() -> PathBuf {
    paths::default_install_dir()
}

/// The version installed in `install_dir`, or this build's version when the
/// directory holds no installation metadata.
pub fn installed_version(install_dir: &Path) -> Version {
    InstallationMetadata::load_optional(install_dir)
        .ok()
        .flatten()
        .map(|metadata| metadata.version)
        .unwrap_or_else(|| {
            requestlab_manager::VERSION
                .parse()
                .unwrap_or(Version::new(0, 0, 0))
        })
}

/// Ask GitHub for the newest stable release newer than the installed one.
pub async fn check(install_dir: &Path) -> Result<Option<AvailableUpdate>, String> {
    let client = GitHubClient::new().map_err(|error| error.to_string())?;
    let data_dir = paths::user_data_dir();
    let current = installed_version(install_dir);
    updater::check_for_update(&client, &data_dir, Channel::Stable, &current)
        .await
        .map_err(|error| format!("could not check for an update: {error}"))
}

/// Download, verify and apply the newest release.
///
/// With `pid`, the running application is left alone: the update is staged and
/// the helper waits for that process to exit before swapping files. Without it,
/// the files are replaced in this process, which requires RequestLab to be
/// closed.
pub async fn update<F: FnMut(ManagerEvent)>(
    install_dir: &Path,
    pid: Option<u32>,
    launch: bool,
    mut sink: F,
) -> Result<Version, String> {
    sink(ManagerEvent::stage(
        Stage::CheckingRelease,
        "Checking for updates",
    ));
    let available = check(install_dir).await?;
    let Some(available) = available else {
        return Err(format!(
            "RequestLab {} is already the newest version",
            installed_version(install_dir)
        ));
    };

    let downloader = Downloader::new().map_err(|error| error.to_string())?;
    let staged = updater::stage_update(&available, install_dir, &downloader, None, &mut sink)
        .await
        .map_err(|error| format!("could not prepare the update: {error}"))?;

    let helper_started = match pid.filter(|pid| *pid > 0) {
        Some(pid) => updater::launch_helper(install_dir, &staged, pid)
            .map_err(|error| format!("could not start the update helper: {error}"))?,
        None => false,
    };

    if helper_started {
        sink(ManagerEvent::stage(
            Stage::WaitingForExit,
            "RequestLab will restart to finish the update",
        ));
        return Ok(staged.version);
    }

    let options = InstallOptions {
        install_dir: install_dir.to_path_buf(),
        channel: staged.channel,
        shortcuts: true,
        register_uninstall: true,
    };
    let outcome = updater::apply_staged(&staged, install_dir, &options, &mut sink)
        .map_err(|error| format!("could not install the update: {error}"))?;

    if launch {
        let executable = paths::app_path(install_dir);
        platform::launch(&executable)
            .map_err(|error| format!("RequestLab was updated but could not start: {error}"))?;
    }

    Ok(outcome.metadata.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_directory_without_metadata_reports_this_build() {
        let temp = tempfile::tempdir().unwrap();
        let version = installed_version(temp.path());
        assert_eq!(version.to_string(), requestlab_manager::VERSION);
    }

    #[test]
    fn a_clean_directory_has_no_installation() {
        let temp = tempfile::tempdir().unwrap();
        assert!(InstallationMetadata::load_optional(temp.path())
            .unwrap()
            .is_none());
    }
}
