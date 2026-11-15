//! Application core: service wiring, request execution and settings.

use std::path::PathBuf;
use std::time::Duration;

use requestlab_collections::CollectionService;
use requestlab_environments::EnvironmentService;
use requestlab_http::{
    prepare_request, HttpClient, HttpConfig, HttpError, SendOptions,
};
use requestlab_http::client::cancellation_channel;
use requestlab_models::{
    now_rfc3339, HistoryEntry, HistoryQuery, HttpMethod, HttpResponse, Request,
    DEFAULT_HISTORY_LIMIT,
};
use requestlab_storage::{
    repositories::{history as history_repo, settings as settings_repo},
    Storage, HISTORY_RETENTION_KEY, THEME_KEY, TIMEOUT_KEY,
};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::errors::{CoreError, ExecutionError};
use crate::state::{AppSettings, AppSnapshot, ExecutionOutcome};

/// The façade the UI talks to. Owns storage, HTTP and both services.
#[derive(Debug, Clone)]
pub struct AppService {
    storage: Storage,
    http: HttpClient,
    collections: CollectionService,
    environments: EnvironmentService,
    settings: AppSettings,
    recovered: Option<PathBuf>,
}

impl AppService {
    /// Open the database at `path` (defaulting to the platform data directory)
    /// and prepare every service.
    pub async fn open(path: Option<PathBuf>) -> Result<Self, CoreError> {
        let database_path = path.unwrap_or_else(requestlab_storage::default_database_path);
        let (storage, recovered) = requestlab_storage::open_or_recover(&database_path).await?;
        Self::from_storage(storage, recovered).await
    }

    /// An in-memory instance, used by tests and by the UI in demo mode.
    pub async fn open_in_memory() -> Result<Self, CoreError> {
        let storage = Storage::open_in_memory().await?;
        Self::from_storage(storage, None).await
    }

    async fn from_storage(
        storage: Storage,
        recovered: Option<PathBuf>,
    ) -> Result<Self, CoreError> {
        let settings = load_settings(&storage).await?;
        let http = HttpClient::new(HttpConfig {
            timeout: Duration::from_secs(settings.timeout_seconds),
            ..HttpConfig::default()
        })?;
        Ok(Self {
            collections: CollectionService::new(storage.clone()),
            environments: EnvironmentService::new(storage.clone()),
            storage,
            http,
            settings,
            recovered,
        })
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }

    pub fn collections(&self) -> &CollectionService {
        &self.collections
    }

    pub fn environments(&self) -> &EnvironmentService {
        &self.environments
    }

    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    /// The database file that was moved aside after a corruption, if any.
    pub fn recovered_database(&self) -> Option<&PathBuf> {
        self.recovered.as_ref()
    }

    /// Load everything needed for the first frame.
    pub async fn snapshot(&self) -> Result<AppSnapshot, CoreError> {
        Ok(AppSnapshot {
            collections: self.collections.list_trees().await?,
            environments: self.environments.list().await?,
            active_environment: self.environments.active_id().await?,
            history: self.history(&HistoryQuery::default()).await?,
            settings: self.settings.clone(),
        })
    }

    /// Run one request end to end, recording history whatever the outcome.
    ///
    /// `execution_id` lets the UI discard results from superseded sends.
    pub async fn send(
        &self,
        request: &Request,
        execution_id: Uuid,
        cancel: oneshot::Receiver<()>,
    ) -> ExecutionOutcome {
        let request_id = request.id;

        let resolved = match self.environments.resolve(request).await {
            Ok(resolved) => resolved,
            Err(err) => {
                let error = match err {
                    requestlab_environments::EnvironmentError::Resolution(failure) => {
                        ExecutionError::Resolution(failure)
                    }
                    other => ExecutionError::Environment(other.to_string()),
                };
                let history_id = self.record_failure(request, &error).await;
                return ExecutionOutcome {
                    execution_id,
                    request_id,
                    request: request.clone(),
                    resolved_request: None,
                    variables: Vec::new(),
                    result: Err(error),
                    history_id,
                };
            }
        };

        let options = SendOptions {
            timeout: Duration::from_secs(self.settings.timeout_seconds.clamp(1, 300)),
            ..SendOptions::default()
        };

        let prepared = match prepare_request(&resolved.request, &options) {
            Ok(prepared) => prepared,
            Err(HttpError::Validation(issues)) => {
                let error = ExecutionError::Validation(issues);
                let history_id = self.record_failure(&resolved.request, &error).await;
                return ExecutionOutcome {
                    execution_id,
                    request_id,
                    request: request.clone(),
                    resolved_request: Some(resolved.request),
                    variables: resolved.variables,
                    result: Err(error),
                    history_id,
                };
            }
            Err(other) => {
                let error = ExecutionError::Http(other);
                let history_id = self.record_failure(&resolved.request, &error).await;
                return ExecutionOutcome {
                    execution_id,
                    request_id,
                    request: request.clone(),
                    resolved_request: Some(resolved.request),
                    variables: resolved.variables,
                    result: Err(error),
                    history_id,
                };
            }
        };

        let result = self
            .http
            .execute(&prepared, cancel)
            .await
            .map_err(ExecutionError::Http);

        let history_id = self
            .record_attempt(&resolved.request, result.as_ref().err().map(|e| e.message()), result.as_ref().ok())
            .await;

        ExecutionOutcome {
            execution_id,
            request_id,
            request: request.clone(),
            resolved_request: Some(resolved.request),
            variables: resolved.variables,
            result,
            history_id,
        }
    }

