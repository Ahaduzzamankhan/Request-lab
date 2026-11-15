//! Plain snapshot types shared with the UI.
//!
//! These hold no GPUI types, so the core stays testable without a window.

use requestlab_models::{CollectionTree, Environment, HistoryEntry};
use uuid::Uuid;

/// Persisted user preferences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSettings {
    /// Per-request timeout in seconds.
    pub timeout_seconds: u64,
    /// Maximum number of history rows kept on disk.
    pub history_limit: i64,
    /// `dark` or `light`.
    pub theme: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            timeout_seconds: 30,
            history_limit: requestlab_models::DEFAULT_HISTORY_LIMIT,
            theme: "dark".to_string(),
        }
    }
}

/// Everything the UI needs to draw its first frame.
#[derive(Debug, Clone, Default)]
pub struct AppSnapshot {
    pub collections: Vec<CollectionTree>,
    pub environments: Vec<Environment>,
    pub active_environment: Option<Uuid>,
    pub history: Vec<HistoryEntry>,
    pub settings: AppSettings,
}

/// The outcome of one send, keyed by execution id so stale results can be dropped.
#[derive(Debug, Clone)]
pub struct ExecutionOutcome {
    pub execution_id: Uuid,
    pub request_id: Uuid,
    /// The request exactly as the user had it, unresolved.
    pub request: requestlab_models::Request,
    /// The request after environment resolution, if resolution succeeded.
    pub resolved_request: Option<requestlab_models::Request>,
    /// Variables substituted during resolution.
    pub variables: Vec<String>,
    pub result: Result<requestlab_models::HttpResponse, crate::errors::ExecutionError>,
    /// History row written for this attempt.
    pub history_id: Option<Uuid>,
}

impl ExecutionOutcome {
    pub fn status(&self) -> Option<u16> {
        self.result.as_ref().ok().map(|response| response.status)
    }

    pub fn is_error(&self) -> bool {
        self.result.is_err()
    }
}
