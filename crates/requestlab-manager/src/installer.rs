//! Transactional installation, repair and uninstall.
//!
//! Files are copied next to their destination first, the previous installation is
//! backed up, and only then are files swapped in. Any failure restores the backup,
//! so an installation is never left half-replaced.

use std::path::{Path, PathBuf};

use crate::archive;
use crate::downloader::human_bytes;
use crate::error::{ManagerError, ManagerResult};
use crate::events::{EventSink, ManagerEvent, Stage};
use crate::manifest::{Channel, ReleaseManifest};
use crate::paths;
use crate::platform;
use crate::rollback;
use crate::state::{InstallationLock, InstallationMetadata};

/// How to install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOptions {
    pub install_dir: PathBuf,
    pub channel: Channel,
    /// Create desktop and start menu shortcuts.
    pub shortcuts: bool,
    /// Register an Add/Remove Programs entry.
    pub register_uninstall: bool,
}

impl InstallOptions {
    pub fn new(install_dir: impl Into<PathBuf>) -> Self {
        Self {
            install_dir: install_dir.into(),
            channel: Channel::Stable,
            shortcuts: true,
            register_uninstall: true,
        }
    }
}

/// Result of an installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallOutcome {
    pub metadata: InstallationMetadata,
    pub shortcuts: Vec<PathBuf>,
    /// Non-fatal problems (a shortcut or registry entry that could not be made).
    pub warnings: Vec<String>,
}

/// Why a directory cannot be used as an installation directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExistingInstallation {
    /// Nothing is installed there.
    None,
    /// A RequestLab installation exists; reinstall or update will replace it.
    RequestLab { version: crate::version::Version },
    /// The directory exists, is not empty, and holds something else.
    Occupied,
}

impl ExistingInstallation {
    pub fn detail(&self) -> String {
        match self {
            ExistingInstallation::None => "The directory is empty.".to_string(),
            ExistingInstallation::RequestLab { version } => {
                format!("RequestLab {version} is already installed here; it will be replaced.")
            }
            ExistingInstallation::Occupied => {
                "The directory contains other files. RequestLab will only add its own files."
                    .to_string()
            }
        }
    }
}

/// Detect an existing installation or unrelated content in a directory.
pub fn inspect_directory(install_dir: &Path) -> ExistingInstallation {
    if let Ok(Some(metadata)) = InstallationMetadata::load_optional(install_dir) {
        return ExistingInstallation::RequestLab {
            version: metadata.version,
        };
    }
    match std::fs::read_dir(install_dir) {
        Ok(mut entries) => {
            if entries.next().is_some() {
                ExistingInstallation::Occupied
            } else {
                ExistingInstallation::None
            }
        }
        Err(_) => ExistingInstallation::None,
    }
}

/// Verify free space for an unpacked package, when the platform reports it.
pub fn check_disk_space(install_dir: &Path, needed: u64) -> ManagerResult<()> {
    if let Some(available) = platform::free_space(install_dir) {
        // Keep a margin for the backup copy and filesystem overhead.
        let required = needed.saturating_mul(2).saturating_add(64 * 1024 * 1024);
        if available < required {
            return Err(ManagerError::DiskSpace {
                directory: install_dir.display().to_string(),
                needed: human_bytes(required),
                available: human_bytes(available),
            });
        }
    }
    Ok(())
}

/// Install from a verified package archive.
pub fn install_package(
    package_path: &Path,
    manifest: &ReleaseManifest,
    options: &InstallOptions,
    events: EventSink<'_>,
) -> ManagerResult<InstallOutcome> {
    let _lock = InstallationLock::acquire(&options.install_dir, "install")?;
    paths::ensure_dir(&options.install_dir)?;

    let staging = paths::staging_dir(&options.install_dir);
    paths::remove_dir_if_exists(&staging)?;

    events(ManagerEvent::stage(
        Stage::Extracting,
        "Unpacking the package",
    ));
    let extracted = archive::extract_zip(package_path, &staging, &mut |fraction| {
        events(ManagerEvent::Extracting {
            fraction: Some(fraction),
            bytes: 0,
        });
    })?;
    let root = archive::package_root(&staging)?;
    events(ManagerEvent::Warning(format!(
        "Extracted {} files",
        extracted.files.len()
    )));

    // Installing a package that was downloaded for this machine is the normal
    // path, but a caller may install an archive for another platform (a staging
    // machine, a test). Use the smallest declared package to bound disk usage.
    let needed = manifest
        .packages
        .iter()
        .map(|package| package.size)
        .min()
        .unwrap_or(0);
    check_disk_space(&options.install_dir, needed)?;

    let mut outcome =
        apply_extracted(&root, manifest, options, events, /* keep_lock */ false)?;
    outcome.warnings.push(format!(
        "Extracted {} files ({} in the package)",
        extracted.files.len(),
        human_bytes(extracted.total_bytes)
    ));
    paths::remove_dir_if_exists(&staging)?;
    Ok(outcome)
}