    /// Convenience wrapper for callers that do not need cancellation.
    pub async fn send_simple(&self, request: &Request) -> ExecutionOutcome {
        let (_canceller, receiver) = cancellation_channel();
        self.send(request, Uuid::new_v4(), receiver).await
    }

    async fn record_attempt(
        &self,
        request: &Request,
        error: Option<String>,
        response: Option<&HttpResponse>,
    ) -> Option<Uuid> {
        let entry = HistoryEntry::from_attempt(
            request,
            response.map(|response| response.status),
            response.map(|response| response.duration_ms),
            error,
            now_rfc3339(),
        );
        match history_repo::insert(self.storage.pool(), &entry).await {
            Ok(()) => {
                let _ = history_repo::prune(self.storage.pool(), self.settings.history_limit).await;
                Some(entry.id)
            }
            Err(err) => {
                tracing::error!(error = %err, "could not record history");
                None
            }
        }
    }

    async fn record_failure(&self, request: &Request, error: &ExecutionError) -> Option<Uuid> {
        self.record_attempt(request, Some(error.message()), None).await
    }

    pub async fn history(&self, query: &HistoryQuery) -> Result<Vec<HistoryEntry>, CoreError> {
        Ok(history_repo::list(self.storage.pool(), query).await?)
    }

    pub async fn clear_history(&self) -> Result<u64, CoreError> {
        Ok(history_repo::clear(self.storage.pool()).await?)
    }

    pub async fn delete_history_entry(&self, id: Uuid) -> Result<(), CoreError> {
        Ok(history_repo::delete(self.storage.pool(), id).await?)
    }

    pub async fn history_entry(&self, id: Uuid) -> Result<HistoryEntry, CoreError> {
        Ok(history_repo::get(self.storage.pool(), id).await?)
    }

    pub async fn set_timeout_seconds(&mut self, seconds: u64) -> Result<(), CoreError> {
        let seconds = seconds.clamp(1, 300);
        self.settings.timeout_seconds = seconds;
        settings_repo::set(self.storage.pool(), TIMEOUT_KEY, &seconds.to_string()).await?;
        self.http = HttpClient::new(HttpConfig {
            timeout: Duration::from_secs(seconds),
            ..HttpConfig::default()
        })?;
        Ok(())
    }

    pub async fn set_history_limit(&mut self, limit: i64) -> Result<(), CoreError> {
        let limit = limit.clamp(50, 100_000);
        self.settings.history_limit = limit;
        settings_repo::set(self.storage.pool(), HISTORY_RETENTION_KEY, &limit.to_string()).await?;
        history_repo::prune(self.storage.pool(), limit).await?;
        Ok(())
    }

    pub async fn set_theme(&mut self, theme: &str) -> Result<(), CoreError> {
        self.settings.theme = if theme == "light" { "light" } else { "dark" }.to_string();
        settings_repo::set(self.storage.pool(), THEME_KEY, &self.settings.theme).await?;
        Ok(())
    }

    /// Default method for a brand new tab.
    pub fn default_request() -> Request {
        Request::new(HttpMethod::Get, "")
    }
}

async fn load_settings(storage: &Storage) -> Result<AppSettings, CoreError> {
    let pool = storage.pool();
    let mut settings = AppSettings::default();

    if let Some(raw) = settings_repo::get(pool, TIMEOUT_KEY).await? {
        if let Ok(seconds) = raw.parse::<u64>() {
            settings.timeout_seconds = seconds.clamp(1, 300);
        }
    }
    if let Some(raw) = settings_repo::get(pool, HISTORY_RETENTION_KEY).await? {
        if let Ok(limit) = raw.parse::<i64>() {
            settings.history_limit = limit.clamp(50, 100_000);
        }
    }
    if let Some(raw) = settings_repo::get(pool, THEME_KEY).await? {
        settings.theme = raw;
    } else {
        settings.history_limit = DEFAULT_HISTORY_LIMIT;
    }
    Ok(settings)
}
