//! Build a transport-ready request from a user [`Request`].
//!
//! Preparation is the step between environment resolution and the network: it
//! validates the request, merges query parameters and authentication, and picks
//! the body encoding. It never mutates the caller's request.

use std::time::Duration;

use requestlab_models::{
    is_sensitive_header, normalize_url, validate_url, Authentication, HttpMethod, Request,
    RequestBody, ValidationIssue,
};
use url::form_urlencoded;

use crate::authentication::{self, base64_encode};
use crate::errors::HttpError;

/// Per-send options that are not part of the saved request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendOptions {
    pub timeout: Duration,
    pub follow_redirects: bool,
    /// Send the request body even for methods where it is unusual.
    pub allow_body_on_get: bool,
}

impl Default for SendOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            follow_redirects: true,
            allow_body_on_get: true,
        }
    }
}

impl SendOptions {
    pub fn with_timeout_secs(mut self, seconds: u64) -> Self {
        self.timeout = Duration::from_secs(seconds.clamp(1, 300));
        self
    }
}

/// The body as it will be written to the wire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedBody {
    Text { content: String },
    Form { encoded: String },
}

impl ResolvedBody {
    pub fn bytes(&self) -> Vec<u8> {
        match self {
            ResolvedBody::Text { content } => content.clone().into_bytes(),
            ResolvedBody::Form { encoded } => encoded.clone().into_bytes(),
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            ResolvedBody::Text { content } => content.is_empty(),
            ResolvedBody::Form { encoded } => encoded.is_empty(),
        }
    }
}

/// Authentication that cannot be reduced to a fixed header at prepare time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthPlan {
    None,
    /// Retry the request with a computed header after a 401 challenge.
    Digest {
        username: String,
        password: String,
    },
    /// Sign every request with AWS Signature Version 4.
    AwsSigV4(crate::aws_sigv4::AwsCredentials),
    /// Fetch (or reuse) a token from the token endpoint.
    OAuth2(requestlab_models::Authentication),
}

/// A request that is fully resolved and ready for `HttpClient::execute`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedRequest {
    pub method: HttpMethod,
    pub url: String,
    /// Final header list in wire order. May contain credentials.
    pub headers: Vec<(String, String)>,
    pub body: Option<ResolvedBody>,
    pub timeout: Duration,
    pub follow_redirects: bool,
    pub auth: AuthPlan,
}

impl PreparedRequest {
    /// Look up a header without exposing whether it is sensitive.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Header list with credentials replaced, safe for logging.
    pub fn redacted_headers(&self) -> Vec<(String, String)> {
        self.headers
            .iter()
            .map(|(name, value)| {
                let value = if is_sensitive_header(name) {
                    requestlab_models::REDACTED.to_string()
                } else {
                    value.clone()
                };
                (name.clone(), value)
            })
            .collect()
    }

    /// A one-line description for logs: method, host and path only.
    pub fn log_target(&self) -> String {
        let sanitized = sanitize_url(&self.url);
        format!("{} {}", self.method, sanitized)
    }
}

/// Remove query values that look like credentials before logging a URL.
pub fn sanitize_url(raw: &str) -> String {
    let Ok(mut url) = url::Url::parse(raw) else {
        return raw.to_string();
    };
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            let redacted = if is_sensitive_header(&key)
                || key.to_ascii_lowercase().contains("key")
                || key.to_ascii_lowercase().contains("token")
                || key.to_ascii_lowercase().contains("secret")
            {
                requestlab_models::REDACTED.to_string()
            } else {
                value.to_string()
            };
            (key.to_string(), redacted)
        })
        .collect();
    if !pairs.is_empty() {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    url.to_string()
}

