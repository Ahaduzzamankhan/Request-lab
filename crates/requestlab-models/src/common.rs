//! Small shared helpers: identifiers, formatting, redaction and URL handling.

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

/// Maximum supported nesting depth for collection folders.
pub const MAX_FOLDER_DEPTH: usize = 8;

/// Hard cap applied to response bodies kept in memory (10 MiB).
pub const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

/// Cap applied to request bodies stored in history rows.
pub const MAX_HISTORY_BODY_BYTES: usize = 16 * 1024;

/// Default number of history rows retained on disk.
pub const DEFAULT_HISTORY_LIMIT: i64 = 1000;

/// Placeholder written instead of sensitive values.
pub const REDACTED: &str = "[redacted]";

/// Current UTC timestamp in RFC 3339 with millisecond precision.
pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Truncate a string to at most `max_bytes`, never splitting a UTF-8 code point.
pub fn truncate_utf8(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Format a byte count for display, e.g. `1.4 KB`.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Format a duration in milliseconds for display.
pub fn human_duration(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else {
        format!("{:.2} s", ms as f64 / 1000.0)
    }
}

/// Whether a header value must be treated as a credential.
///
/// Used to keep secrets out of history rows and log output.
pub fn is_sensitive_header(name: &str) -> bool {
    let lowered = name.trim().to_ascii_lowercase();
    matches!(
        lowered.as_str(),
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "api-key"
            | "x-api-key"
            | "x-auth-token"
    ) || lowered.contains("token")
        || lowered.contains("secret")
        || lowered.contains("password")
}

/// Return [`REDACTED`] for sensitive headers, otherwise the original value.
pub fn redact_if_sensitive(name: &str, value: &str) -> String {
    if is_sensitive_header(name) && !value.is_empty() {
        REDACTED.to_string()
    } else {
        value.to_string()
    }
}

/// Validate an HTTP header name against the RFC 7230 `token` production.
pub fn is_valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

/// Header values must not contain control characters that would split the message.
pub fn is_valid_header_value(value: &str) -> bool {
    !value.contains('\r') && !value.contains('\n') && !value.contains('\0')
}

/// Prefix a scheme when the user typed a bare host such as `localhost:3000`.
///
/// Text containing `{{...}}` template variables is returned unchanged; the final
/// URL is validated after environment resolution.
pub fn normalize_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if contains_template_variable(trimmed) || trimmed.contains("://") {
        return trimmed.to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("//") {
        return format!("https://{rest}");
    }
    format!("http://{trimmed}")
}

/// Whether the text contains an `{{...}}` template expression.
pub fn contains_template_variable(text: &str) -> bool {
    text.contains("{{")
}

/// Parse and validate a request URL.
///
/// Only `http` and `https` are accepted. A missing scheme is filled in with
/// `http://` so local development servers can be typed without a scheme.
pub fn validate_url(raw: &str) -> Result<url::Url, String> {
    let candidate = normalize_url(raw);
    if candidate.is_empty() {
        return Err("Enter a request URL".to_string());
    }
    if candidate.contains(char::is_whitespace) {
        return Err("URL must not contain spaces".to_string());
    }
    let parsed = url::Url::parse(&candidate).map_err(|err| format!("Invalid URL: {err}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!(
                "Unsupported URL scheme '{other}'; use http or https"
            ))
        }
    }
    if parsed.host_str().is_none_or(str::is_empty) {
        return Err("URL is missing a host".to_string());
    }
    Ok(parsed)
}

/// Redact credential-looking query parameter values in a URL.
///
/// Used before a URL is written to history or to a log line, so a token passed
/// as a query parameter never ends up in the database in clear text.
pub fn redact_url(raw: &str) -> String {
    let Ok(mut url) = url::Url::parse(raw) else {
        return raw.to_string();
    };
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            let lowered = key.to_ascii_lowercase();
            let secret = is_sensitive_header(&key)
                || lowered.contains("key")
                || lowered.contains("token")
                || lowered.contains("secret")
                || lowered.contains("password")
                || lowered.contains("auth")
                || lowered.contains("sig");
            let value = if secret && !value.is_empty() {
                REDACTED.to_string()
            } else {
                value.to_string()
            };
            (key.to_string(), value)
        })
        .collect();
    if !pairs.is_empty() {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    url.to_string()
}

/// Identifies which part of a request a validation issue refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestField {
    Url,
    Method,
    Header,
    QueryParameter,
    Body,
    Authentication,
}

impl RequestField {
    pub fn label(self) -> &'static str {
        match self {
            RequestField::Url => "URL",
            RequestField::Method => "Method",
            RequestField::Header => "Headers",
            RequestField::QueryParameter => "Params",
            RequestField::Body => "Body",
            RequestField::Authentication => "Auth",
        }
    }
}

/// A single user-facing validation problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub field: RequestField,
    pub message: String,
}

impl ValidationIssue {
    pub fn new(field: RequestField, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field.label(), self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_adds_scheme_for_bare_host() {
        assert_eq!(normalize_url("localhost:3000"), "http://localhost:3000");
        assert_eq!(
            normalize_url("https://api.example.com/v1"),
            "https://api.example.com/v1"
        );
        assert_eq!(normalize_url("  example.com/a  "), "http://example.com/a");
        assert_eq!(normalize_url("//example.com/x"), "https://example.com/x");
        assert_eq!(normalize_url(""), "");
    }

    #[test]
    fn normalize_leaves_template_urls_alone() {
        assert_eq!(normalize_url("{{BASE_URL}}/users"), "{{BASE_URL}}/users");
    }

    #[test]
    fn validate_accepts_http_and_https() {
        assert!(validate_url("https://example.com/a?b=c").is_ok());
        assert!(validate_url("http://127.0.0.1:8080/health").is_ok());
        assert!(validate_url("localhost:3000/users").is_ok());
    }

    #[test]
    fn validate_rejects_bad_input() {
        assert!(validate_url("").is_err());
        assert!(validate_url("ftp://example.com").is_err());
        assert!(validate_url("http://exa mple.com").is_err());
        assert!(validate_url("http://").is_err());
    }

    #[test]
    fn header_name_rules() {
        assert!(is_valid_header_name("Content-Type"));
        assert!(is_valid_header_name("x-request-id"));
        assert!(!is_valid_header_name("Bad Header"));
        assert!(!is_valid_header_name(""));
        assert!(!is_valid_header_value("a\nb"));
        assert!(is_valid_header_value("a/b c"));
    }

    #[test]
    fn sensitive_headers_are_redacted() {
        assert_eq!(redact_if_sensitive("Authorization", "Bearer abc"), REDACTED);
        assert_eq!(
            redact_if_sensitive("X-Api-Key", "secret"),
            REDACTED
        );
        assert_eq!(redact_if_sensitive("Accept", "application/json"), "application/json");
        assert!(is_sensitive_header("X-Auth-Token"));
    }

    #[test]
    fn url_redaction_hides_credentials() {
        let redacted = redact_url("https://api.example.com/x?api_key=secret&page=2");
        assert!(!redacted.contains("secret"));
        assert!(redacted.contains("page=2"));
        assert_eq!(redact_url("not a url"), "not a url");
    }

    #[test]
    fn formatting_helpers() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_duration(45), "45 ms");
        assert_eq!(human_duration(1500), "1.50 s");
    }

    #[test]
    fn truncation_keeps_char_boundaries() {
        let text = "héllo";
        let cut = truncate_utf8(text, 2);
        assert_eq!(cut, "h");
        assert_eq!(truncate_utf8(text, 100), text);
    }
}
