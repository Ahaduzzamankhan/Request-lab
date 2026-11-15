//! Request history models.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::{is_sensitive_header, truncate_utf8};
use crate::request::{HeaderEntry, HttpMethod, Request};

/// A recorded request attempt: success or failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: Uuid,
    pub method: HttpMethod,
    pub url: String,
    /// The request as sent, with credentials redacted and the body truncated.
    pub request: Request,
    pub status_code: Option<u16>,
    pub duration_ms: Option<u64>,
    pub error_message: Option<String>,
    pub created_at: String,
}

impl HistoryEntry {
    /// Build a history row from a completed attempt.
    ///
    /// Sensitive header values are replaced with a redaction marker and the body
    /// is truncated, so seeding history cannot leak credentials or blow up the
    /// database.
    pub fn from_attempt(
        request: &Request,
        status_code: Option<u16>,
        duration_ms: Option<u64>,
        error_message: Option<String>,
        created_at: String,
    ) -> Self {
        let mut redacted = request.clone();
        redacted.headers = request
            .headers
            .iter()
            .map(|header| HeaderEntry {
                id: header.id,
                key: header.key.clone(),
                value: if is_sensitive_header(&header.key) && !header.value.is_empty() {
                    crate::common::REDACTED.to_string()
                } else {
                    header.value.clone()
                },
                enabled: header.enabled,
            })
            .collect();
        redacted.body = request.body.for_history();

        Self {
            id: Uuid::new_v4(),
            method: request.method,
            url: truncate_utf8(&request.url, 2048).to_string(),
            request: redacted,
            status_code,
            duration_ms,
            error_message: error_message.map(|message| truncate_utf8(&message, 1024).to_string()),
            created_at,
        }
    }

    /// The request to reload into the editor when replaying this entry.
    pub fn replay_request(&self) -> Request {
        let mut request = self.request.clone();
        request.id = Uuid::new_v4();
        request
    }

    pub fn succeeded(&self) -> bool {
        self.status_code.is_some_and(|code| (200..400).contains(&code))
    }

    /// `GET /users` style label for the history list.
    pub fn label(&self) -> String {
        let path = self
            .url
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(&self.url);
        crate::common::truncate_utf8(path, 72).to_string()
    }
}

/// Status grouping used by the history filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryStatusFilter {
    #[default]
    All,
    Success,
    Redirect,
    ClientError,
    ServerError,
    Failed,
}

impl HistoryStatusFilter {
    pub const ALL: [HistoryStatusFilter; 6] = [
        HistoryStatusFilter::All,
        HistoryStatusFilter::Success,
        HistoryStatusFilter::Redirect,
        HistoryStatusFilter::ClientError,
        HistoryStatusFilter::ServerError,
        HistoryStatusFilter::Failed,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HistoryStatusFilter::All => "All statuses",
            HistoryStatusFilter::Success => "2xx Success",
            HistoryStatusFilter::Redirect => "3xx Redirect",
            HistoryStatusFilter::ClientError => "4xx Client error",
            HistoryStatusFilter::ServerError => "5xx Server error",
            HistoryStatusFilter::Failed => "No response",
        }
    }

    /// Inclusive status range for SQL filtering, or `None` for "no response".
    pub fn status_range(self) -> Option<(u16, u16)> {
        match self {
            HistoryStatusFilter::All => None,
            HistoryStatusFilter::Success => Some((200, 299)),
            HistoryStatusFilter::Redirect => Some((300, 399)),
            HistoryStatusFilter::ClientError => Some((400, 499)),
            HistoryStatusFilter::ServerError => Some((500, 599)),
            HistoryStatusFilter::Failed => None,
        }
    }

    pub fn matches(self, status: Option<u16>) -> bool {
        match self {
            HistoryStatusFilter::All => true,
            HistoryStatusFilter::Failed => status.is_none(),
            _ => {
                let Some((low, high)) = self.status_range() else {
                    return true;
                };
                status.is_some_and(|code| (low..=high).contains(&code))
            }
        }
    }
}

/// A history query, bounded so the UI never loads the whole table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryQuery {
    pub search: String,
    pub method: Option<HttpMethod>,
    pub status: HistoryStatusFilter,
    pub limit: i64,
    pub offset: i64,
}