/// Validate and assemble a request for execution.
pub fn prepare_request(
    request: &Request,
    options: &SendOptions,
) -> Result<PreparedRequest, HttpError> {
    let issues: Vec<ValidationIssue> = request.validate();
    if !issues.is_empty() {
        return Err(HttpError::Validation(issues));
    }

    let raw_url = normalize_url(&request.url);
    let mut url = validate_url(&raw_url).map_err(HttpError::InvalidUrl)?;

    // Enabled query rows are appended to whatever the URL already carries, with
    // proper percent-encoding for both keys and values.
    let query_rows: Vec<(String, String)> = request
        .query_params
        .iter()
        .filter(|param| param.enabled && !param.key.trim().is_empty())
        .map(|param| (param.key.trim().to_string(), param.value.clone()))
        .collect();

    let auth = authentication::apply(&request.authentication);

    if !query_rows.is_empty() || !auth.query.is_empty() {
        let mut serializer = form_urlencoded::Serializer::new(String::new());
        for (key, value) in url.query_pairs() {
            serializer.append_pair(&key, &value);
        }
        for (key, value) in query_rows.iter().chain(auth.query.iter()) {
            serializer.append_pair(key.as_str(), value.as_str());
        }
        url.set_query(Some(&serializer.finish()));
    }

    let mut headers: Vec<(String, String)> = request
        .headers
        .iter()
        .filter(|header| header.enabled && !header.key.trim().is_empty())
        .map(|header| (header.key.trim().to_string(), header.value.clone()))
        .collect();

    for (name, value) in auth.headers {
        // An explicit Authorization header wins over auth config only when the
        // user disabled auth; otherwise auth is authoritative.
        headers.retain(|(existing, _)| !existing.eq_ignore_ascii_case(&name));
        headers.push((name, value));
    }

    let has_content_type = headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("content-type"));

    let body = match &request.body {
        RequestBody::None => None,
        RequestBody::Json { content } => {
            if !has_content_type {
                headers.push(("Content-Type".to_string(), "application/json".to_string()));
            }
            Some(ResolvedBody::Text {
                content: content.clone(),
            })
        }
        RequestBody::Raw {
            content,
            content_type,
        } => {
            if !has_content_type {
                let content_type = if content_type.trim().is_empty() {
                    "text/plain"
                } else {
                    content_type.trim()
                };
                headers.push(("Content-Type".to_string(), content_type.to_string()));
            }
            Some(ResolvedBody::Text {
                content: content.clone(),
            })
        }
        RequestBody::FormUrlEncoded { fields } => {
            let mut serializer = form_urlencoded::Serializer::new(String::new());
            for field in fields.iter().filter(|field| field.enabled) {
                if field.key.trim().is_empty() {
                    continue;
                }
                serializer.append_pair(field.key.trim(), &field.value);
            }
            if !has_content_type {
                headers.push((
                    "Content-Type".to_string(),
                    "application/x-www-form-urlencoded".to_string(),
                ));
            }
            Some(ResolvedBody::Form {
                encoded: serializer.finish(),
            })
        }
    };

    let body = body.filter(|body| !body.is_empty() || !matches!(&request.body, RequestBody::None));
    let body = if request.method.forbids_body() {
        None
    } else {
        body
    };

    // A body-less GET stays body-less unless the user explicitly kept content.
    let body = match (&body, request.method) {
        (Some(ResolvedBody::Text { content }), HttpMethod::Get | HttpMethod::Options)
            if content.is_empty() =>
        {
            None
        }
        _ => body,
    };

    let auth = match &request.authentication {
        Authentication::Digest { username, password } => AuthPlan::Digest {
            username: username.clone(),
            password: password.clone(),
        },
        Authentication::AwsSigV4 {
            access_key,
            secret_key,
            region,
            service,
            session_token,
        } => AuthPlan::AwsSigV4(crate::aws_sigv4::AwsCredentials {
            access_key: access_key.clone(),
            secret_key: secret_key.clone(),
            region: region.clone(),
            service: service.clone(),
            session_token: session_token.clone(),
        }),
        Authentication::OAuth2 { .. } => AuthPlan::OAuth2(request.authentication.clone()),
        _ => AuthPlan::None,
    };

    Ok(PreparedRequest {
        method: request.method,
        url: url.to_string(),
        headers,
        body,
        timeout: options.timeout,
        follow_redirects: options.follow_redirects,
        auth,
    })
}

/// Convenience: prepare and report the URL that will actually be requested.
pub fn prepared_url(request: &Request, options: &SendOptions) -> Result<String, HttpError> {
    prepare_request(request, options).map(|prepared| prepared.url)
}

/// Encode a value for use in a query string, for UI previews.
pub fn encode_query_value(value: &str) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    serializer.append_pair("v", value);
    let encoded = serializer.finish();
    encoded.strip_prefix("v=").unwrap_or(&encoded).to_string()
}

