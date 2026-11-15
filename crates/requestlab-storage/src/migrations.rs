//! Versioned schema migrations.
//!
//! The applied version lives in SQLite's `PRAGMA user_version`, so no extra
//! bookkeeping table is needed. Each migration runs inside a transaction.

use sqlx::SqlitePool;

use crate::errors::StorageError;

pub struct Migration {
    pub version: i64,
    pub statements: &'static [&'static str],
}

/// Append-only list of migrations. Never edit a shipped migration.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    statements: &[
        r#"
        CREATE TABLE collections (
            id          TEXT PRIMARY KEY NOT NULL,
            name        TEXT NOT NULL,
            description TEXT,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        )
        "#,
        r#"
        CREATE TABLE folders (
            id            TEXT PRIMARY KEY NOT NULL,
            collection_id TEXT NOT NULL,
            parent_id     TEXT,
            name          TEXT NOT NULL,
            sort_order    INTEGER NOT NULL DEFAULT 0,
            created_at    TEXT NOT NULL,
            updated_at    TEXT NOT NULL,
            FOREIGN KEY (collection_id) REFERENCES collections (id) ON DELETE CASCADE,
            FOREIGN KEY (parent_id) REFERENCES folders (id) ON DELETE CASCADE
        )
        "#,
        r#"
        CREATE TABLE saved_requests (
            id            TEXT PRIMARY KEY NOT NULL,
            collection_id TEXT NOT NULL,
            folder_id     TEXT,
            name          TEXT NOT NULL,
            request_json  TEXT NOT NULL,
            sort_order    INTEGER NOT NULL DEFAULT 0,
            created_at    TEXT NOT NULL,
            updated_at    TEXT NOT NULL,
            FOREIGN KEY (collection_id) REFERENCES collections (id) ON DELETE CASCADE,
            FOREIGN KEY (folder_id) REFERENCES folders (id) ON DELETE SET NULL
        )
        "#,
        r#"
        CREATE TABLE request_history (
            id            TEXT PRIMARY KEY NOT NULL,
            method        TEXT NOT NULL,
            url           TEXT NOT NULL,
            request_json  TEXT NOT NULL,
            status_code   INTEGER,
            duration_ms   INTEGER,
            error_message TEXT,
            created_at    TEXT NOT NULL
        )
        "#,
        r#"
        CREATE TABLE environments (
            id         TEXT PRIMARY KEY NOT NULL,
            name       TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        )
        "#,
        r#"
        CREATE TABLE environment_variables (
            id             TEXT PRIMARY KEY NOT NULL,
            environment_id TEXT NOT NULL,
            key            TEXT NOT NULL,
            value          TEXT NOT NULL,
            enabled        INTEGER NOT NULL DEFAULT 1,
            is_secret      INTEGER NOT NULL DEFAULT 0,
            position       INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (environment_id) REFERENCES environments (id) ON DELETE CASCADE
        )
        "#,
        r#"
        CREATE TABLE application_settings (
            key   TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        )
        "#,
        "CREATE INDEX idx_folders_collection ON folders (collection_id, parent_id, sort_order)",
        "CREATE INDEX idx_saved_requests_collection ON saved_requests (collection_id, folder_id, sort_order)",
        "CREATE INDEX idx_saved_requests_folder ON saved_requests (folder_id, sort_order)",
        "CREATE INDEX idx_history_created ON request_history (created_at DESC)",
        "CREATE INDEX idx_history_method ON request_history (method)",
        "CREATE INDEX idx_history_status ON request_history (status_code)",
        "CREATE INDEX idx_env_vars_environment ON environment_variables (environment_id, position)",
    ],
}];

/// The schema version this build expects.
pub fn latest_version() -> i64 {
    MIGRATIONS
        .last()
        .map(|migration| migration.version)
        .unwrap_or(0)
}

/// Read the schema version recorded in the database file.
pub async fn current_version(pool: &SqlitePool) -> Result<i64, StorageError> {
    let version: i64 = sqlx::query_scalar("PRAGMA user_version")
        .fetch_one(pool)
        .await?;
    Ok(version)
}

/// Apply every migration newer than the stored version.
pub async fn run(pool: &SqlitePool) -> Result<i64, StorageError> {
    let mut version = current_version(pool).await?;
    let pending: Vec<&Migration> = MIGRATIONS
        .iter()
        .filter(|migration| migration.version > version)
        .collect();
    for migration in pending {
        tracing::info!(version = migration.version, "applying database migration");
        let mut tx = pool.begin().await?;
        for statement in migration.statements {
            sqlx::query(statement).execute(&mut *tx).await?;
        }
        // PRAGMA does not accept bound parameters; the value is a compile-time
        // constant from MIGRATIONS, never user input.
        sqlx::query(&format!("PRAGMA user_version = {}", migration.version))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        version = migration.version;
    }
    Ok(version)
}
