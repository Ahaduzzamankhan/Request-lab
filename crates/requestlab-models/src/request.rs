//! Request models: methods, parameters, headers, bodies, authentication.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::{
    is_valid_header_name, is_valid_header_value, validate_url, RequestField, ValidationIssue,
    MAX_HISTORY_BODY_BYTES,
};

/// The HTTP methods RequestLab can issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    pub const ALL: [HttpMethod; 7] = [
        HttpMethod::Get,
        HttpMethod::Post,
        HttpMethod::Put,
        HttpMethod::Patch,
        HttpMethod::Delete,
        HttpMethod::Head,
        HttpMethod::Options,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
        }
    }

    /// Parse a method name case-insensitively.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|method| method.as_str().eq_ignore_ascii_case(value.trim()))
    }

    /// `HEAD` responses never carry a body, so sending one is a user error.
    pub fn forbids_body(self) -> bool {
        matches!(self, HttpMethod::Head)
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for HttpMethod {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| format!("'{s}' is not a supported HTTP method"))
    }
}

/// A single key/value row used by both query parameters and form bodies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryParameter {
    pub id: Uuid,
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

impl QueryParameter {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            value: value.into(),
            enabled: true,
        }
    }

    /// Copy of this row with a fresh identifier, for the Duplicate action.
    pub fn duplicate(&self) -> Self {
        Self {
            id: Uuid::new_v4(),
            ..self.clone()
        }
    }
}

/// A single request header row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderEntry {
    pub id: Uuid,
    pub key: String,
    pub value: String,
    pub enabled: bool,
}

impl HeaderEntry {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            key: key.into(),
            value: value.into(),
            enabled: true,
        }
    }

    pub fn duplicate(&self) -> Self {
        Self {
            id: Uuid::new_v4(),
            ..self.clone()
        }
    }
}

impl Default for HeaderEntry {
    fn default() -> Self {
        Self {
            id: Uuid::new_v4(),
            key: String::new(),
            value: String::new(),
            enabled: true,
        }
    }
}

/// Which body representation the request uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestBodyKind {
    None,
    Json,
    Raw,
    FormUrlEncoded,
}

impl RequestBodyKind {
    pub const ALL: [RequestBodyKind; 4] = [
        RequestBodyKind::None,
        RequestBodyKind::Json,
        RequestBodyKind::Raw,
        RequestBodyKind::FormUrlEncoded,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RequestBodyKind::None => "None",
            RequestBodyKind::Json => "JSON",
            RequestBodyKind::Raw => "Raw",
            RequestBodyKind::FormUrlEncoded => "Form",
        }
    }
}

/// The request body. A typed enum instead of ambiguous flags.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RequestBody {
    #[default]
    None,
    Json {
        content: String,
    },
    Raw {
        content: String,
        content_type: String,
    },
    FormUrlEncoded {
        fields: Vec<QueryParameter>,
    },
}

impl RequestBody {
    pub fn json(content: impl Into<String>) -> Self {
        RequestBody::Json {
            content: content.into(),
        }
    }

    pub fn raw(content: impl Into<String>) -> Self {
        RequestBody::Raw {
            content: content.into(),
            content_type: "text/plain".to_string(),
        }
    }

    pub fn form(fields: Vec<QueryParameter>) -> Self {
        RequestBody::FormUrlEncoded { fields }
    }

    pub fn kind(&self) -> RequestBodyKind {
        match self {
            RequestBody::None => RequestBodyKind::None,
            RequestBody::Json { .. } => RequestBodyKind::Json,
            RequestBody::Raw { .. } => RequestBodyKind::Raw,
            RequestBody::FormUrlEncoded { .. } => RequestBodyKind::FormUrlEncoded,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            RequestBody::None => true,
            RequestBody::Json { content } => content.trim().is_empty(),
            RequestBody::Raw { content, .. } => content.is_empty(),
            RequestBody::FormUrlEncoded { fields } => fields
                .iter()
                .all(|field| !field.enabled || field.key.is_empty()),
        }
    }

