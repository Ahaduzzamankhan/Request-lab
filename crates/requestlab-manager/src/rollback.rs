//! Backups, transactional replacement and interrupted-update recovery.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{ManagerError, ManagerResult};
use crate::paths;
use crate::state::InstallationMetadata;
use crate::version::Version;

/// Marker written before files are replaced and removed after success.
pub const APPLY_MARKER: &str = "apply.json";
/// Record of the backup kept for the previous version.
pub const BACKUP_RECORD: &str = "backup.json";

/// What a replacement was doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyMarker {
    pub target_version: Version,
    pub previous_version: Option<Version>,
    pub started_at: DateTime<Utc>,
    pub pid: u32,
}

/// Record of the previous version kept for rollback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupRecord {
    pub version: Version,
    pub created_at: DateTime<Utc>,
    pub files: Vec<String>,
}

fn marker_path(install_dir: &Path) -> PathBuf {
    paths::work_dir(install_dir).join(APPLY_MARKER)
}

fn backup_record_path(install_dir: &Path) -> PathBuf {
    paths::backup_dir(install_dir).join(BACKUP_RECORD)
}

/// Write the apply marker before replacing the first file.
pub fn begin_apply(
    install_dir: &Path,
    target: &Version,
    previous: Option<&Version>,
) -> ManagerResult<ApplyMarker> {
    paths::ensure_dir(&paths::work_dir(install_dir))?;
    let marker = ApplyMarker {
        target_version: target.clone(),
        previous_version: previous.cloned(),
        started_at: Utc::now(),
        pid: std::process::id(),
    };
    let bytes = serde_json::to_vec_pretty(&marker).map_err(|error| {
        ManagerError::Other(format!("cannot serialise the apply marker: {error}"))
    })?;
    let path = marker_path(install_dir);
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, bytes).map_err(|error| ManagerError::io(&temporary, error))?;
    std::fs::rename(&temporary, &path).map_err(|error| ManagerError::io(&path, error))?;
    Ok(marker)
}

/// Remove the apply marker after the new installation is recorded.
pub fn finish_apply(install_dir: &Path) -> ManagerResult<()> {
    paths::remove_file_if_exists(&marker_path(install_dir))
}

/// The pending marker, if a previous replacement did not finish.
pub fn pending_apply(install_dir: &Path) -> ManagerResult<Option<ApplyMarker>> {
    let path = marker_path(install_dir);
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|error| {
            ManagerError::Other(format!("{} is unreadable: {error}", path.display()))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ManagerError::io(&path, error)),
    }
}

/// Copy the installed files into the backup directory.
pub fn create_backup(install_dir: &Path) -> ManagerResult<Option<BackupRecord>> {
    let Some(metadata) = InstallationMetadata::load_optional(install_dir)? else {
        return Ok(None);
    };

    let backup = paths::backup_dir(install_dir);
    paths::remove_dir_if_exists(&backup)?;
    paths::ensure_dir(&backup)?;

    let mut files = Vec::new();
    for file in &metadata.files {
        let source = install_dir.join(&file.path);
        if !source.is_file() {
            tracing::warn!(path = %source.display(), "not backing up a missing file");
            continue;
        }
        let destination = backup.join(&file.path);
        if let Some(parent) = destination.parent() {
            paths::ensure_dir(parent)?;
        }
        copy_file(&source, &destination)?;
        files.push(file.path.clone());
    }

    let record = BackupRecord {
        version: metadata.version.clone(),
        created_at: Utc::now(),
        files,
    };
    let bytes = serde_json::to_vec_pretty(&record).map_err(|error| {
        ManagerError::Other(format!("cannot serialise the backup record: {error}"))
    })?;
    std::fs::write(backup_record_path(install_dir), bytes)
        .map_err(|error| ManagerError::io(backup_record_path(install_dir), error))?;

    let previous_metadata = InstallationMetadata::path(install_dir);
    if previous_metadata.is_file() {
        copy_file(&previous_metadata, &backup.join(paths::METADATA_FILE))?;
    }

    Ok(Some(record))
}

/// Restore the most recent backup, returning the version now installed.
pub fn restore_backup(install_dir: &Path) -> ManagerResult<Version> {
    let record_path = backup_record_path(install_dir);
    let bytes = std::fs::read(&record_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ManagerError::Update(format!(
                "no previous version to restore: {} is missing",
                record_path.display()
            ))
        } else {
            ManagerError::io(&record_path, error)
        }
    })?;
    let record: BackupRecord = serde_json::from_slice(&bytes).map_err(|error| {
        ManagerError::Update(format!("{} is unreadable: {error}", record_path.display()))
    })?;

    let backup = paths::backup_dir(install_dir);
    for file in &record.files {
        let source = backup.join(file);
        let destination = install_dir.join(file);
        if !source.is_file() {
            return Err(ManagerError::Update(format!(
                "backup is incomplete: {} is missing",
                source.display()
            )));
        }
        if let Some(parent) = destination.parent() {
            paths::ensure_dir(parent)?;
        }
        copy_file(&source, &destination)?;
    }

    let previous_metadata = backup.join(paths::METADATA_FILE);
    if previous_metadata.is_file() {
        copy_file(&previous_metadata, &InstallationMetadata::path(install_dir))?;
    }

    Ok(record.version)
}

/// Copy a file, replacing the destination.
pub fn copy_file(source: &Path, destination: &Path) -> ManagerResult<()> {
    std::fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| ManagerError::io(destination, error))
}

