//! Database file management and connection setup.

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::errors::StorageError;
use crate::migrations;

/// Settings key holding the selected environment id.
pub const ACTIVE_ENVIRONMENT_KEY: &str = "active_environment";
/// Settings key holding the history retention limit.
pub const HISTORY_RETENTION_KEY: &str = "history_retention";
/// Settings key holding the selected theme name.
pub const THEME_KEY: &str = "theme";
/// Settings key holding the default request timeout in seconds.
pub const TIMEOUT_KEY: &str = "request_timeout_seconds";

/// A live SQLite database with its schema migrated to the current version.
#[derive(Debug, Clone)]
pub struct Storage {
    pool: SqlitePool,
    path: Option<PathBuf>,
}

impl Storage {
    /// Open (or create) the database at `path`, enabling foreign keys and WAL.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(options)
            .await?;
        Self::finish(pool, Some(path)).await
    }

    /// An in-memory database, used by tests. Single connection so every query
    /// sees the same data.
    pub async fn open_in_memory() -> Result<Self, StorageError> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        Self::finish(pool, None).await
    }

    async fn finish(pool: SqlitePool, path: Option<PathBuf>) -> Result<Self, StorageError> {
        migrations::run(&pool).await?;
        Ok(Self { pool, path })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub async fn close(&self) {
        self.pool.close().await;
    }
}

/// Directory holding RequestLab's user data.
///
/// On Windows this is `%APPDATA%\RequestLab`; elsewhere the XDG data directory.
/// `REQUESTLAB_DATA_DIR` overrides it, which the tests and portable builds use.
pub fn default_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("REQUESTLAB_DATA_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("RequestLab");
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("RequestLab");
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".local").join("share").join("RequestLab");
    }
    PathBuf::from("RequestLab")
}

/// The default database file location.
pub fn default_database_path() -> PathBuf {
    default_data_dir().join("requestlab.db")
}

/// Open the database, recovering from a corrupt file once.
///
/// If the existing file cannot be opened, it is moved aside with a timestamped
/// name so the user keeps the data for inspection, and a fresh database is
/// created. The returned flag reports whether recovery happened.
pub async fn open_or_recover(path: impl AsRef<Path>) -> Result<(Storage, Option<PathBuf>), StorageError> {
    let path = path.as_ref().to_path_buf();
    match Storage::open(&path).await {
        Ok(storage) => Ok((storage, None)),
        Err(first_error) if path.exists() => {
            let stamp = chrono_like_stamp();
            let backup = path.with_extension(format!("corrupt-{stamp}.db"));
            tracing::error!(error = %first_error, "database could not be opened; moving aside");
            std::fs::rename(&path, &backup)?;
            for suffix in ["-wal", "-shm"] {
                let side = PathBuf::from(format!("{}{}", path.display(), suffix));
                if side.exists() {
                    let target = PathBuf::from(format!("{}{}", backup.display(), suffix));
                    let _ = std::fs::rename(side, target);
                }
            }
            let storage = Storage::open(&path).await?;
            Ok((storage, Some(backup)))
        }
        Err(error) => Err(error),
    }
}

fn chrono_like_stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", now.as_secs())
}