impl Default for HistoryQuery {
    fn default() -> Self {
        Self {
            search: String::new(),
            method: None,
            status: HistoryStatusFilter::All,
            limit: 200,
            offset: 0,
        }
    }
}

impl HistoryQuery {
    pub fn with_search(mut self, search: impl Into<String>) -> Self {
        self.search = search.into();
        self
    }

    pub fn with_method(mut self, method: Option<HttpMethod>) -> Self {
        self.method = method;
        self
    }

    pub fn with_status(mut self, status: HistoryStatusFilter) -> Self {
        self.status = status;
        self
    }

    /// Whether an entry satisfies this query, used to verify SQL results in tests.
    pub fn matches(&self, entry: &HistoryEntry) -> bool {
        let needle = self.search.trim().to_ascii_lowercase();
        let text_ok = needle.is_empty()
            || entry.url.to_ascii_lowercase().contains(&needle)
            || entry.method.as_str().to_ascii_lowercase().contains(&needle)
            || entry
                .error_message
                .as_ref()
                .is_some_and(|message| message.to_ascii_lowercase().contains(&needle));
        let method_ok = self.method.is_none_or(|method| method == entry.method);
        text_ok && method_ok && self.status.matches(entry.status_code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::{Authentication, HeaderEntry};

    fn request() -> Request {
        let mut request = Request::new(HttpMethod::Post, "https://api.example.com/login");
        request
            .headers
            .push(HeaderEntry::new("Authorization", "Bearer super-secret"));
        request.headers.push(HeaderEntry::new("Accept", "application/json"));
        request.authentication = Authentication::Bearer {
            token: "super-secret".into(),
        };
        request
    }

    #[test]
    fn records_attempt_with_redacted_credentials() {
        let entry = HistoryEntry::from_attempt(
            &request(),
            Some(201),
            Some(120),
            None,
            "2024-05-01T10:00:00.000Z".into(),
        );
        assert_eq!(entry.method, HttpMethod::Post);
        assert_eq!(entry.status_code, Some(201));
        assert_eq!(
            entry.request.headers[0].value,
            crate::common::REDACTED
        );
        assert_eq!(entry.request.headers[1].value, "application/json");
        assert!(entry.succeeded());
        assert!(entry.label().starts_with("api.example.com"));
    }

    #[test]
    fn records_network_failures() {
        let entry = HistoryEntry::from_attempt(
            &request(),
            None,
            None,
            Some("connection refused".into()),
            "t".into(),
        );
        assert!(!entry.succeeded());
        assert_eq!(entry.error_message.as_deref(), Some("connection refused"));
    }

    #[test]
    fn replay_gets_fresh_identity() {
        let entry =
            HistoryEntry::from_attempt(&request(), Some(200), Some(5), None, "t".into());
        let replay = entry.replay_request();
        assert_ne!(replay.id, entry.request.id);
        assert_eq!(replay.url, entry.url);
    }

    #[test]
    fn status_filter_ranges() {
        assert!(HistoryStatusFilter::Success.matches(Some(204)));
        assert!(!HistoryStatusFilter::Success.matches(Some(404)));
        assert!(HistoryStatusFilter::Failed.matches(None));
        assert!(!HistoryStatusFilter::Failed.matches(Some(200)));
        assert!(HistoryStatusFilter::All.matches(None));
        assert_eq!(HistoryStatusFilter::ClientError.status_range(), Some((400, 499)));
    }

    #[test]
    fn query_filters_by_search_method_and_status() {
        let entry = HistoryEntry::from_attempt(
            &request(),
            Some(404),
            Some(9),
            None,
            "t".into(),
        );
        assert!(HistoryQuery::default().matches(&entry));
        assert!(HistoryQuery::default().with_search("login").matches(&entry));
        assert!(!HistoryQuery::default().with_search("nope").matches(&entry));
        assert!(HistoryQuery::default()
            .with_method(Some(HttpMethod::Post))
            .matches(&entry));
        assert!(!HistoryQuery::default()
            .with_method(Some(HttpMethod::Get))
            .matches(&entry));
        assert!(HistoryQuery::default()
            .with_status(HistoryStatusFilter::ClientError)
            .matches(&entry));
        assert!(!HistoryQuery::default()
            .with_status(HistoryStatusFilter::Success)
            .matches(&entry));
    }
}