/// Apply already-extracted package files (used by the update helper).
pub fn apply_extracted(
    package_root: &Path,
    manifest: &ReleaseManifest,
    options: &InstallOptions,
    events: EventSink<'_>,
    _keep_lock: bool,
) -> ManagerResult<InstallOutcome> {
    let install_dir = &options.install_dir;
    paths::ensure_dir(install_dir)?;

    let previous = InstallationMetadata::load_optional(install_dir)?;
    if previous.is_some() {
        events(ManagerEvent::stage(
            Stage::Planning,
            "Backing up the installed version",
        ));
        rollback::create_backup(install_dir)?;
    }
    rollback::begin_apply(
        install_dir,
        &manifest.version,
        previous.as_ref().map(|m| &m.version),
    )?;

    events(ManagerEvent::stage(Stage::Installing, "Replacing files"));
    match commit_files(package_root, install_dir) {
        Ok(installed) => {
            let mut metadata = InstallationMetadata::new(
                manifest.version.clone(),
                manifest.channel,
                install_dir,
                installed,
            );
            metadata.previous_version = previous.as_ref().map(|m| m.version.clone());
            metadata.notes_url = manifest.notes_url.clone();
            metadata.save(install_dir)?;
            rollback::finish_apply(install_dir)?;
        }
        Err(error) => {
            events(ManagerEvent::stage(
                Stage::RollingBack,
                "Restoring the previous version",
            ));
            // Best effort: report the original failure, but leave a consistent tree.
            if let Err(restore) = rollback::restore_backup(install_dir) {
                tracing::error!(%restore, "rollback after a failed installation also failed");
            }
            rollback::finish_apply(install_dir)?;
            return Err(error);
        }
    }

    let metadata = InstallationMetadata::load(install_dir)?;
    let mut warnings = Vec::new();
    let mut shortcuts = Vec::new();

    if options.shortcuts {
        events(ManagerEvent::stage(
            Stage::CreatingShortcuts,
            "Creating shortcuts",
        ));
        match platform::create_shortcuts(install_dir) {
            Ok(created) => shortcuts = created,
            Err(error) => warnings.push(error.to_string()),
        }
    }
    if options.register_uninstall {
        events(ManagerEvent::stage(
            Stage::RegisteringUninstall,
            "Registering the uninstaller",
        ));
        if let Err(error) = platform::register_uninstall(install_dir, &metadata.version.to_string())
        {
            warnings.push(error.to_string());
        }
    }

    events(ManagerEvent::Installed {
        version: metadata.version.clone(),
        directory: install_dir.to_path_buf(),
    });
    Ok(InstallOutcome {
        metadata,
        shortcuts,
        warnings,
    })
}

/// Copy every file from the package into place, replacing existing files.
///
/// All new files are written beside their destination first; only when every copy
/// succeeded are they swapped in, so a failed copy never leaves a mixed tree.
fn commit_files(
    package_root: &Path,
    install_dir: &Path,
) -> ManagerResult<Vec<crate::state::InstalledFile>> {
    let files = collect_files(package_root)?;
    if files.is_empty() {
        return Err(ManagerError::UnsafeArchive(
            "package contains no files".to_string(),
        ));
    }

    let required = paths::app_binary();
    if !files.iter().any(|file| file == &required) {
        return Err(ManagerError::UnsafeArchive(format!(
            "package does not contain {required}"
        )));
    }

    let mut staged_paths = Vec::new();
    for relative in &files {
        let source = package_root.join(relative);
        let destination = install_dir.join(relative);
        let staged = staged_name(&destination);
        if let Some(parent) = staged.parent() {
            paths::ensure_dir(parent)?;
        }
        rollback::copy_file(&source, &staged)?;
        // Verify the copy before it can replace anything.
        let source_digest = crate::verifier::sha256_file(&source)?;
        let staged_digest = crate::verifier::sha256_file(&staged)?;
        if source_digest != staged_digest {
            paths::remove_file_if_exists(&staged)?;
            return Err(ManagerError::Checksum {
                file: relative.clone(),
                expected: source_digest,
                actual: staged_digest,
            });
        }
        staged_paths.push((staged, destination));
    }

    for (staged, destination) in staged_paths {
        rollback::replace_file(&staged, &destination)?;
    }

    InstallationMetadata::record_files(install_dir, &files)
}

