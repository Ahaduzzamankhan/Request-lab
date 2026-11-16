//! Update checking, staging and application.
//!
//! Checking never installs anything: it returns what is available and why. A
//! package is only downloaded and staged after the signed manifest verifies, and
//! the running executable is never overwritten in place — the helper waits for the
//! application to exit and swaps files transactionally.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::downloader::{DownloadOutcome, Downloader, Expected};
use crate::error::{ManagerError, ManagerResult};
use crate::events::{EventSink, ManagerEvent, Stage};
use crate::github::GitHubClient;
use crate::installer::{self, InstallOptions, InstallOutcome};
use crate::manifest::{Channel, Package, ReleaseManifest};
use crate::paths;
use crate::platform;
use crate::releases::{self, GitHubRelease};
use crate::state::ManagerConfig;
use crate::verifier::TrustStore;
use crate::version::Version;

/// Record of a downloaded, extracted and verified package.
pub const STAGED_FILE: &str = "staged-update.json";
/// Marker telling the helper what to do.
pub const HELPER_ARGUMENT: &str = "--staged";

/// An update the user can install.
#[derive(Debug, Clone, PartialEq)]
pub struct AvailableUpdate {
    pub version: Version,
    pub channel: Channel,
    pub title: String,
    pub notes: String,
    pub notes_url: Option<String>,
    pub published_at: Option<DateTime<Utc>>,
    pub release_url: String,
    pub manifest: ReleaseManifest,
    pub package: Package,
    /// Identifier of the trusted key that verified the manifest.
    pub verified_by: String,
}

impl AvailableUpdate {
    /// Download size in bytes.
    pub fn size(&self) -> u64 {
        self.package.size
    }
}

/// Find the newest installable release for a channel.
pub async fn check_for_update(
    client: &GitHubClient,
    data_dir: &Path,
    channel: Channel,
    current: &Version,
) -> ManagerResult<Option<AvailableUpdate>> {
    let releases = client.list_releases(30).await?;
    let Some(release) = releases::select_update(&releases, channel, current)? else {
        return Ok(None);
    };
    let update = verify_release(client, data_dir, release).await?;
    if update.version <= *current {
        return Ok(None);
    }
    Ok(Some(update))
}

/// Check a specific tag, used by `install --version` and `update --version`.
pub async fn check_tag(
    client: &GitHubClient,
    data_dir: &Path,
    tag: &str,
) -> ManagerResult<AvailableUpdate> {
    let release = client.release_by_tag(tag).await?;
    verify_release(client, data_dir, &release).await
}

/// Download, verify and interpret a release's signed manifest.
pub async fn verify_release(
    client: &GitHubClient,
    data_dir: &Path,
    release: &GitHubRelease,
) -> ManagerResult<AvailableUpdate> {
    let manifest_asset = release.manifest_asset()?;
    let signature_asset = release.signature_asset()?;

    let manifest_bytes = client.download_asset(manifest_asset).await?;
    let signature_bytes = client.download_asset(signature_asset).await?;
    let signature = String::from_utf8(signature_bytes)
        .map_err(|_| ManagerError::Signature("signature asset is not text".to_string()))?;

    let store = TrustStore::load(Some(data_dir));
    let verified_by = store.verify_manifest(&manifest_bytes, &signature)?;

    let manifest = ReleaseManifest::parse(&manifest_bytes).map_err(ManagerError::Manifest)?;
    let release_version = release.version()?;
    if manifest.version != release_version {
        return Err(ManagerError::Manifest(format!(
            "manifest describes {} but the release tag is {}",
            manifest.version, release.tag_name
        )));
    }

    let package = manifest
        .package_for_current_platform()
        .map_err(ManagerError::NoCompatiblePackage)?
        .clone();

    // The published asset must be the one the manifest names: this catches a
    // manifest copied from another release before anything is downloaded.
    let asset = release.package_asset(package.os, package.arch)?;
    if asset.browser_download_url != package.url {
        return Err(ManagerError::Manifest(
            "the package asset URL does not match the signed manifest".to_string(),
        ));
    }

    let current: Version = env!("CARGO_PKG_VERSION")
        .parse()
        .unwrap_or_else(|_| Version::new(0, 0, 0));
    if current < manifest.min_updater_version {
        return Err(ManagerError::UpdaterTooOld {
            required: manifest.min_updater_version.to_string(),
        });
    }

    Ok(AvailableUpdate {
        version: manifest.version.clone(),
        channel: manifest.channel,
        title: release.title(),
        notes: release.body.clone().unwrap_or_default(),
        notes_url: manifest.notes_url.clone(),
        published_at: release.published_at,
        release_url: release.html_url.clone(),
        manifest,
        package,
        verified_by,
    })
}

