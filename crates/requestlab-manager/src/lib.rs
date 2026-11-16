//! RequestLab's installation and update engine.
//!
//! One implementation powers the desktop installer, the desktop updater, the
//! update helper and `requestlab-cli`. It discovers releases through the GitHub
//! API, trusts only a signed manifest, downloads and verifies packages, extracts
//! them safely and replaces an installation transactionally with a rollback
//! path.

pub mod archive;

/// Error alias module so submodules can `use crate::error::{ManagerError, ManagerResult}`.
pub mod error {
    pub use crate::{ManagerError, ManagerResult};
}
pub mod downloader;
pub mod events;
pub mod github;
pub mod installer;
pub mod manifest;
pub mod paths;
pub mod platform;
pub mod releases;
pub mod rollback;
pub mod state;
pub mod updater;
pub mod verifier;
pub mod version;

use std::path::Path;

pub use manifest::{Architecture, Channel, OperatingSystem, ReleaseManifest};
pub use state::{InstallationMetadata, ManagerConfig};
pub use version::Version;

/// Application identifier used in manifests and metadata.
pub const APP_ID: &str = "requestlab";
/// Repository owner for release discovery.
pub const REPO_OWNER: &str = "Ahaduzzamankhan";
/// Repository name for release discovery.
pub const REPO_NAME: &str = "Request-lab";
/// Version this build was compiled from.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Everything the engine can fail with. Messages are written for end users.
#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("network error: {0}")]
    Network(String),
    #[error("GitHub API error: {0}")]
    GitHub(String),
    #[error("GitHub API rate limit reached; retry after {retry_after}")]
    RateLimited { retry_after: String },
    #[error("release '{0}' was not found")]
    ReleaseNotFound(String),
    #[error("release {version} has no asset named '{asset}'")]
    AssetMissing { version: String, asset: String },
    #[error("no compatible package: {0}")]
    NoCompatiblePackage(String),
    #[error("invalid release manifest: {0}")]
    Manifest(String),
    #[error("release signature could not be verified: {0}")]
    Signature(String),
    #[error("checksum mismatch for {file}: expected {expected}, got {actual}")]
    Checksum {
        file: String,
        expected: String,
        actual: String,
    },
    #[error("downloaded {file} is {actual} bytes but the manifest says {expected}")]
    SizeMismatch {
        file: String,
        expected: u64,
        actual: u64,
    },
    #[error("archive entry '{0}' is not safe to extract")]
    UnsafeArchive(String),
    #[error("the download was cancelled")]
    Cancelled,
    #[error("io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0} is not supported on this platform")]
    Unsupported(String),
    #[error("not enough free disk space in {directory}: need {needed}, available {available}")]
    DiskSpace {
        directory: String,
        needed: String,
        available: String,
    },
    #[error("another RequestLab operation is already running ({0})")]
    Locked(String),
    #[error("the RequestLab installation at {path} is unusable: {detail}")]
    Installation { path: String, detail: String },
    #[error("update could not be applied: {0}")]
    Update(String),
    #[error("this release needs a newer updater ({required}); update RequestLab first")]
    UpdaterTooOld { required: String },
    #[error("{0}")]
    Other(String),
}

impl ManagerError {
    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        ManagerError::Io {
            path: path.as_ref().display().to_string(),
            source,
        }
    }

    /// Whether retrying later could succeed unchanged.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            ManagerError::Network(_) | ManagerError::RateLimited { .. }
        )
    }
}

/// Result alias for the engine.
pub type ManagerResult<T> = Result<T, ManagerError>;
