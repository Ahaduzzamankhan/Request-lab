//! Errors surfaced by the application core.

use requestlab_environments::errors::ResolutionFailure;
use requestlab_http::HttpError;
use requestlab_models::ValidationIssue;
use requestlab_storage::StorageError;
use thiserror::Error;

/// Anything that can stop a request from producing a response.
#[derive(Debug, Clone, Error)]
pub enum ExecutionError {
    #[error("Validation failed")]
    Validation(Vec<ValidationIssue>),

    #[error("{}", .0)]
    Resolution(#[from] ResolutionFailure),

    #[error(transparent)]
    Http(#[from] HttpError),

    #[error("{0}")]
    Environment(String),
}

impl ExecutionError {
    pub fn category(&self) -> &'static str {
        match self {
            ExecutionError::Validation(_) => "Validation",
            ExecutionError::Resolution(_) => "Variables",
            ExecutionError::Environment(_) => "Environment",
            ExecutionError::Http(err) => err.category(),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, ExecutionError::Http(err) if err.is_cancelled())
    }

    /// A user-facing message; never contains credentials.
    pub fn message(&self) -> String {
        match self {
            ExecutionError::Validation(issues) => issues
                .iter()
                .map(|issue| issue.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            ExecutionError::Resolution(failure) => failure
                .problems
                .iter()
                .map(|problem| problem.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            ExecutionError::Http(err) => err.to_string(),
            ExecutionError::Environment(message) => message.clone(),
        }
    }

    pub fn validation_issues(&self) -> &[ValidationIssue] {
        match self {
            ExecutionError::Validation(issues) => issues,
            _ => &[],
        }
    }
}

/// Errors from the core services.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    Collections(#[from] requestlab_collections::CollectionError),

    #[error(transparent)]
    Environments(#[from] requestlab_environments::EnvironmentError),

    #[error(transparent)]
    Http(#[from] HttpError),

    #[error("{0}")]
    Invalid(String),
}
