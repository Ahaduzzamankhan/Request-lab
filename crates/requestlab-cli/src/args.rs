//! Argument parsing for the RequestLab CLI.
//!
//! The grammar is intentionally small and explicit so that it can be tested
//! without touching the network, the database or the process environment.

use requestlab_models::{ApiKeyLocation, HttpMethod, RequestBodyKind};

/// Top-level command selected on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Send(SendOptions),
    /// Parse a cURL command and print the request without sending it.
    Curl {
        command: String,
    },
    CollectionsList,
    CollectionsCreate {
        name: String,
    },
    CollectionsDelete {
        id: String,
    },
    HistoryList(HistoryQuery),
    HistoryClear,
    EnvironmentsList,
    EnvironmentsCreate {
        name: String,
    },
    EnvironmentsVariables {
        name: String,
    },
    EnvironmentsSet {
        name: String,
        key: String,
        value: String,
        secret: bool,
        disabled: bool,
    },
    EnvironmentsUnset {
        name: String,
        key: String,
    },
    EnvironmentsUse {
        name: String,
    },
    ConfigGet,
    ConfigSet {
        key: ConfigKey,
        value: String,
    },
    Help,
    Version,
}

/// A single `--header`, `--param` or `--form` pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    pub key: String,
    pub value: String,
}

impl Pair {
    /// Parse `key=value` or `key: value`. Returns a usage error for a missing
    /// separator so callers never send a silently wrong header.
    pub fn parse(input: &str, separator: char) -> Result<Self, String> {
        match input.split_once(separator) {
            Some((key, value)) if !key.trim().is_empty() => Ok(Self {
                key: key.trim().to_string(),
                value: value.trim().to_string(),
            }),
            Some(_) => Err(format!("missing name before '{separator}' in '{input}'")),
            None => Err(format!(
                "expected NAME{separator}VALUE, for example 'Accept{separator} application/json'"
            )),
        }
    }
}

/// Supported authentication selections on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAuth {
    Basic {
        username: String,
        password: String,
    },
    Bearer {
        token: String,
    },
    ApiKey {
        key: String,
        value: String,
        location: ApiKeyLocation,
    },
}

/// Options gathered for a `send` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendOptions {
    pub url: String,
    pub method: HttpMethod,
    pub headers: Vec<Pair>,
    pub params: Vec<Pair>,
    pub form: Vec<Pair>,
    pub body: Option<String>,
    pub body_kind: RequestBodyKind,
    pub auth: Option<CliAuth>,
    pub environment: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub print_body: bool,
    pub fail_on_error_status: bool,
    /// A cURL command to import instead of the individual options.
    pub curl: Option<String>,
}

impl Default for SendOptions {
    fn default() -> Self {
        Self {
            url: String::new(),
            method: HttpMethod::Get,
            headers: Vec::new(),
            params: Vec::new(),
            form: Vec::new(),
            body: None,
            body_kind: RequestBodyKind::None,
            auth: None,
            environment: None,
            timeout_seconds: None,
            print_body: true,
            fail_on_error_status: false,
            curl: None,
        }
    }
}

/// Options gathered for a `history list` invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryQuery {
    pub limit: i64,
    pub method: Option<HttpMethod>,
    pub status: String,
    pub search: String,
}

impl Default for HistoryQuery {
    fn default() -> Self {
        Self {
            limit: 20,
            method: None,
            status: "all".to_string(),
            search: String::new(),
        }
    }
}

/// Settings that can be written with `config set`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKey {
    Timeout,
    HistoryLimit,
    Theme,
}

impl ConfigKey {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "timeout" => Ok(ConfigKey::Timeout),
            "history-limit" => Ok(ConfigKey::HistoryLimit),
            "theme" => Ok(ConfigKey::Theme),
            other => Err(format!(
                "unknown setting '{other}' (expected timeout, history-limit or theme)"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            ConfigKey::Timeout => "timeout",
            ConfigKey::HistoryLimit => "history-limit",
            ConfigKey::Theme => "theme",
        }
    }
}

/// Global switches that apply to every command.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GlobalOptions {
    pub json: bool,
    pub data_dir: Option<String>,
}

/// The complete parsed invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub globals: GlobalOptions,
    pub command: Command,
}

pub const USAGE: &str = "\
RequestLab CLI - send requests, inspect history and manage collections.

USAGE:
    requestlab-cli [--json] [--data-dir PATH] <command> [options]