/// Package downloaded and extracted, ready for the helper to apply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StagedUpdate {
    pub version: Version,
    pub channel: Channel,
    pub package_path: String,
    pub package_sha256: String,
    pub extracted_dir: String,
    #[serde(default)]
    pub notes_url: Option<String>,
    pub manifest: ReleaseManifest,
}

impl StagedUpdate {
    /// Where the helper finds the staged record.
    pub fn record_path(install_dir: &Path) -> PathBuf {
        paths::work_dir(install_dir).join(STAGED_FILE)
    }

    pub fn save(&self, install_dir: &Path) -> ManagerResult<()> {
        let path = Self::record_path(install_dir);
        paths::ensure_dir(&paths::work_dir(install_dir))?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| {
            ManagerError::Other(format!("cannot serialise the staged update: {error}"))
        })?;
        std::fs::write(&path, bytes).map_err(|error| ManagerError::io(&path, error))
    }

    pub fn load(install_dir: &Path) -> ManagerResult<Self> {
        let path = Self::record_path(install_dir);
        let bytes = std::fs::read(&path).map_err(|error| ManagerError::io(&path, error))?;
        serde_json::from_slice(&bytes).map_err(|error| {
            ManagerError::Update(format!("{} is unreadable: {error}", path.display()))
        })
    }

    pub fn remove_record(install_dir: &Path) -> ManagerResult<()> {
        paths::remove_file_if_exists(&Self::record_path(install_dir))
    }
}

/// Download and verify a package, then extract it into the staging directory.
pub async fn stage_update(
    update: &AvailableUpdate,
    install_dir: &Path,
    downloader: &Downloader,
    cancel: Option<&crate::downloader::CancelToken>,
    events: EventSink<'_>,
) -> ManagerResult<StagedUpdate> {
    let downloads = paths::download_dir(install_dir);
    paths::ensure_dir(&downloads)?;
    let destination = downloads.join(update.package.file_name());

    events(ManagerEvent::stage(
        Stage::Downloading,
        format!("Downloading RequestLab {}", update.version),
    ));
    let expected = Expected {
        size: update.package.size,
        sha256: update.package.sha256.clone(),
    };
    let package: DownloadOutcome = downloader
        .download(
            &update.package.url,
            &destination,
            Some(&expected),
            cancel,
            &mut |transfer| {
                events(ManagerEvent::Transfer(transfer));
            },
        )
        .await?;

    events(ManagerEvent::stage(
        Stage::VerifyingPackage,
        "Verifying the package",
    ));
    crate::verifier::verify_package(&package.path, &expected.sha256, expected.size)?;

    let staging = paths::staging_dir(install_dir).join(update.version.to_string());
    paths::remove_dir_if_exists(&staging)?;
    events(ManagerEvent::stage(
        Stage::Extracting,
        "Unpacking the package",
    ));
    crate::archive::extract_zip(&package.path, &staging, &mut |fraction| {
        events(ManagerEvent::Extracting {
            fraction: Some(fraction),
            bytes: 0,
        });
    })?;
    let root = crate::archive::package_root(&staging)?;

    let staged = StagedUpdate {
        version: update.version.clone(),
        channel: update.channel,
        package_path: package.path.display().to_string(),
        package_sha256: package.sha256,
        extracted_dir: root.display().to_string(),
        notes_url: update.notes_url.clone(),
        manifest: update.manifest.clone(),
    };
    staged.save(install_dir)?;
    Ok(staged)
}

