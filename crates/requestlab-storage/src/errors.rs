//! Typed storage errors.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{entity} was not found")]
    NotFound { entity: &'static str },

    #[error("Stored data is invalid: {0}")]
    InvalidData(String),
}

impl StorageError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, StorageError::NotFound { .. })
    }
}
