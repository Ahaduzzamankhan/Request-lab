//! SQL for the small key/value settings table.

use sqlx::SqlitePool;

use crate::errors::StorageError;

pub async fn get(pool: &SqlitePool, key: &str) -> Result<Option<String>, StorageError> {
    let value: Option<String> =
        sqlx::query_scalar("SELECT value FROM application_settings WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(value)
}

pub async fn set(pool: &SqlitePool, key: &str, value: &str) -> Result<(), StorageError> {
    sqlx::query(
        "INSERT INTO application_settings (key, value) VALUES (?, ?)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove(pool: &SqlitePool, key: &str) -> Result<(), StorageError> {
    sqlx::query("DELETE FROM application_settings WHERE key = ?")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}