COMMANDS:
    send <url>                    Send an HTTP request
    curl <command>                Show how a cURL command is understood
    collections list              List collections
    collections create <name>     Create a collection
    collections delete <id>       Delete a collection
    history list                  List recent requests
    history clear                 Delete all history
    environments list             List environments
    environments create <name>    Create an environment
    environments use <name|none>  Select the active environment
    environments variables <name> Show an environment's variables
    environments set <name> <K=V>  Add or update a variable (--secret, --disabled)
    environments unset <name> <K>  Remove a variable
    config get                    Show stored preferences
    config set <key> <value>      Update timeout, history-limit or theme

SEND OPTIONS:
    -X, --method <METHOD>         GET (default), POST, PUT, PATCH, DELETE, HEAD, OPTIONS
    -H, --header <NAME: VALUE>    Add a request header (repeatable)
    -p, --param <NAME=VALUE>      Add a query parameter (repeatable)
    -d, --data <TEXT>             Request body
        --body-type <KIND>        json (default), raw or form
    -F, --form <NAME=VALUE>       Add a form field (implies --body-type form)
        --bearer <TOKEN>          Bearer token authentication
        --basic <USER:PASSWORD>   Basic authentication
        --api-key <KEY=VALUE>     API key; add @query to send it as a query parameter
    -e, --env <NAME>              Environment to resolve {{VARIABLES}} with
    -t, --timeout <SECONDS>       Override the stored timeout
        --no-body                 Do not print the response body
    -f, --fail                    Exit with status 22 on a 4xx/5xx response
        --curl <COMMAND>          Import a cURL command instead of the flags

HISTORY OPTIONS:
        --limit <N>               Rows to show (default 20)
        --method <METHOD>         Filter by HTTP method
        --status <FILTER>         all, 2xx, 3xx, 4xx, 5xx or failed
        --search <TEXT>           Filter by URL or error text

GLOBAL OPTIONS:
        --json                    Machine-readable JSON output
        --data-dir <PATH>         Use an alternative data directory
    -h, --help                    Show this help
    -V, --version                 Show the version

EXIT CODES:
    0  success            1  runtime or network failure
    2  usage error        22 HTTP error response with --fail
";

/// Parse a full command line (without the executable name).
pub fn parse<I, S>(args: I) -> Result<Invocation, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut globals = GlobalOptions::default();
    let mut rest: Vec<String> = Vec::new();

    let mut args = args.into_iter().map(Into::into).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => globals.json = true,
            "--data-dir" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--data-dir needs a path".to_string())?;
                globals.data_dir = Some(value);
            }
            "-h" | "--help" => {
                return Ok(Invocation {
                    globals,
                    command: Command::Help,
                })
            }
            "-V" | "--version" => {
                return Ok(Invocation {
                    globals,
                    command: Command::Version,
                })
            }
            other => rest.push(other.to_string()),
        }
    }

    let command = parse_command(&rest)?;
    Ok(Invocation { globals, command })
}