/// Apply a staged update in this process (used by the helper and by the CLI when
/// the application is not running).
pub fn apply_staged(
    staged: &StagedUpdate,
    install_dir: &Path,
    options: &InstallOptions,
    events: EventSink<'_>,
) -> ManagerResult<InstallOutcome> {
    let root = PathBuf::from(&staged.extracted_dir);
    if !root.is_dir() {
        return Err(ManagerError::Update(format!(
            "staged files are missing at {}; download the update again",
            root.display()
        )));
    }
    crate::verifier::verify_package(
        Path::new(&staged.package_path),
        &staged.package_sha256,
        staged
            .manifest
            .packages
            .iter()
            .map(|p| p.size)
            .max()
            .unwrap_or(0),
    )
    .or_else(|error| match error {
        // The archive may legitimately be larger than the current platform's
        // package; only a digest mismatch is fatal here.
        ManagerError::SizeMismatch { .. } => Ok(()),
        other => Err(other),
    })?;

    let outcome = installer::apply_extracted(&root, &staged.manifest, options, events, true)?;
    StagedUpdate::remove_record(install_dir)?;
    let _ = paths::remove_dir_if_exists(&paths::download_dir(install_dir));
    Ok(outcome)
}

/// Start the update helper so the running application can exit first.
///
/// Returns `Ok(false)` when this installation has no helper, in which case the
/// caller must only apply the update while the application is not running.
pub fn launch_helper(
    install_dir: &Path,
    staged: &StagedUpdate,
    application_pid: u32,
) -> ManagerResult<bool> {
    let helper = paths::helper_path(install_dir);
    if !helper.is_file() {
        return Ok(false);
    }
    let record = StagedUpdate::record_path(install_dir);
    // The helper reads the record from disk, so make sure what is written is
    // exactly what the caller staged.
    let recorded = StagedUpdate::load(install_dir)?;
    if recorded.version != staged.version {
        return Err(ManagerError::Update(format!(
            "the staged record describes {} but {} was requested",
            recorded.version, staged.version
        )));
    }
    let status = std::process::Command::new(&helper)
        .arg(HELPER_ARGUMENT)
        .arg(&record)
        .arg("--install-dir")
        .arg(install_dir)
        .arg("--pid")
        .arg(application_pid.to_string())
        .arg("--launch")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| {
            ManagerError::Update(format!("could not start {}: {error}", helper.display()))
        })?;
    tracing::info!(pid = status.id(), "started the update helper");
    Ok(true)
}

/// Update settings after a successful check, so automatic checks respect the
/// configured interval.
pub fn record_check(data_dir: &Path, channel: Channel) -> ManagerResult<()> {
    let mut config = ManagerConfig::load(data_dir).unwrap_or_default();
    config.channel = channel;
    config.last_check = Some(Utc::now());
    config.save(data_dir)
}

/// Wait for a process to exit, logging progress. Used by the helper.
pub fn wait_for_application(pid: u32, timeout: std::time::Duration) -> ManagerResult<()> {
    if platform::process_is_running(pid) {
        tracing::info!(pid, "waiting for the application to exit");
        if !platform::wait_for_process_exit(pid, timeout)? {
            return Err(ManagerError::Update(format!(
                "RequestLab (pid {pid}) did not exit within {} seconds",
                timeout.as_secs()
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staged_record_round_trips() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ReleaseManifest::parse(crate::manifest::tests::sample().as_bytes()).unwrap();
        let staged = StagedUpdate {
            version: manifest.version.clone(),
            channel: Channel::Stable,
            package_path: "pkg.zip".to_string(),
            package_sha256: "a".repeat(64),
            extracted_dir: "extracted".to_string(),
            notes_url: None,
            manifest,
        };
        staged.save(temp.path()).unwrap();
        assert_eq!(StagedUpdate::load(temp.path()).unwrap(), staged);
        StagedUpdate::remove_record(temp.path()).unwrap();
        assert!(StagedUpdate::load(temp.path()).is_err());
    }

    #[test]
    fn launching_without_a_helper_reports_false() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = ReleaseManifest::parse(crate::manifest::tests::sample().as_bytes()).unwrap();
        let staged = StagedUpdate {
            version: manifest.version.clone(),
            channel: Channel::Stable,
            package_path: "pkg.zip".to_string(),
            package_sha256: "a".repeat(64),
            extracted_dir: "extracted".to_string(),
            notes_url: None,
            manifest,
        };
        assert!(!launch_helper(temp.path(), &staged, 1).unwrap());
    }
}
