//! Repository layer: one module per aggregate.

pub mod collections;
pub mod environments;
pub mod history;
pub mod settings;

use sqlx::sqlite::SqliteRow;
use sqlx::Row;
use uuid::Uuid;

use crate::errors::StorageError;

/// Read a UUID from a TEXT column.
pub(crate) fn uuid_at(row: &SqliteRow, column: &str) -> Result<Uuid, StorageError> {
    let raw: String = row.try_get(column)?;
    Uuid::parse_str(&raw).map_err(|err| StorageError::InvalidData(format!("{column}: {err}")))
}

/// Read an optional UUID from a nullable TEXT column.
pub(crate) fn opt_uuid_at(row: &SqliteRow, column: &str) -> Result<Option<Uuid>, StorageError> {
    let raw: Option<String> = row.try_get(column)?;
    match raw {
        None => Ok(None),
        Some(value) => Uuid::parse_str(&value)
            .map(Some)
            .map_err(|err| StorageError::InvalidData(format!("{column}: {err}"))),
    }
}

/// Read a required string column.
pub(crate) fn text_at(row: &SqliteRow, column: &str) -> Result<String, StorageError> {
    Ok(row.try_get(column)?)
}

/// Read an optional string column.
pub(crate) fn opt_text_at(row: &SqliteRow, column: &str) -> Result<Option<String>, StorageError> {
    Ok(row.try_get(column)?)
}

/// Read a boolean stored as INTEGER.
pub(crate) fn bool_at(row: &SqliteRow, column: &str) -> Result<bool, StorageError> {
    let value: i64 = row.try_get(column)?;
    Ok(value != 0)
}