fn parse_command(args: &[String]) -> Result<Command, String> {
    let Some(head) = args.first() else {
        return Ok(Command::Help);
    };
    let tail = &args[1..];
    match head.as_str() {
        "help" => Ok(Command::Help),
        "send" => parse_send(tail),
        "curl" => {
            if tail.is_empty() {
                return Err("curl needs a command, for example: curl \"curl https://example.com\"".to_string());
            }
            Ok(Command::Curl {
                command: tail.join(" "),
            })
        }
        "collections" => match tail.first().map(String::as_str) {
            Some("list") => Ok(Command::CollectionsList),
            Some("create") => Ok(Command::CollectionsCreate {
                name: require_positional(tail, 1, "collection name")?,
            }),
            Some("delete") => Ok(Command::CollectionsDelete {
                id: require_positional(tail, 1, "collection id")?,
            }),
            Some(other) => Err(format!(
                "unknown collections subcommand '{other}' (expected list, create or delete)"
            )),
            None => Err("collections needs a subcommand (list, create or delete)".to_string()),
        },
        "history" => match tail.first().map(String::as_str) {
            Some("list") => Ok(Command::HistoryList(parse_history(&tail[1..])?)),
            Some("clear") => Ok(Command::HistoryClear),
            Some(other) => Err(format!(
                "unknown history subcommand '{other}' (expected list or clear)"
            )),
            None => Err("history needs a subcommand (list or clear)".to_string()),
        },
        "environments" => match tail.first().map(String::as_str) {
            Some("list") => Ok(Command::EnvironmentsList),
            Some("create") => Ok(Command::EnvironmentsCreate {
                name: require_positional(tail, 1, "environment name")?,
            }),
            Some("use") => Ok(Command::EnvironmentsUse {
                name: require_positional(tail, 1, "environment name or 'none'")?,
            }),
            Some("variables") => Ok(Command::EnvironmentsVariables {
                name: require_positional(tail, 1, "environment name")?,
            }),
            Some("set") => {
                let name = require_positional(tail, 1, "environment name")?;
                let pair = require_positional(tail, 2, "KEY=VALUE assignment")?;
                let assignment = Pair::parse(&pair, '=')?;
                let secret = tail.iter().any(|flag| flag == "--secret");
                let disabled = tail.iter().any(|flag| flag == "--disabled");
                for flag in tail.iter().skip(3) {
                    if flag.starts_with('-') && flag != "--secret" && flag != "--disabled" {
                        return Err(format!("unknown environments set option '{flag}'"));
                    }
                }
                Ok(Command::EnvironmentsSet {
                    name,
                    key: assignment.key,
                    value: assignment.value,
                    secret,
                    disabled,
                })
            }
            Some("unset") => Ok(Command::EnvironmentsUnset {
                name: require_positional(tail, 1, "environment name")?,
                key: require_positional(tail, 2, "variable name")?,
            }),
            Some(other) => Err(format!(
                "unknown environments subcommand '{other}' (expected list, create, use, variables, set or unset)"
            )),
            None => Err(
                "environments needs a subcommand (list, create, use or variables)".to_string(),
            ),
        },
        "config" => match tail.first().map(String::as_str) {
            Some("get") => Ok(Command::ConfigGet),
            Some("set") => {
                let key = require_positional(tail, 1, "setting name")?;
                let value = require_positional(tail, 2, "setting value")?;
                Ok(Command::ConfigSet {
                    key: ConfigKey::parse(&key)?,
                    value,
                })
            }
            Some(other) => Err(format!(
                "unknown config subcommand '{other}' (expected get or set)"
            )),
            None => Err("config needs a subcommand (get or set)".to_string()),
        },
        other => Err(format!(
            "unknown command '{other}' (run 'requestlab-cli --help')"
        )),
    }
}

fn require_positional(args: &[String], index: usize, what: &str) -> Result<String, String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| format!("missing {what}"))
}

fn parse_history(args: &[String]) -> Result<HistoryQuery, String> {
    let mut query = HistoryQuery::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--limit" => {
                let value = iter.next().ok_or("--limit needs a number")?;
                query.limit = value
                    .parse::<i64>()
                    .map_err(|_| format!("'{value}' is not a number"))?
                    .clamp(1, 1000);
            }
            "--method" => {
                let value = iter.next().ok_or("--method needs a value")?;
                query.method = Some(
                    HttpMethod::parse(value)
                        .ok_or_else(|| format!("'{value}' is not a supported HTTP method"))?,
                );
            }
            "--status" => {
                let value = iter.next().ok_or("--status needs a value")?;
                let normalized = value.to_ascii_lowercase();
                if !matches!(
                    normalized.as_str(),
                    "all" | "2xx" | "3xx" | "4xx" | "5xx" | "failed"
                ) {
                    return Err(format!(
                        "'{value}' is not a status filter (all, 2xx, 3xx, 4xx, 5xx, failed)"
                    ));
                }
                query.status = normalized;
            }
            "--search" => {
                query.search = iter.next().ok_or("--search needs a value")?.to_string();
            }
            other => return Err(format!("unknown history option '{other}'")),
        }
    }
    Ok(query)
}