fn staged_name(destination: &Path) -> PathBuf {
    let mut name = destination
        .file_name()
        .map(|value| value.to_os_string())
        .unwrap_or_default();
    name.push(".rlnew");
    destination.with_file_name(name)
}

/// Recursively list regular files, relative and `/` separated.
pub fn collect_files(root: &Path) -> ManagerResult<Vec<String>> {
    fn walk(root: &Path, current: &Path, out: &mut Vec<String>) -> ManagerResult<()> {
        let entries =
            std::fs::read_dir(current).map_err(|error| ManagerError::io(current, error))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let metadata = entry
                .metadata()
                .map_err(|error| ManagerError::io(&path, error))?;
            if metadata.is_dir() {
                if path.file_name().and_then(|name| name.to_str()) == Some(paths::WORK_DIR) {
                    continue;
                }
                walk(root, &path, out)?;
            } else if metadata.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| ManagerError::Other("path outside the package".to_string()))?;
                out.push(
                    relative
                        .components()
                        .map(|component| component.as_os_str().to_string_lossy().to_string())
                        .collect::<Vec<_>>()
                        .join("/"),
                );
            } else {
                return Err(ManagerError::UnsafeArchive(format!(
                    "{} is not a regular file",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    walk(root, root, &mut files)?;
    files.sort();
    Ok(files)
}

/// Verify an installation against its metadata.
pub fn verify_installation(install_dir: &Path) -> ManagerResult<InstallationMetadata> {
    let metadata = InstallationMetadata::load(install_dir)?;
    metadata.verify()?;
    Ok(metadata)
}

/// Remove an installation, keeping user data unless `purge_data` is set.
pub fn uninstall(
    install_dir: &Path,
    data_dir: &Path,
    purge_data: bool,
    events: EventSink<'_>,
) -> ManagerResult<Vec<String>> {
    let _lock = InstallationLock::acquire(install_dir, "uninstall")?;
    let metadata = InstallationMetadata::load_optional(install_dir)?;
    let mut removed = Vec::new();

    if let Some(metadata) = &metadata {
        for file in &metadata.files {
            let path = install_dir.join(&file.path);
            match paths::remove_file_if_exists(&path) {
                Ok(()) => removed.push(path.display().to_string()),
                Err(error) => tracing::warn!(%error, "could not remove an installed file"),
            }
        }
    }

    paths::remove_dir_if_exists(&paths::work_dir(install_dir))?;
    for error in platform::remove_shortcuts(install_dir) {
        tracing::warn!(%error, "could not remove a shortcut");
    }
    if let Err(error) = platform::unregister_uninstall() {
        tracing::warn!(%error, "could not remove the uninstall registry entry");
    }

    // Remove the directory itself only when nothing else lives in it.
    if let Ok(entries) = std::fs::read_dir(install_dir) {
        if entries.flatten().next().is_none() {
            let _ = std::fs::remove_dir(install_dir);
        }
    }

    if purge_data {
        events(ManagerEvent::stage(Stage::Finalizing, "Removing user data"));
        paths::remove_dir_if_exists(data_dir)?;
        removed.push(data_dir.display().to_string());
    }

    if metadata.is_none() && removed.is_empty() {
        return Err(ManagerError::Installation {
            path: install_dir.display().to_string(),
            detail: "no install.json - RequestLab is not installed here".to_string(),
        });
    }

    events(ManagerEvent::stage(Stage::Finalizing, "Uninstalled"));
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::tests::sample;
    use std::io::Write as _;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    fn package_with(files: &[(&str, &[u8])]) -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("package.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        for (name, contents) in files {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
        (temp, path)
    }

    fn options(dir: &Path) -> InstallOptions {
        InstallOptions {
            install_dir: dir.to_path_buf(),
            channel: Channel::Stable,
            shortcuts: false,
            register_uninstall: false,
        }
    }

    fn manifest() -> ReleaseManifest {
        ReleaseManifest::parse(sample().as_bytes()).unwrap()
    }

    #[test]
    fn installs_and_verifies_an_archive() {
        let (temp, package) = package_with(&[
            (crate::paths::app_binary().as_str(), b"app"),
            ("README.md", b"readme"),
        ]);
        let install_dir = temp.path().join("install");
        let outcome = install_package(&package, &manifest(), &options(&install_dir), &mut |_| {})
            .expect("install");

        assert_eq!(outcome.metadata.files.len(), 2);
        assert_eq!(
            std::fs::read(install_dir.join(crate::paths::app_binary())).unwrap(),
            b"app"
        );
        verify_installation(&install_dir).unwrap();
        assert!(!paths::staging_dir(&install_dir).exists());
    }

    #[test]
    fn reinstall_replaces_files_and_records_the_previous_version() {
        let (temp, package) = package_with(&[(crate::paths::app_binary().as_str(), b"v1")]);
        let install_dir = temp.path().join("install");
        install_package(&package, &manifest(), &options(&install_dir), &mut |_| {}).unwrap();

        let (temp2, package2) = package_with(&[(crate::paths::app_binary().as_str(), b"v2")]);
        let mut second = manifest();
        second.version = "0.3.0".parse().unwrap();
        let outcome =
            install_package(&package2, &second, &options(&install_dir), &mut |_| {}).unwrap();

        assert_eq!(outcome.metadata.version, "0.3.0".parse().unwrap());
        assert_eq!(
            outcome.metadata.previous_version,
            Some("0.2.0".parse().unwrap())
        );
        assert_eq!(
            std::fs::read(install_dir.join(crate::paths::app_binary())).unwrap(),
            b"v2"
        );
        let _ = temp2;
    }

    #[test]
    fn refuses_a_package_without_the_application_binary() {
        let (temp, package) = package_with(&[("notes.txt", b"no binary")]);
        let install_dir = temp.path().join("install");
        let error = install_package(&package, &manifest(), &options(&install_dir), &mut |_| {})
            .unwrap_err();
        assert!(error.to_string().contains("does not contain"), "{error}");
    }

    #[test]
    fn a_failed_install_restores_the_previous_files() {
        let (temp, package) = package_with(&[(crate::paths::app_binary().as_str(), b"good")]);
        let install_dir = temp.path().join("install");
        install_package(&package, &manifest(), &options(&install_dir), &mut |_| {}).unwrap();

        // A package whose second file cannot be written because a directory now
        // occupies its destination path.
        std::fs::create_dir_all(install_dir.join("README.md")).unwrap();
        let (temp2, bad) = package_with(&[
            (crate::paths::app_binary().as_str(), b"broken"),
            ("README.md", b"readme"),
        ]);
        let mut next = manifest();
        next.version = "0.3.0".parse().unwrap();
        let error = install_package(&bad, &next, &options(&install_dir), &mut |_| {}).unwrap_err();
        assert!(!error.to_string().is_empty());

        assert_eq!(
            std::fs::read(install_dir.join(crate::paths::app_binary())).unwrap(),
            b"good"
        );
        verify_installation(&install_dir).unwrap();
        let _ = temp2;
    }

    #[test]
    fn uninstall_removes_files_and_keeps_user_data_by_default() {
        let (temp, package) = package_with(&[(crate::paths::app_binary().as_str(), b"app")]);
        let install_dir = temp.path().join("install");
        let data_dir = temp.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(data_dir.join("requestlab.db"), b"user data").unwrap();
        install_package(&package, &manifest(), &options(&install_dir), &mut |_| {}).unwrap();

        let removed = uninstall(&install_dir, &data_dir, false, &mut |_| {}).unwrap();
        assert!(!removed.is_empty());
        assert!(!install_dir.join(crate::paths::app_binary()).exists());
        assert!(data_dir.join("requestlab.db").exists());

        // A second uninstall is idempotent and can purge the remaining data.
        uninstall(&install_dir, &data_dir, true, &mut |_| {}).unwrap();
        assert!(!data_dir.exists());
    }

    #[test]
    fn existing_installation_is_detected() {
        let temp = tempfile::tempdir().unwrap();
        let install_dir = temp.path().join("install");
        assert_eq!(inspect_directory(&install_dir), ExistingInstallation::None);
        std::fs::create_dir_all(&install_dir).unwrap();
        std::fs::write(install_dir.join("other.txt"), b"x").unwrap();
        assert_eq!(
            inspect_directory(&install_dir),
            ExistingInstallation::Occupied
        );

        let (package_temp, package) =
            package_with(&[(crate::paths::app_binary().as_str(), b"app")]);
        install_package(&package, &manifest(), &options(&install_dir), &mut |_| {}).unwrap();
        assert!(matches!(
            inspect_directory(&install_dir),
            ExistingInstallation::RequestLab { .. }
        ));
        let _ = package_temp;
    }
}