    /// Content type implied by the body representation, if any.
    pub fn content_type(&self) -> Option<&str> {
        match self {
            RequestBody::None | RequestBody::FormUrlEncoded { .. } => None,
            RequestBody::Json { .. } => Some("application/json"),
            RequestBody::Raw { content_type, .. } => {
                if content_type.trim().is_empty() {
                    Some("text/plain")
                } else {
                    Some(content_type.as_str())
                }
            }
        }
    }

    /// The editable text of the body, for the code editor.
    pub fn text(&self) -> Option<&str> {
        match self {
            RequestBody::Json { content } | RequestBody::Raw { content, .. } => Some(content),
            RequestBody::None | RequestBody::FormUrlEncoded { .. } => None,
        }
    }

    /// Mutable access to the editable text of the body.
    pub fn text_mut(&mut self) -> Option<&mut String> {
        match self {
            RequestBody::Json { content } | RequestBody::Raw { content, .. } => Some(content),
            RequestBody::None | RequestBody::FormUrlEncoded { .. } => None,
        }
    }

    /// Validate JSON bodies before sending. Returns a human readable error.
    pub fn json_validation_error(&self) -> Option<String> {
        let RequestBody::Json { content } = self else {
            return None;
        };
        if content.trim().is_empty() {
            return None;
        }
        match serde_json::from_str::<serde_json::Value>(content) {
            Ok(_) => None,
            Err(err) => Some(format!(
                "Invalid JSON at line {}, column {}: {}",
                err.line(),
                err.column(),
                err
            )),
        }
    }

    /// A copy of this body truncated for history storage.
    pub fn for_history(&self) -> RequestBody {
        match self {
            RequestBody::None => RequestBody::None,
            RequestBody::Json { content } => RequestBody::Json {
                content: crate::common::truncate_utf8(content, MAX_HISTORY_BODY_BYTES).to_string(),
            },
            RequestBody::Raw {
                content,
                content_type,
            } => RequestBody::Raw {
                content: crate::common::truncate_utf8(content, MAX_HISTORY_BODY_BYTES).to_string(),
                content_type: content_type.clone(),
            },
            RequestBody::FormUrlEncoded { fields } => RequestBody::FormUrlEncoded {
                fields: fields
                    .iter()
                    .map(|field| QueryParameter {
                        value: crate::common::truncate_utf8(&field.value, 1024).to_string(),
                        ..field.clone()
                    })
                    .collect(),
            },
        }
    }
}

/// Where an API key is injected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyLocation {
    #[default]
    Header,
    Query,
}

/// Which OAuth 2.0 grant a request uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuth2Grant {
    #[default]
    ClientCredentials,
    Password,
}

impl OAuth2Grant {
    pub const ALL: [OAuth2Grant; 2] = [OAuth2Grant::ClientCredentials, OAuth2Grant::Password];

    pub fn label(self) -> &'static str {
        match self {
            OAuth2Grant::ClientCredentials => "Client credentials",
            OAuth2Grant::Password => "Password",
        }
    }

    /// The `grant_type` value sent to the token endpoint.
    pub fn grant_type(self) -> &'static str {
        match self {
            OAuth2Grant::ClientCredentials => "client_credentials",
            OAuth2Grant::Password => "password",
        }
    }
}

/// Request authentication. Credentials live here, never in the header list.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Authentication {
    #[default]
    None,
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
    /// HTTP Digest (RFC 7616). The challenge is read from the 401 response at
    /// send time, so no nonce is stored here.
    Digest {
        username: String,
        password: String,
    },
    /// AWS Signature Version 4. Session tokens are for temporary credentials.
    AwsSigV4 {
        access_key: String,
        secret_key: String,
        region: String,
        service: String,
        #[serde(default)]
        session_token: String,
    },
    /// OAuth 2.0. The token is fetched and cached by the engine, never saved.
    OAuth2 {
        grant: OAuth2Grant,
        token_url: String,
        client_id: String,
        client_secret: String,
        #[serde(default)]
        scope: String,
        #[serde(default)]
        username: String,
        #[serde(default)]
        password: String,
    },
}

