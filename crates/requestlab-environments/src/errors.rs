//! Errors for environment management and variable resolution.

use requestlab_models::RequestField;
use requestlab_storage::StorageError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EnvironmentError {
    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error("An environment name is required")]
    EmptyName,

    #[error("{0}")]
    Invalid(String),

    #[error(transparent)]
    Resolution(#[from] ResolutionFailure),
}

/// A single variable-resolution problem.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResolutionError {
    #[error("Undefined variable {{{{{name}}}}}")]
    Undefined { name: String },

    #[error("Malformed variable expression: {0}")]
    Malformed(String),

    #[error("Variable {{{{{name}}}}} is disabled in the active environment")]
    Disabled { name: String },
}

/// One resolution problem together with where it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: RequestField,
    pub error: ResolutionError,
}

impl std::fmt::Display for FieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field.label(), self.error)
    }
}

/// Every resolution problem found in a request.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{}", problems.iter().map(|problem| problem.to_string()).collect::<Vec<_>>().join("; "))]
pub struct ResolutionFailure {
    pub problems: Vec<FieldError>,
}

impl ResolutionFailure {
    pub fn new(problems: Vec<FieldError>) -> Self {
        Self { problems }
    }

    /// The names of variables that could not be resolved.
    pub fn missing_variables(&self) -> Vec<String> {
        self.problems
            .iter()
            .filter_map(|problem| match &problem.error {
                ResolutionError::Undefined { name } => Some(name.clone()),
                _ => None,
            })
            .collect()
    }
}