fn parse_send(args: &[String]) -> Result<Command, String> {
    let mut options = SendOptions {
        body_kind: RequestBodyKind::None,
        print_body: true,
        ..SendOptions::default()
    };
    let mut positionals: Vec<String> = Vec::new();
    let mut body_kind_set = false;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut take = |flag: &str| -> Result<String, String> {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{flag} needs a value"))
        };
        match arg.as_str() {
            "-X" | "--method" => {
                let value = take("--method")?;
                options.method = HttpMethod::parse(&value)
                    .ok_or_else(|| format!("'{value}' is not a supported HTTP method"))?;
            }
            "-H" | "--header" => {
                options.headers.push(Pair::parse(&take("--header")?, ':')?);
            }
            "-p" | "--param" => {
                options.params.push(Pair::parse(&take("--param")?, '=')?);
            }
            "-F" | "--form" => {
                options.form.push(Pair::parse(&take("--form")?, '=')?);
                options.body_kind = RequestBodyKind::FormUrlEncoded;
                body_kind_set = true;
            }
            "-d" | "--data" | "--body" => {
                options.body = Some(take("--data")?);
                if !body_kind_set {
                    options.body_kind = RequestBodyKind::Json;
                }
            }
            "--body-type" => {
                let value = take("--body-type")?;
                options.body_kind = match value.as_str() {
                    "json" => RequestBodyKind::Json,
                    "raw" => RequestBodyKind::Raw,
                    "form" => RequestBodyKind::FormUrlEncoded,
                    "none" => RequestBodyKind::None,
                    other => {
                        return Err(format!(
                            "'{other}' is not a body type (json, raw, form, none)"
                        ))
                    }
                };
                body_kind_set = true;
            }
            "--bearer" => {
                options.auth = Some(CliAuth::Bearer {
                    token: take("--bearer")?,
                });
            }
            "--basic" => {
                let raw = take("--basic")?;
                let (username, password) =
                    raw.split_once(':').ok_or("--basic expects USER:PASSWORD")?;
                options.auth = Some(CliAuth::Basic {
                    username: username.to_string(),
                    password: password.to_string(),
                });
            }
            "--api-key" => {
                let raw = take("--api-key")?;
                let (raw, location) = match raw.strip_suffix("@query") {
                    Some(trimmed) => (trimmed.to_string(), ApiKeyLocation::Query),
                    None => (raw, ApiKeyLocation::Header),
                };
                let pair = Pair::parse(&raw, '=')?;
                options.auth = Some(CliAuth::ApiKey {
                    key: pair.key,
                    value: pair.value,
                    location,
                });
            }
            "-e" | "--env" => {
                options.environment = Some(take("--env")?);
            }
            "-t" | "--timeout" => {
                let value = take("--timeout")?;
                options.timeout_seconds = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| format!("'{value}' is not a number of seconds"))?,
                );
            }
            "--no-body" => options.print_body = false,
            "-f" | "--fail" => options.fail_on_error_status = true,
            "--curl" => options.curl = Some(take("--curl")?),
            other if other.starts_with('-') && other.len() > 1 => {
                return Err(format!("unknown send option '{other}'"));
            }
            other => positionals.push(other.to_string()),
        }
    }

    // With --curl the URL comes from the imported command, not from the
    // positional argument.
    options.url = match positionals.into_iter().next() {
        Some(url) => url,
        None if options.curl.is_some() => String::new(),
        None => {
            return Err(
                "send needs a URL, for example: requestlab-cli send https://example.com"
                    .to_string(),
            )
        }
    };
    if options.curl.is_some()
        && (!options.headers.is_empty() || options.body.is_some() || !options.params.is_empty())
    {
        return Err("--curl already carries the request; drop the other send options".to_string());
    }

    if options.body.is_some() && options.body_kind == RequestBodyKind::FormUrlEncoded {
        return Err("use --form for URL-encoded bodies instead of --data".to_string());
    }
    if options.body_kind == RequestBodyKind::FormUrlEncoded
        && options.body.is_none()
        && options.form.is_empty()
    {
        return Err("--body-type form needs at least one --form NAME=VALUE".to_string());
    }

    Ok(Command::Send(options))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_accepts_a_curl_command() {
        let Command::Send(options) = parse([
            "send",
            "--curl",
            "curl -X POST https://api.example.com -H 'X-Trace: 1'",
        ])
        .unwrap()
        .command
        else {
            panic!("expected send");
        };
        let command = options.curl.expect("--curl is captured");
        assert!(command.starts_with("curl -X POST"), "{command}");
    }

    #[test]
    fn curl_preview_takes_the_whole_command() {
        let Command::Curl { command } = parse(["curl", "curl", "-I", "https://example.com"])
            .unwrap()
            .command
        else {
            panic!("expected curl");
        };
        assert_eq!(command, "curl -I https://example.com");
    }

    #[test]
    fn curl_without_a_command_is_a_usage_error() {
        assert!(parse(["curl"]).is_err());
    }

    #[test]
    fn parses_a_full_send_invocation() {
        let invocation = parse([
            "--json",
            "send",
            "https://api.example.com/users",
            "-X",
            "post",
            "-H",
            "Content-Type: application/json",
            "-H",
            "Accept: application/json",
            "-d",
            "{\"name\":\"ada\"}",
            "-e",
            "Development",
            "--timeout",
            "12",
            "-f",
        ])
        .expect("invocation parses");

        assert!(invocation.globals.json);
        let Command::Send(options) = invocation.command else {
            panic!("expected send");
        };
        assert_eq!(options.method, HttpMethod::Post);
        assert_eq!(options.headers.len(), 2);
        assert_eq!(options.headers[0].key, "Content-Type");
        assert_eq!(options.body_kind, RequestBodyKind::Json);
        assert_eq!(options.environment.as_deref(), Some("Development"));
        assert_eq!(options.timeout_seconds, Some(12));
        assert!(options.fail_on_error_status);
    }

    #[test]
    fn data_defaults_to_json_but_raw_can_be_requested() {
        let Command::Send(options) = parse([
            "send",
            "https://example.com",
            "-d",
            "<x/>",
            "--body-type",
            "raw",
        ])
        .unwrap()
        .command
        else {
            panic!("expected send");
        };
        assert_eq!(options.body_kind, RequestBodyKind::Raw);
        assert_eq!(options.body.as_deref(), Some("<x/>"));
    }

    #[test]
    fn form_fields_imply_form_body_kind() {
        let Command::Send(options) =
            parse(["send", "https://example.com", "-F", "a=1", "-F", "b=2"])
                .unwrap()
                .command
        else {
            panic!("expected send");
        };
        assert_eq!(options.body_kind, RequestBodyKind::FormUrlEncoded);
        assert_eq!(options.form.len(), 2);
    }

    #[test]
    fn api_key_location_suffix_is_parsed() {
        let Command::Send(options) = parse([
            "send",
            "https://example.com",
            "--api-key",
            "token=abc@query",
        ])
        .unwrap()
        .command
        else {
            panic!("expected send");
        };
        assert_eq!(
            options.auth,
            Some(CliAuth::ApiKey {
                key: "token".into(),
                value: "abc".into(),
                location: ApiKeyLocation::Query,
            })
        );
    }

    #[test]
    fn basic_auth_requires_a_colon() {
        let error = parse(["send", "https://example.com", "--basic", "user"]).unwrap_err();
        assert!(error.contains("USER:PASSWORD"));
    }

    #[test]
    fn missing_url_is_a_usage_error() {
        let error = parse(["send", "-X", "GET"]).unwrap_err();
        assert!(error.contains("needs a URL"));
    }

    #[test]
    fn unknown_option_is_rejected() {
        let error = parse(["send", "https://example.com", "--nope"]).unwrap_err();
        assert!(error.contains("unknown send option"));
    }

    #[test]
    fn invalid_method_is_rejected() {
        let error = parse(["send", "https://example.com", "-X", "TRACE"]).unwrap_err();
        assert!(error.contains("not a supported HTTP method"));
    }

    #[test]
    fn header_without_separator_is_rejected() {
        let error = parse(["send", "https://example.com", "-H", "Broken"]).unwrap_err();
        assert!(error.contains("expected NAME"));
    }

    #[test]
    fn history_filters_are_validated() {
        let invocation = parse([
            "history", "list", "--limit", "5", "--method", "get", "--status", "4xx", "--search",
            "users",
        ])
        .unwrap();
        let Command::HistoryList(query) = invocation.command else {
            panic!("expected history list");
        };
        assert_eq!(query.limit, 5);
        assert_eq!(query.method, Some(HttpMethod::Get));
        assert_eq!(query.status, "4xx");
        assert_eq!(query.search, "users");

        assert!(parse(["history", "list", "--status", "6xx"]).is_err());
        assert!(parse(["history", "list", "--limit", "many"]).is_err());
    }

    #[test]
    fn environment_set_parses_assignment_and_flags() {
        let invocation = parse(["environments", "set", "Local", "token=abc", "--secret"]).unwrap();
        let Command::EnvironmentsSet {
            name,
            key,
            value,
            secret,
            disabled,
        } = invocation.command
        else {
            panic!("expected environments set");
        };
        assert_eq!(name, "Local");
        assert_eq!(key, "token");
        assert_eq!(value, "abc");
        assert!(secret);
        assert!(!disabled);

        assert!(parse(["environments", "set", "Local"]).is_err());
        assert!(parse(["environments", "set", "Local", "novalue"]).is_err());
        assert!(parse(["environments", "unset", "Local"]).is_err());
    }

    #[test]
    fn config_set_validates_the_key() {
        assert!(parse(["config", "set", "timeout", "10"]).is_ok());
        assert!(parse(["config", "set", "lol", "10"]).is_err());
    }

    #[test]
    fn help_and_version_flags_win() {
        assert_eq!(parse(["--help"]).unwrap().command, Command::Help);
        assert_eq!(parse(["-V"]).unwrap().command, Command::Version);
    }

    #[test]
    fn data_dir_is_captured_and_requires_a_value() {
        let invocation = parse(["--data-dir", "/tmp/rl", "collections", "list"]).unwrap();
        assert_eq!(invocation.globals.data_dir.as_deref(), Some("/tmp/rl"));
        assert!(parse(["--data-dir"]).is_err());
    }

    #[test]
    fn unknown_command_is_rejected() {
        assert!(parse(["frobnicate"]).is_err());
        assert!(parse(["collections", "rename"]).is_err());
    }
}
