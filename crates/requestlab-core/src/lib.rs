//! RequestLab's application core.
//!
//! Owns service wiring and the request execution pipeline. It has no GPUI
//! dependency: the UI crate drives it from a background executor.

pub mod app_service;
pub mod errors;
pub mod state;

pub use app_service::AppService;
pub use errors::{CoreError, ExecutionError};
pub use state::{AppSettings, AppSnapshot, ExecutionOutcome};

// Re-exports so the UI can talk to the core without depending on the service
// crates directly.
pub use requestlab_collections::CollectionError;
pub use requestlab_environments::EnvironmentError;
pub use requestlab_http::{cancellation_channel, Canceller};
pub use requestlab_models::HttpResponse;