/// Basic auth header value, exposed for tests and previews.
pub fn basic_auth_header(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        base64_encode(format!("{username}:{password}").as_bytes())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use requestlab_models::{
        ApiKeyLocation, Authentication, HeaderEntry, QueryParameter, RequestBody,
    };

    fn options() -> SendOptions {
        SendOptions::default()
    }

    #[test]
    fn builds_url_with_query_parameters() {
        let mut request = Request::new(HttpMethod::Get, "https://api.example.com/users?page=1");
        request
            .query_params
            .push(QueryParameter::new("per_page", "50"));
        request
            .query_params
            .push(QueryParameter::new("q", "ada lovelace & co"));
        let mut disabled = QueryParameter::new("debug", "true");
        disabled.enabled = false;
        request.query_params.push(disabled);

        let prepared = prepare_request(&request, &options()).unwrap();
        assert!(prepared.url.contains("page=1"));
        assert!(prepared.url.contains("per_page=50"));
        assert!(prepared.url.contains("q=ada+lovelace+%26+co"));
        assert!(!prepared.url.contains("debug"));
    }

    #[test]
    fn adds_scheme_for_bare_hosts() {
        let request = Request::new(HttpMethod::Get, "localhost:3000/health");
        let prepared = prepare_request(&request, &options()).unwrap();
        assert_eq!(prepared.url, "http://localhost:3000/health");
    }

    #[test]
    fn sets_content_type_from_body_kind() {
        let mut request = Request::new(HttpMethod::Post, "https://example.com");
        request.body = RequestBody::json("{\"a\":1}");
        let prepared = prepare_request(&request, &options()).unwrap();
        assert_eq!(prepared.header("Content-Type"), Some("application/json"));

        let mut form = Request::new(HttpMethod::Post, "https://example.com");
        form.body = RequestBody::form(vec![
            QueryParameter::new("name", "Ada Lovelace"),
            QueryParameter::new("role", "admin & owner"),
        ]);
        let prepared = prepare_request(&form, &options()).unwrap();
        assert_eq!(
            prepared.header("Content-Type"),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(
            prepared.body,
            Some(ResolvedBody::Form {
                encoded: "name=Ada+Lovelace&role=admin+%26+owner".to_string()
            })
        );
    }

    #[test]
    fn user_content_type_wins_over_default() {
        let mut request = Request::new(HttpMethod::Post, "https://example.com");
        request
            .headers
            .push(HeaderEntry::new("Content-Type", "application/vnd.api+json"));
        request.body = RequestBody::json("{}");
        let prepared = prepare_request(&request, &options()).unwrap();
        assert_eq!(
            prepared.header("Content-Type"),
            Some("application/vnd.api+json")
        );
        assert_eq!(
            prepared
                .headers
                .iter()
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-type"))
                .count(),
            1
        );
    }

    #[test]
    fn auth_headers_replace_user_authorization() {
        let mut request = Request::new(HttpMethod::Get, "https://example.com");
        request
            .headers
            .push(HeaderEntry::new("Authorization", "stale"));
        request.authentication = Authentication::Bearer {
            token: "fresh".into(),
        };
        let prepared = prepare_request(&request, &options()).unwrap();
        let auth_headers: Vec<_> = prepared
            .headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
            .collect();
        assert_eq!(auth_headers.len(), 1);
        assert_eq!(auth_headers[0].1, "Bearer fresh");
    }

    #[test]
    fn api_key_in_query_is_appended_and_redacted_in_logs() {
        let mut request = Request::new(HttpMethod::Get, "https://example.com/search");
        request.authentication = Authentication::ApiKey {
            key: "api_key".into(),
            value: "super-secret".into(),
            location: ApiKeyLocation::Query,
        };
        let prepared = prepare_request(&request, &options()).unwrap();
        assert!(prepared.url.contains("api_key=super-secret"));
        let target = prepared.log_target();
        assert!(!target.contains("super-secret"));
        assert!(target.contains("api_key=%5Bredacted%5D") || target.contains("api_key=[redacted]"));
    }

    #[test]
    fn disabled_and_unnamed_headers_are_dropped() {
        let mut request = Request::new(HttpMethod::Get, "https://example.com");
        let mut disabled = HeaderEntry::new("X-Debug", "1");
        disabled.enabled = false;
        request.headers.push(disabled);
        request.headers.push(HeaderEntry::new("", ""));
        let prepared = prepare_request(&request, &options()).unwrap();
        assert!(prepared.headers.is_empty());
    }

    #[test]
    fn header_order_is_preserved() {
        let mut request = Request::new(HttpMethod::Get, "https://example.com");
        request.headers.push(HeaderEntry::new("X-Third", "3"));
        request.headers.push(HeaderEntry::new("X-First", "1"));
        request.headers.push(HeaderEntry::new("X-Second", "2"));
        let prepared = prepare_request(&request, &options()).unwrap();
        let names: Vec<&str> = prepared
            .headers
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(names, vec!["X-Third", "X-First", "X-Second"]);
    }

    #[test]
    fn invalid_requests_are_rejected_before_network_use() {
        let request = Request::blank();
        let error = prepare_request(&request, &options()).unwrap_err();
        assert!(matches!(error, HttpError::Validation(_)));

        let mut bad_json = Request::new(HttpMethod::Post, "https://example.com");
        bad_json.body = RequestBody::json("{");
        assert!(matches!(
            prepare_request(&bad_json, &options()),
            Err(HttpError::Validation(_))
        ));

        let mut bad_header = Request::new(HttpMethod::Get, "https://example.com");
        bad_header.headers.push(HeaderEntry::new("Bad Name", "x"));
        assert!(matches!(
            prepare_request(&bad_header, &options()),
            Err(HttpError::Validation(_))
        ));
    }

    #[test]
    fn head_requests_never_carry_a_body() {
        let mut request = Request::new(HttpMethod::Head, "https://example.com");
        request.body = RequestBody::None;
        let prepared = prepare_request(&request, &options()).unwrap();
        assert!(prepared.body.is_none());
    }

    #[test]
    fn redacted_headers_hide_credentials() {
        let mut request = Request::new(HttpMethod::Get, "https://example.com");
        request.authentication = Authentication::Basic {
            username: "ada".into(),
            password: "secret".into(),
        };
        let prepared = prepare_request(&request, &options()).unwrap();
        let redacted = prepared.redacted_headers();
        assert_eq!(redacted[0].1, requestlab_models::REDACTED);
        assert_eq!(prepared.headers[0].1, basic_auth_header("ada", "secret"));
    }

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(encode_query_value("a b"), "a+b");
        assert_eq!(encode_query_value("a/b"), "a%2Fb");
    }
}
