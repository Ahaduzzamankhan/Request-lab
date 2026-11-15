//! SQLite persistence for RequestLab.
//!
//! Owns the database file, the versioned schema migrations and every SQL query.
//! Nothing above this crate writes SQL or knows the table layout.

pub mod database;
pub mod errors;
pub mod migrations;
pub mod repositories;

pub use database::{
    default_data_dir, default_database_path, open_or_recover, Storage, ACTIVE_ENVIRONMENT_KEY,
    HISTORY_RETENTION_KEY, THEME_KEY, TIMEOUT_KEY,
};
pub use errors::StorageError;
/// Re-exported so downstream crates can hold a pool without depending on SQLx.
pub use sqlx::SqlitePool;
