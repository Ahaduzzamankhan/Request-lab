//! Persistent state: what is installed, the operation lock, and manager settings.

use std::fs::File;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{ManagerError, ManagerResult};
use crate::manifest::{Channel, APP_ID};
use crate::paths;
use crate::verifier::sha256_file;
use crate::version::Version;

/// Metadata schema this build writes and understands.
pub const METADATA_SCHEMA: u32 = 1;

/// One installed file, relative to the installation directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

/// The record of one installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallationMetadata {
    pub schema: u32,
    pub app: String,
    pub version: Version,
    pub channel: Channel,
    pub installed_at: DateTime<Utc>,
    pub install_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_version: Option<Version>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes_url: Option<String>,
    pub files: Vec<InstalledFile>,
}

impl InstallationMetadata {
    pub fn new(
        version: Version,
        channel: Channel,
        install_dir: &Path,
        files: Vec<InstalledFile>,
    ) -> Self {
        Self {
            schema: METADATA_SCHEMA,
            app: APP_ID.to_string(),
            version,
            channel,
            installed_at: Utc::now(),
            install_dir: install_dir.display().to_string(),
            previous_version: None,
            notes_url: None,
            files,
        }
    }

    pub fn path(install_dir: &Path) -> PathBuf {
        paths::metadata_path(install_dir)
    }

    pub fn load_optional(install_dir: &Path) -> ManagerResult<Option<Self>> {
        let path = Self::path(install_dir);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(ManagerError::io(&path, error)),
        };
        let metadata: Self =
            serde_json::from_slice(&bytes).map_err(|error| ManagerError::Installation {
                path: install_dir.display().to_string(),
                detail: format!("{} is not readable: {error}", path.display()),
            })?;
        if metadata.schema != METADATA_SCHEMA {
            return Err(ManagerError::Installation {
                path: install_dir.display().to_string(),
                detail: format!("metadata schema {} is not supported", metadata.schema),
            });
        }
        Ok(Some(metadata))
    }

    pub fn load(install_dir: &Path) -> ManagerResult<Self> {
        Self::load_optional(install_dir)?.ok_or_else(|| ManagerError::Installation {
            path: install_dir.display().to_string(),
            detail: "no install.json - RequestLab is not installed here".to_string(),
        })
    }

    /// Write metadata atomically.
    pub fn save(&self, install_dir: &Path) -> ManagerResult<()> {
        paths::ensure_dir(install_dir)?;
        let path = Self::path(install_dir);
        let temporary = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| {
            ManagerError::Other(format!("cannot serialise install.json: {error}"))
        })?;
        {
            let mut file =
                File::create(&temporary).map_err(|error| ManagerError::io(&temporary, error))?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|error| ManagerError::io(&temporary, error))?;
        }
        std::fs::rename(&temporary, &path).map_err(|error| ManagerError::io(&path, error))
    }

    /// Problems found checking every recorded file against disk.
    pub fn problems(&self) -> Vec<String> {
        let root = PathBuf::from(&self.install_dir);
        let mut problems = Vec::new();
        for file in &self.files {
            let path = root.join(&file.path);
            match std::fs::metadata(&path) {
                Err(_) => problems.push(format!("{} is missing", file.path)),
                Ok(metadata) if metadata.len() != file.size => problems.push(format!(
                    "{} has size {} but should have {}",
                    file.path,
                    metadata.len(),
                    file.size
                )),
                Ok(_) => match sha256_file(&path) {
                    Ok(digest) if digest != file.sha256 => {
                        problems.push(format!("{} does not match its recorded digest", file.path))
                    }
                    Ok(_) => {}
                    Err(error) => problems.push(format!("{}: {error}", file.path)),
                },
            }
        }
        problems
    }

    /// Verify the installation, reporting every problem at once.
    pub fn verify(&self) -> ManagerResult<()> {
        let problems = self.problems();
        if problems.is_empty() {
            Ok(())
        } else {
            Err(ManagerError::Installation {
                path: self.install_dir.clone(),
                detail: problems.join("; "),
            })
        }
    }

    /// Record sizes and digests for installed files.
    pub fn record_files(
        root: &Path,
        relative_paths: &[String],
    ) -> ManagerResult<Vec<InstalledFile>> {
        let mut files = Vec::new();
        for relative in relative_paths {
            let path = root.join(relative);
            let metadata =
                std::fs::metadata(&path).map_err(|error| ManagerError::io(&path, error))?;
            files.push(InstalledFile {
                path: relative.clone(),
                size: metadata.len(),
                sha256: sha256_file(&path)?,
            });
        }
        files.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(files)
    }
}

/// Exclusive lock over one installation directory.
pub struct InstallationLock {
    path: PathBuf,
}

impl InstallationLock {
    /// Locks older than this are abandoned.
    pub const STALE_AFTER_HOURS: i64 = 2;

    pub fn acquire(install_dir: &Path, operation: &str) -> ManagerResult<Self> {
        let directory = paths::work_dir(install_dir);
        paths::ensure_dir(&directory)?;
        let path = paths::lock_path(install_dir);

        for attempt in 0..2 {
            match File::create_new(&path) {
                Ok(mut file) => {
                    let contents = format!(
                        "pid={}\noperation={}\ncreated={}\n",
                        std::process::id(),
                        operation,
                        Utc::now().to_rfc3339()
                    );
                    file.write_all(contents.as_bytes())
                        .and_then(|()| file.sync_all())
                        .map_err(|error| ManagerError::io(&path, error))?;
                    return Ok(Self { path });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let holder = std::fs::read_to_string(&path).unwrap_or_default();
                    if attempt == 0 && is_stale(&path, &holder) {
                        tracing::warn!(path = %path.display(), "reclaiming a stale installation lock");
                        paths::remove_file_if_exists(&path)?;
                        continue;
                    }
                    let summary = holder
                        .lines()
                        .filter(|line| line.starts_with("operation=") || line.starts_with("pid="))
                        .collect::<Vec<_>>()
                        .join(" ");
                    return Err(ManagerError::Locked(if summary.is_empty() {
                        path.display().to_string()
                    } else {
                        summary
                    }));
                }
                Err(error) => return Err(ManagerError::io(&path, error)),
            }
        }
        Err(ManagerError::Locked(path.display().to_string()))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for InstallationLock {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %self.path.display(), %error, "could not release the lock");
            }
        }
    }
}

