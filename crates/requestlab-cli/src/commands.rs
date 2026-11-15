//! Command execution against the shared application service.

use std::path::PathBuf;

use anyhow::Context as _;
use requestlab_core::AppService;
use requestlab_models::{
    ApiKeyLocation, Authentication, HeaderEntry, HistoryQuery as ModelHistoryQuery,
    HistoryStatusFilter, HttpMethod, QueryParameter, Request, RequestBody,
};
use uuid::Uuid;

use crate::args::{CliAuth, Command, ConfigKey, GlobalOptions, Invocation, SendOptions};
use crate::output;

/// A failure that carries the process exit code to use.
#[derive(Debug)]
pub struct CliError {
    pub message: String,
    pub exit_code: i32,
}

impl CliError {
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 2,
        }
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 1,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

/// Rendered result of one command.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub text: String,
    pub exit_code: i32,
}

impl CommandOutput {
    fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            exit_code: 0,
        }
    }
}

/// Execute a parsed invocation.
pub async fn run(invocation: Invocation) -> Result<CommandOutput, CliError> {
    let Invocation { globals, command } = invocation;
    let service = open_service(&globals).await?;
    execute(service, globals.json, command).await
}

async fn open_service(globals: &GlobalOptions) -> Result<AppService, CliError> {
    let path = globals.data_dir.as_ref().map(PathBuf::from);
    AppService::open(path)
        .await
        .map_err(|error| CliError::runtime(format!("could not open the RequestLab data: {error}")))
}

async fn execute(
    mut service: AppService,
    json: bool,
    command: Command,
) -> Result<CommandOutput, CliError> {
    match command {
        Command::Help => Ok(CommandOutput::ok(crate::args::USAGE)),
        Command::Version => Ok(CommandOutput::ok(format!(
            "requestlab-cli {}",
            crate::VERSION
        ))),
        Command::Send(options) => send(&service, json, options).await,
        Command::CollectionsList => {
            let trees = service
                .collections()
                .list_trees()
                .await
                .map_err(runtime_error)?;
            Ok(CommandOutput::ok(if json {
                output::collections_json(&trees)
            } else {
                output::collections_text(&trees)
            }))
        }
        Command::CollectionsCreate { name } => {
            let collection = service
                .collections()
                .create_collection(&name)
                .await
                .map_err(runtime_error)?;
            Ok(CommandOutput::ok(if json {
                output::collection_created_json(&collection)
            } else {
                format!(
                    "Created collection '{}' ({})",
                    collection.name, collection.id
                )
            }))
        }
        Command::CollectionsDelete { id } => {
            let collection_id = parse_uuid(&id, "collection id")?;
            service
                .collections()
                .delete_collection(collection_id)
                .await
                .map_err(runtime_error)?;
            Ok(CommandOutput::ok(format!(
                "Deleted collection {collection_id}"
            )))
        }
        Command::HistoryList(query) => {
            let status = match query.status.as_str() {
                "2xx" => HistoryStatusFilter::Success,
                "3xx" => HistoryStatusFilter::Redirect,
                "4xx" => HistoryStatusFilter::ClientError,
                "5xx" => HistoryStatusFilter::ServerError,
                "failed" => HistoryStatusFilter::Failed,
                _ => HistoryStatusFilter::All,
            };
            let query = ModelHistoryQuery {
                search: query.search,
                method: query.method,
                status,
                limit: query.limit,
                offset: 0,
            };
            let rows = service.history(&query).await.map_err(runtime_error)?;
            Ok(CommandOutput::ok(if json {
                output::history_json(&rows)
            } else {
                output::history_text(&rows)
            }))
        }
        Command::HistoryClear => {
            let removed = service.clear_history().await.map_err(runtime_error)?;
            Ok(CommandOutput::ok(if json {
                serde_json::json!({ "removed": removed }).to_string()
            } else {
                format!("Cleared {removed} history entries")
            }))
        }
        Command::EnvironmentsList => {
            let environments = service.environments().list().await.map_err(runtime_error)?;
            let active = service
                .environments()
                .active_id()
                .await
                .map_err(runtime_error)?;
            Ok(CommandOutput::ok(if json {
                output::environments_json(&environments, active)
            } else {
                output::environments_text(&environments, active)
            }))
        }
        Command::EnvironmentsCreate { name } => {
            let environment = service
                .environments()
                .create(&name)
                .await
                .map_err(runtime_error)?;
            Ok(CommandOutput::ok(if json {
                serde_json::json!({ "id": environment.id, "name": environment.name }).to_string()
            } else {
                format!(
                    "Created environment '{}' ({})",
                    environment.name, environment.id
                )
            }))
        }
        Command::EnvironmentsUse { name } => {
            if name.eq_ignore_ascii_case("none") {
                service
                    .environments()
                    .set_active(None)
                    .await
                    .map_err(runtime_error)?;
                return Ok(CommandOutput::ok("No active environment"));
            }
            let environments = service.environments().list().await.map_err(runtime_error)?;
            let environment = environments
                .iter()
                .find(|environment| environment.name.eq_ignore_ascii_case(&name))
                .ok_or_else(|| CliError::usage(format!("no environment named '{name}'")))?;
            service
                .environments()
                .set_active(Some(environment.id))
                .await
                .map_err(runtime_error)?;
            Ok(CommandOutput::ok(format!(
                "Active environment: {}",
                environment.name
            )))
        }
        Command::EnvironmentsVariables { name } => {
            let environments = service.environments().list().await.map_err(runtime_error)?;
            let environment = environments
                .iter()
                .find(|environment| environment.name.eq_ignore_ascii_case(&name))
                .ok_or_else(|| CliError::usage(format!("no environment named '{name}'")))?;
            Ok(CommandOutput::ok(if json {
                output::variables_json(environment)
            } else {
                output::variables_text(environment)
            }))
        }
        Command::ConfigGet => {
            let settings = service.settings();
            Ok(CommandOutput::ok(if json {
                output::settings_json(settings)
            } else {
                format!(
                    "timeout = {}s\nhistory-limit = {}\ntheme = {}",
                    settings.timeout_seconds, settings.history_limit, settings.theme
                )
            }))
        }
        Command::ConfigSet { key, value } => {
            match key {
                ConfigKey::Timeout => {
                    let seconds = value.parse::<u64>().map_err(|_| {
                        CliError::usage(format!("'{value}' is not a number of seconds"))
                    })?;
                    if !(1..=600).contains(&seconds) {
                        return Err(CliError::usage("timeout must be between 1 and 600 seconds"));
                    }
                    service
                        .set_timeout_seconds(seconds)
                        .await
                        .map_err(runtime_error)?;
                }
                ConfigKey::HistoryLimit => {
                    let limit = value.parse::<i64>().map_err(|_| {
                        CliError::usage(format!("'{value}' is not a valid history limit"))
                    })?;
                    if !(10..=100_000).contains(&limit) {
                        return Err(CliError::usage(
                            "history-limit must be between 10 and 100000",
                        ));
                    }
                    service
                        .set_history_limit(limit)
                        .await
                        .map_err(runtime_error)?;
                }
                ConfigKey::Theme => {
                    if !matches!(value.as_str(), "dark" | "light") {
                        return Err(CliError::usage("theme must be 'dark' or 'light'"));
                    }
                    service.set_theme(&value).await.map_err(runtime_error)?;
                }
            }
            Ok(CommandOutput::ok(format!("{} = {}", key.as_str(), value)))
        }
    }
}

