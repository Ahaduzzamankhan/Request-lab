//! The installation itself, shared by the window and the silent mode.

use std::path::PathBuf;

use requestlab_manager::downloader::Downloader;
use requestlab_manager::events::ManagerEvent;
use requestlab_manager::github::GitHubClient;
use requestlab_manager::installer::InstallOptions;
use requestlab_manager::manifest::Channel;
use requestlab_manager::state::InstallationMetadata;
use requestlab_manager::updater;
use requestlab_manager::version::Version;
use requestlab_manager::{paths, platform};

/// Install the newest release on `channel` into `install_dir`.
///
/// The signed manifest is verified before a byte is downloaded, and the engine
/// keeps the previous installation until the new files are in place.
pub async fn install<F: FnMut(ManagerEvent)>(
    options: InstallOptions,
    launch: bool,
    mut sink: F,
) -> Result<InstallationMetadata, String> {
    let data_dir = paths::user_data_dir();
    let client = GitHubClient::new().map_err(|error| error.to_string())?;

    sink(ManagerEvent::stage(
        requestlab_manager::events::Stage::CheckingRelease,
        format!(
            "Looking for the newest {} release",
            options.channel.as_str()
        ),
    ));

    // A fresh installation has no previous version to compare against.
    let current = Version::new(0, 0, 0);
    let update = updater::check_for_update(&client, &data_dir, options.channel, &current)
        .await
        .map_err(|error| format!("could not check for a release: {error}"))?
        .ok_or_else(|| {
            format!(
                "no {} release is published yet, so there is nothing to install",
                options.channel.as_str()
            )
        })?;

    let downloader = Downloader::new().map_err(|error| error.to_string())?;
    let staged = updater::stage_update(&update, &options.install_dir, &downloader, None, &mut sink)
        .await
        .map_err(|error| format!("could not prepare the download: {error}"))?;

    let outcome = updater::apply_staged(&staged, &options.install_dir, &options, &mut sink)
        .map_err(|error| format!("could not install the files: {error}"))?;

    if launch {
        let executable = paths::app_path(&options.install_dir);
        platform::launch(&executable)
            .map_err(|error| format!("RequestLab was installed but could not start: {error}"))?;
    }

    Ok(outcome.metadata)
}

/// Where the installer suggests putting RequestLab.
pub fn suggested_install_dir() -> PathBuf {
    paths::default_install_dir()
}

/// The channel a fresh installation uses.
pub fn default_channel() -> Channel {
    Channel::Stable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_install_directory_is_under_the_user_profile() {
        let directory = suggested_install_dir();
        assert!(directory.is_absolute(), "{directory:?} should be absolute");
        assert!(directory
            .to_string_lossy()
            .to_lowercase()
            .contains("requestlab"));
        assert_eq!(default_channel(), Channel::Stable);
    }
}
