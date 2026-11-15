//! Errors raised by collection operations.

use requestlab_models::MAX_FOLDER_DEPTH;
use requestlab_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CollectionError {
    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error("A name is required")]
    EmptyName,

    #[error("Folder nesting is limited to {MAX_FOLDER_DEPTH} levels")]
    TooDeep,

    #[error("A folder cannot be moved inside itself")]
    InvalidMove,

    #[error("{0}")]
    Invalid(String),
}

impl CollectionError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, CollectionError::Storage(err) if err.is_not_found())
    }
}

/// Trim and validate a user-supplied name.
pub fn normalized_name(name: &str) -> Result<String, CollectionError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(CollectionError::EmptyName);
    }
    if trimmed.chars().count() > 120 {
        return Err(CollectionError::Invalid(
            "Names are limited to 120 characters".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}