/// Replace `destination` with `source`, restoring the previous file on failure.
pub fn replace_file(source: &Path, destination: &Path) -> ManagerResult<()> {
    if let Some(parent) = destination.parent() {
        paths::ensure_dir(parent)?;
    }
    let displaced = destination.with_extension("rlold");
    paths::remove_file_if_exists(&displaced)?;
    if destination.exists() {
        std::fs::rename(destination, &displaced)
            .map_err(|error| ManagerError::io(destination, error))?;
    }
    match std::fs::rename(source, destination) {
        Ok(()) => {
            paths::remove_file_if_exists(&displaced)?;
            Ok(())
        }
        Err(error) => {
            if displaced.exists() {
                let _ = std::fs::rename(&displaced, destination);
            }
            Err(ManagerError::io(destination, error))
        }
    }
}

/// Outcome of an interrupted-update check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recovery {
    Clean,
    CleanedUp,
    Restored(Version),
}

/// Recover from an interrupted update, if there was one.
pub fn recover_if_interrupted(install_dir: &Path) -> ManagerResult<Recovery> {
    let Some(marker) = pending_apply(install_dir)? else {
        paths::remove_dir_if_exists(&paths::staging_dir(install_dir))?;
        return Ok(Recovery::Clean);
    };

    tracing::warn!(
        target = %marker.target_version,
        previous = ?marker.previous_version,
        pid = marker.pid,
        "found an interrupted update"
    );

    let healthy = InstallationMetadata::load_optional(install_dir)?
        .map(|metadata| metadata.problems().is_empty())
        .unwrap_or(false);

    if healthy {
        finish_apply(install_dir)?;
        paths::remove_dir_if_exists(&paths::staging_dir(install_dir))?;
        return Ok(Recovery::CleanedUp);
    }

    if backup_record_path(install_dir).is_file() {
        let version = restore_backup(install_dir)?;
        finish_apply(install_dir)?;
        paths::remove_dir_if_exists(&paths::staging_dir(install_dir))?;
        return Ok(Recovery::Restored(version));
    }

    Err(ManagerError::Update(format!(
        "an update to {} was interrupted and no backup exists; reinstall RequestLab",
        marker.target_version
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Channel;
    use crate::state::InstalledFile;

    fn install_fake(install_dir: &Path, version: &str, contents: &[u8]) {
        paths::ensure_dir(install_dir).unwrap();
        std::fs::write(install_dir.join("requestlab.exe"), contents).unwrap();
        let metadata = InstallationMetadata::new(
            version.parse().unwrap(),
            Channel::Stable,
            install_dir,
            vec![InstalledFile {
                path: "requestlab.exe".to_string(),
                size: contents.len() as u64,
                sha256: crate::verifier::sha256_hex(contents),
            }],
        );
        metadata.save(install_dir).unwrap();
    }

    #[test]
    fn backup_and_restore_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        install_fake(root, "0.1.1", b"old version");

        assert_eq!(
            create_backup(root).unwrap().unwrap().files,
            vec!["requestlab.exe".to_string()]
        );
        std::fs::write(root.join("requestlab.exe"), b"broken").unwrap();
        assert_eq!(restore_backup(root).unwrap(), Version::new(0, 1, 1));
        assert_eq!(
            std::fs::read(root.join("requestlab.exe")).unwrap(),
            b"old version"
        );
        InstallationMetadata::load(root).unwrap().verify().unwrap();
    }

    #[test]
    fn restore_without_backup_explains_what_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        assert!(restore_backup(temp.path())
            .unwrap_err()
            .to_string()
            .contains("no previous version"));
    }

    #[test]
    fn replace_file_swaps_content_and_cleans_up() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("app.exe");
        let new = temp.path().join("app.exe.new");
        std::fs::write(&old, b"old").unwrap();
        std::fs::write(&new, b"new").unwrap();

        replace_file(&new, &old).unwrap();
        assert_eq!(std::fs::read(&old).unwrap(), b"new");
        assert!(!new.exists());
        assert!(!temp.path().join("app.rlold").exists());
    }

    #[test]
    fn recovery_clears_stale_scratch_and_healthy_markers() {
        let temp = tempfile::tempdir().unwrap();
        install_fake(temp.path(), "0.1.1", b"installed");
        paths::ensure_dir(&paths::staging_dir(temp.path())).unwrap();
        assert_eq!(
            recover_if_interrupted(temp.path()).unwrap(),
            Recovery::Clean
        );

        begin_apply(
            temp.path(),
            &Version::new(0, 2, 0),
            Some(&Version::new(0, 1, 1)),
        )
        .unwrap();
        assert_eq!(
            recover_if_interrupted(temp.path()).unwrap(),
            Recovery::CleanedUp
        );
        assert!(pending_apply(temp.path()).unwrap().is_none());
    }

    #[test]
    fn recovery_restores_a_broken_half_applied_update() {
        let temp = tempfile::tempdir().unwrap();
        install_fake(temp.path(), "0.1.1", b"old version");
        create_backup(temp.path()).unwrap();

        std::fs::write(temp.path().join("requestlab.exe"), b"new version").unwrap();
        begin_apply(
            temp.path(),
            &Version::new(0, 2, 0),
            Some(&Version::new(0, 1, 1)),
        )
        .unwrap();

        assert_eq!(
            recover_if_interrupted(temp.path()).unwrap(),
            Recovery::Restored(Version::new(0, 1, 1))
        );
        assert_eq!(
            std::fs::read(temp.path().join("requestlab.exe")).unwrap(),
            b"old version"
        );
    }
}