fn is_stale(path: &Path, contents: &str) -> bool {
    if let Some(created) = contents
        .lines()
        .find_map(|line| line.strip_prefix("created="))
        .and_then(|value| DateTime::parse_from_rfc3339(value.trim()).ok())
    {
        let age = Utc::now().signed_duration_since(created.with_timezone(&Utc));
        return age.num_hours() >= InstallationLock::STALE_AFTER_HOURS;
    }
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .map(|age| age.as_secs() >= (InstallationLock::STALE_AFTER_HOURS as u64) * 3600)
        .unwrap_or(false)
}

/// Manager settings shared by every front end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagerConfig {
    pub channel: Channel,
    pub auto_check: bool,
    pub check_interval_hours: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<DateTime<Utc>>,
    /// Version the user chose to skip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped_version: Option<Version>,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            channel: Channel::Stable,
            auto_check: true,
            check_interval_hours: 24,
            install_dir: None,
            last_check: None,
            skipped_version: None,
        }
    }
}

impl ManagerConfig {
    pub fn path(data_dir: &Path) -> PathBuf {
        paths::config_path(data_dir)
    }

    pub fn load(data_dir: &Path) -> ManagerResult<Self> {
        let path = Self::path(data_dir);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => return Err(ManagerError::io(&path, error)),
        };
        serde_json::from_slice(&bytes).map_err(|error| {
            ManagerError::Other(format!(
                "{} is not valid settings JSON: {error}",
                path.display()
            ))
        })
    }

    pub fn save(&self, data_dir: &Path) -> ManagerResult<()> {
        paths::ensure_dir(data_dir)?;
        let path = Self::path(data_dir);
        let temporary = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| ManagerError::Other(format!("cannot serialise settings: {error}")))?;
        std::fs::write(&temporary, bytes).map_err(|error| ManagerError::io(&temporary, error))?;
        std::fs::rename(&temporary, &path).map_err(|error| ManagerError::io(&path, error))
    }

    pub fn is_check_due(&self, now: DateTime<Utc>) -> bool {
        if !self.auto_check {
            return false;
        }
        match self.last_check {
            None => true,
            Some(last) => {
                now.signed_duration_since(last).num_hours() >= self.check_interval_hours as i64
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::ensure_dir;

    #[test]
    fn metadata_round_trips_and_detects_tampering() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        ensure_dir(&root.join("bin")).unwrap();
        std::fs::write(root.join("bin/app.exe"), b"executable").unwrap();

        let files = InstallationMetadata::record_files(root, &["bin/app.exe".to_string()]).unwrap();
        let mut metadata =
            InstallationMetadata::new(Version::new(0, 2, 0), Channel::Stable, root, files);
        metadata.previous_version = Some(Version::new(0, 1, 1));
        metadata.save(root).unwrap();

        let loaded = InstallationMetadata::load(root).unwrap();
        assert_eq!(loaded.version, Version::new(0, 2, 0));
        assert_eq!(loaded.previous_version, Some(Version::new(0, 1, 1)));
        loaded.verify().unwrap();

        std::fs::write(root.join("bin/app.exe"), b"tampered!!").unwrap();
        let problems = loaded.problems();
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("digest"), "{problems:?}");
    }

    #[test]
    fn missing_metadata_is_reported() {
        let temp = tempfile::tempdir().unwrap();
        assert!(InstallationMetadata::load_optional(temp.path())
            .unwrap()
            .is_none());
        assert!(InstallationMetadata::load(temp.path()).is_err());
    }

    #[test]
    fn lock_is_exclusive_and_released_on_drop() {
        let temp = tempfile::tempdir().unwrap();
        let lock = InstallationLock::acquire(temp.path(), "install").unwrap();
        assert!(matches!(
            InstallationLock::acquire(temp.path(), "update"),
            Err(ManagerError::Locked(_))
        ));
        assert!(std::fs::read_to_string(lock.path())
            .unwrap()
            .contains("operation=install"));
        drop(lock);
        InstallationLock::acquire(temp.path(), "update").unwrap();
    }

    #[test]
    fn stale_locks_are_reclaimed() {
        let temp = tempfile::tempdir().unwrap();
        let path = paths::lock_path(temp.path());
        ensure_dir(path.parent().unwrap()).unwrap();
        let old = Utc::now() - chrono::Duration::hours(5);
        std::fs::write(
            &path,
            format!("pid=1\noperation=install\ncreated={}\n", old.to_rfc3339()),
        )
        .unwrap();
        InstallationLock::acquire(temp.path(), "update").unwrap();
    }

    #[test]
    fn config_interval_and_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        let mut config = ManagerConfig::default();
        assert!(config.is_check_due(Utc::now()));

        config.channel = Channel::Beta;
        config.skipped_version = Some("0.3.0".parse().unwrap());
        config.install_dir = Some("C:/Apps/RequestLab".to_string());
        config.last_check = Some(Utc::now() - chrono::Duration::hours(2));
        config.save(temp.path()).unwrap();
        assert_eq!(ManagerConfig::load(temp.path()).unwrap(), config);
        assert!(!config.is_check_due(Utc::now()));
    }
}