impl Authentication {
    pub fn label(&self) -> &'static str {
        match self {
            Authentication::None => "None",
            Authentication::Basic { .. } => "Basic",
            Authentication::Bearer { .. } => "Bearer",
            Authentication::ApiKey { .. } => "API Key",
            Authentication::Digest { .. } => "Digest",
            Authentication::AwsSigV4 { .. } => "AWS Signature v4",
            Authentication::OAuth2 { .. } => "OAuth 2.0",
        }
    }

    /// Whether this scheme needs work at send time instead of a fixed header.
    pub fn is_negotiated(&self) -> bool {
        matches!(
            self,
            Authentication::Digest { .. }
                | Authentication::AwsSigV4 { .. }
                | Authentication::OAuth2 { .. }
        )
    }

    /// A short description for the request summary line, with no secret material.
    pub fn describe(&self) -> String {
        match self {
            Authentication::None => "No authentication".to_string(),
            Authentication::Basic { username, .. } => {
                if username.is_empty() {
                    "Basic auth (no username)".to_string()
                } else {
                    format!("Basic auth as {username}")
                }
            }
            Authentication::Bearer { .. } => "Bearer token".to_string(),
            Authentication::ApiKey { key, location, .. } => {
                let where_ = match location {
                    ApiKeyLocation::Header => "header",
                    ApiKeyLocation::Query => "query",
                };
                format!("API key '{key}' in {where_}")
            }
            Authentication::Digest { username, .. } => {
                if username.is_empty() {
                    "Digest auth (no username)".to_string()
                } else {
                    format!("Digest auth as {username}")
                }
            }
            Authentication::AwsSigV4 {
                region, service, ..
            } => format!("AWS Signature v4 ({service} in {region})"),
            Authentication::OAuth2 { grant, .. } => {
                format!("OAuth 2.0 ({})", grant.label().to_lowercase())
            }
        }
    }

    fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        match self {
            Authentication::None => {}
            Authentication::Basic { username, .. } => {
                if username.trim().is_empty() {
                    issues.push(ValidationIssue::new(
                        RequestField::Authentication,
                        "Basic auth requires a username",
                    ));
                }
            }
            Authentication::Bearer { token } => {
                if token.trim().is_empty() {
                    issues.push(ValidationIssue::new(
                        RequestField::Authentication,
                        "Bearer auth requires a token",
                    ));
                }
            }
            Authentication::ApiKey { key, value, .. } => {
                if key.trim().is_empty() {
                    issues.push(ValidationIssue::new(
                        RequestField::Authentication,
                        "API key auth requires a key name",
                    ));
                }
                if value.trim().is_empty() {
                    issues.push(ValidationIssue::new(
                        RequestField::Authentication,
                        "API key auth requires a value",
                    ));
                }
            }
            Authentication::Digest { username, .. } => {
                if username.trim().is_empty() {
                    issues.push(ValidationIssue::new(
                        RequestField::Authentication,
                        "Digest auth requires a username",
                    ));
                }
            }
            Authentication::AwsSigV4 {
                access_key,
                secret_key,
                region,
                service,
                ..
            } => {
                let required = [
                    (access_key.as_str(), "an access key id"),
                    (secret_key.as_str(), "a secret access key"),
                    (region.as_str(), "a region"),
                    (service.as_str(), "a service name"),
                ];
                for (value, what) in required {
                    if value.trim().is_empty() {
                        issues.push(ValidationIssue::new(
                            RequestField::Authentication,
                            format!("AWS Signature v4 requires {what}"),
                        ));
                    }
                }
            }
            Authentication::OAuth2 {
                grant,
                token_url,
                client_id,
                username,
                ..
            } => {
                if token_url.trim().is_empty() {
                    issues.push(ValidationIssue::new(
                        RequestField::Authentication,
                        "OAuth 2.0 requires a token URL",
                    ));
                }
                if client_id.trim().is_empty() {
                    issues.push(ValidationIssue::new(
                        RequestField::Authentication,
                        "OAuth 2.0 requires a client id",
                    ));
                }
                if *grant == OAuth2Grant::Password && username.trim().is_empty() {
                    issues.push(ValidationIssue::new(
                        RequestField::Authentication,
                        "The password grant requires a username",
                    ));
                }
            }
        }
        issues
    }
}

/// A complete request definition, as edited and saved by the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    pub id: Uuid,
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    #[serde(default)]
    pub query_params: Vec<QueryParameter>,
    #[serde(default)]
    pub headers: Vec<HeaderEntry>,
    #[serde(default)]
    pub body: RequestBody,
    #[serde(default)]
    pub authentication: Authentication,
}