async fn send(
    service: &AppService,
    json: bool,
    options: SendOptions,
) -> Result<CommandOutput, CliError> {
    let mut request = Request::new(options.method, options.url.clone());
    request.headers = options
        .headers
        .iter()
        .map(|pair| HeaderEntry::new(pair.key.clone(), pair.value.clone()))
        .collect();
    request.query_params = options
        .params
        .iter()
        .map(|pair| QueryParameter::new(pair.key.clone(), pair.value.clone()))
        .collect();
    request.body = match options.body_kind {
        requestlab_models::RequestBodyKind::None => RequestBody::None,
        requestlab_models::RequestBodyKind::Json => RequestBody::Json {
            content: options.body.clone().unwrap_or_default(),
        },
        requestlab_models::RequestBodyKind::Raw => RequestBody::Raw {
            content: options.body.clone().unwrap_or_default(),
            content_type: "text/plain".to_string(),
        },
        requestlab_models::RequestBodyKind::FormUrlEncoded => RequestBody::FormUrlEncoded {
            fields: options
                .form
                .iter()
                .map(|pair| QueryParameter::new(pair.key.clone(), pair.value.clone()))
                .collect(),
        },
    };
    request.authentication = match options.auth.clone() {
        None => Authentication::None,
        Some(CliAuth::Basic { username, password }) => Authentication::Basic { username, password },
        Some(CliAuth::Bearer { token }) => Authentication::Bearer { token },
        Some(CliAuth::ApiKey {
            key,
            value,
            location,
        }) => Authentication::ApiKey {
            key,
            value,
            location: match location {
                ApiKeyLocation::Header => ApiKeyLocation::Header,
                ApiKeyLocation::Query => ApiKeyLocation::Query,
            },
        },
    };

    let issues = request.validate();
    if !issues.is_empty() {
        let message = issues
            .iter()
            .map(|issue| issue.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        return Err(CliError::usage(message));
    }

    if let Some(name) = &options.environment {
        let environments = service.environments().list().await.map_err(runtime_error)?;
        let environment = environments
            .iter()
            .find(|environment| environment.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| CliError::usage(format!("no environment named '{name}'")))?;
        service
            .environments()
            .set_active(Some(environment.id))
            .await
            .map_err(runtime_error)?;
    }

    let outcome = service.send_simple(&request).await;
    let exit_code = match &outcome.result {
        Ok(response) if options.fail_on_error_status && !response.is_success() => 22,
        Ok(_) => 0,
        Err(_) => 1,
    };

    let text = match &outcome.result {
        Ok(response) => {
            if json {
                output::response_json(response, options.print_body)
            } else {
                output::response_text(response, options.print_body)
            }
        }
        Err(error) => {
            let message = error.to_string();
            if json {
                serde_json::json!({
                    "ok": false,
                    "category": format!("{:?}", error.category()),
                    "error": message,
                })
                .to_string()
            } else {
                format!("Request failed: {message}")
            }
        }
    };

    Ok(CommandOutput { text, exit_code })
}

fn parse_uuid(value: &str, what: &str) -> Result<Uuid, CliError> {
    Uuid::parse_str(value.trim())
        .with_context(|| format!("'{value}' is not a valid {what}"))
        .map_err(|error| CliError::usage(error.to_string()))
}

fn runtime_error(error: impl std::fmt::Display) -> CliError {
    CliError::runtime(error.to_string())
}

/// Human readable name for a method, used by renderers and tests.
pub fn method_label(method: HttpMethod) -> &'static str {
    method.as_str()
}
