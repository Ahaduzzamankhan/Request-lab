//! Environment management and `{{variable}}` resolution.
//!
//! Resolution never mutates the user's request: it returns a resolved copy plus
//! the list of variables that were substituted, so the editor can keep showing
//! the original `{{PLACEHOLDER}}` text.

pub mod errors;
pub mod manager;
pub mod resolver;
pub mod validation;

pub use errors::{EnvironmentError, ResolutionError};
pub use manager::EnvironmentService;
pub use resolver::{resolve_request, resolve_text, Resolution, ResolvedRequest};
pub use validation::validate_variables;