impl Request {
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        let url = url.into();
        Self {
            id: Uuid::new_v4(),
            name: String::new(),
            method,
            url,
            query_params: Vec::new(),
            headers: Vec::new(),
            body: RequestBody::None,
            authentication: Authentication::None,
        }
    }

    /// A blank GET request named for a new tab.
    pub fn blank() -> Self {
        let mut request = Self::new(HttpMethod::Get, "");
        request.name = "Untitled request".to_string();
        request
    }

    /// The label shown in tabs; falls back to the URL, then a placeholder.
    pub fn display_name(&self) -> String {
        if !self.name.trim().is_empty() {
            return self.name.clone();
        }
        let url = self.url.trim();
        if url.is_empty() {
            return "Untitled request".to_string();
        }
        let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
        crate::common::truncate_utf8(without_scheme, 60).to_string()
    }

    /// Copy with a new identity and a `(copy)` suffix, for Duplicate.
    pub fn duplicate_request(&self) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: if self.name.trim().is_empty() {
                String::new()
            } else {
                format!("{} (copy)", self.name)
            },
            query_params: self
                .query_params
                .iter()
                .map(QueryParameter::duplicate)
                .collect(),
            headers: self.headers.iter().map(HeaderEntry::duplicate).collect(),
            body: match &self.body {
                RequestBody::FormUrlEncoded { fields } => RequestBody::FormUrlEncoded {
                    fields: fields.iter().map(QueryParameter::duplicate).collect(),
                },
                other => other.clone(),
            },
            ..self.clone()
        }
    }

    /// Validate everything that can be checked before environment resolution.
    pub fn validate(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        let url = self.url.trim();
        if url.is_empty() {
            issues.push(ValidationIssue::new(
                RequestField::Url,
                "Enter a request URL",
            ));
        } else if !crate::common::contains_template_variable(url) {
            if let Err(message) = validate_url(url) {
                issues.push(ValidationIssue::new(RequestField::Url, message));
            }
        }

        for header in self.headers.iter().filter(|h| h.enabled) {
            let key = header.key.trim();
            if key.is_empty() {
                if !header.value.trim().is_empty() {
                    issues.push(ValidationIssue::new(
                        RequestField::Header,
                        "A header value was entered without a header name",
                    ));
                }
                continue;
            }
            if !is_valid_header_name(key) {
                issues.push(ValidationIssue::new(
                    RequestField::Header,
                    format!("'{key}' is not a valid header name"),
                ));
            }
            if !is_valid_header_value(&header.value) {
                issues.push(ValidationIssue::new(
                    RequestField::Header,
                    format!("Header '{key}' contains a newline or control character"),
                ));
            }
        }

        for param in self.query_params.iter().filter(|p| p.enabled) {
            if param.key.trim().is_empty() && !param.value.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    RequestField::QueryParameter,
                    "A query value was entered without a parameter name",
                ));
            }
            if !is_valid_header_value(&param.value) {
                issues.push(ValidationIssue::new(
                    RequestField::QueryParameter,
                    "A query parameter contains a control character",
                ));
            }
        }

        if let Some(message) = self.body.json_validation_error() {
            issues.push(ValidationIssue::new(RequestField::Body, message));
        }
        if self.method.forbids_body() && !self.body.is_empty() {
            issues.push(ValidationIssue::new(
                RequestField::Body,
                "HEAD requests cannot include a body",
            ));
        }
        let empty_fields: &[QueryParameter] = &[];
        for field in match &self.body {
            RequestBody::FormUrlEncoded { fields } => fields.as_slice(),
            _ => empty_fields,
        } {
            if field.enabled && field.key.trim().is_empty() && !field.value.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    RequestField::Body,
                    "A form field has a value but no name",
                ));
            }
        }

        issues.extend(self.authentication.validate());
        issues
    }

    /// Whether the request is currently sendable.
    pub fn is_valid(&self) -> bool {
        self.validate().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_round_trip() {
        for method in HttpMethod::ALL {
            assert_eq!(HttpMethod::parse(method.as_str()), Some(method));
        }
        assert_eq!(HttpMethod::parse("post"), Some(HttpMethod::Post));
        assert_eq!(HttpMethod::parse("trace"), None);
        assert_eq!(HttpMethod::default().as_str(), "GET");
        assert!(HttpMethod::Head.forbids_body());
        assert!(!HttpMethod::Post.forbids_body());
    }

    #[test]
    fn blank_request_reports_missing_url() {
        let issues = Request::blank().validate();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].field, RequestField::Url);
    }

    #[test]
    fn valid_request_has_no_issues() {
        let mut request = Request::new(HttpMethod::Post, "https://api.example.com/users");
        request
            .headers
            .push(HeaderEntry::new("Content-Type", "application/json"));
        request.body = RequestBody::json(r#"{"name":"Ada"}"#);
        assert!(request.validate().is_empty(), "{:?}", request.validate());
    }

    #[test]
    fn invalid_json_is_reported_with_position() {
        let mut request = Request::new(HttpMethod::Post, "https://example.com");
        request.body = RequestBody::json("{ \"a\": }");
        let issues = request.validate();
        assert!(issues.iter().any(|issue| issue.field == RequestField::Body
            && issue.message.contains("Invalid JSON")
            && issue.message.contains("line")));
    }

    #[test]
    fn header_name_and_newline_validation() {
        let mut request = Request::new(HttpMethod::Get, "https://example.com");
        request
            .headers
            .push(HeaderEntry::new("Bad Header", "value"));
        request.headers.push(HeaderEntry::new("X-Ok", "a\nb"));
        let issues = request.validate();
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|i| i.field == RequestField::Header));
    }

    #[test]
    fn disabled_headers_are_ignored_by_validation() {
        let mut request = Request::new(HttpMethod::Get, "https://example.com");
        let mut header = HeaderEntry::new("Bad Header", "x");
        header.enabled = false;
        request.headers.push(header);
        assert!(request.validate().is_empty());
    }

    #[test]
    fn auth_validation() {
        let mut request = Request::new(HttpMethod::Get, "https://example.com");
        request.authentication = Authentication::Bearer {
            token: " ".to_string(),
        };
        assert_eq!(request.validate().len(), 1);
        request.authentication = Authentication::Basic {
            username: "ada".into(),
            password: String::new(),
        };
        assert!(request.validate().is_empty());
    }

    #[test]
    fn template_urls_skip_url_validation() {
        let request = Request::new(HttpMethod::Get, "{{BASE_URL}}/users");
        assert!(request.validate().is_empty());
    }

    #[test]
    fn duplicate_creates_new_identifiers() {
        let mut request = Request::new(HttpMethod::Get, "https://example.com");
        request.name = "List users".into();
        request
            .headers
            .push(HeaderEntry::new("Accept", "application/json"));
        let copy = request.duplicate_request();
        assert_ne!(copy.id, request.id);
        assert_ne!(copy.headers[0].id, request.headers[0].id);
        assert_eq!(copy.name, "List users (copy)");
        assert_eq!(copy.url, request.url);
    }

    #[test]
    fn body_kinds_and_content_types() {
        assert_eq!(RequestBody::None.kind(), RequestBodyKind::None);
        assert_eq!(
            RequestBody::json("{}").content_type(),
            Some("application/json")
        );
        assert_eq!(
            RequestBody::Raw {
                content: "<x/>".into(),
                content_type: String::new()
            }
            .content_type(),
            Some("text/plain")
        );
        assert!(RequestBody::None.is_empty());
        assert!(!RequestBody::raw("hello").is_empty());
    }

    #[test]
    fn history_body_is_truncated() {
        let long = "a".repeat(MAX_HISTORY_BODY_BYTES + 100);
        let trimmed = RequestBody::json(long).for_history();
        assert_eq!(trimmed.text().map(str::len), Some(MAX_HISTORY_BODY_BYTES));
    }

    #[test]
    fn display_name_falls_back_to_url() {
        let mut request = Request::new(HttpMethod::Get, "https://api.example.com/v1/users");
        assert_eq!(request.display_name(), "api.example.com/v1/users");
        request.name = "Users".into();
        assert_eq!(request.display_name(), "Users");
        request.url = String::new();
        request.name = String::new();
        assert_eq!(request.display_name(), "Untitled request");
    }
}
