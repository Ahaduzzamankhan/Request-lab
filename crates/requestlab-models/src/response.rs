//! Response models: bodies, headers, cookies and content classification.

use serde::{Deserialize, Serialize};

use crate::common::human_bytes;

/// A response body either as text or as raw bytes, always bounded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseBody {
    Empty,
    Text {
        text: String,
        #[serde(default)]
        truncated: bool,
    },
    Binary {
        bytes: Vec<u8>,
        #[serde(default)]
        truncated: bool,
    },
}

impl ResponseBody {
    pub fn text(text: impl Into<String>) -> Self {
        ResponseBody::Text {
            text: text.into(),
            truncated: false,
        }
    }

    pub fn binary(bytes: Vec<u8>) -> Self {
        ResponseBody::Binary {
            bytes,
            truncated: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            ResponseBody::Empty => true,
            ResponseBody::Text { text, .. } => text.is_empty(),
            ResponseBody::Binary { bytes, .. } => bytes.is_empty(),
        }
    }

    /// Byte length of the stored body.
    pub fn len(&self) -> usize {
        match self {
            ResponseBody::Empty => 0,
            ResponseBody::Text { text, .. } => text.len(),
            ResponseBody::Binary { bytes, .. } => bytes.len(),
        }
    }

    pub fn is_truncated(&self) -> bool {
        match self {
            ResponseBody::Empty => false,
            ResponseBody::Text { truncated, .. } | ResponseBody::Binary { truncated, .. } => {
                *truncated
            }
        }
    }

    /// Text view of the body when it is stored as text.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ResponseBody::Text { text, .. } => Some(text),
            _ => None,
        }
    }
}

/// How the response body is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyKind {
    Empty,
    Json,
    Xml,
    Html,
    Text,
    Binary,
}

impl BodyKind {
    pub fn label(self) -> &'static str {
        match self {
            BodyKind::Empty => "Empty",
            BodyKind::Json => "JSON",
            BodyKind::Xml => "XML",
            BodyKind::Html => "HTML",
            BodyKind::Text => "Text",
            BodyKind::Binary => "Binary",
        }
    }

    /// Whether the Pretty tab can show this body.
    pub fn is_textual(self) -> bool {
        matches!(
            self,
            BodyKind::Json | BodyKind::Xml | BodyKind::Html | BodyKind::Text
        )
    }
}

/// A single response header, preserving the order sent by the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseHeader {
    pub name: String,
    pub value: String,
}

/// A cookie parsed from a `Set-Cookie` header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub expires: Option<String>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<String>,
}

impl Cookie {
    /// Parse one `Set-Cookie` header value. Returns `None` when unparseable.
    pub fn parse_set_cookie(raw: &str) -> Option<Self> {
        let mut parts = raw.split(';');
        let (name, value) = parts.next()?.split_once('=')?;
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let mut cookie = Cookie {
            name: name.to_string(),
            value: value.trim().to_string(),
            domain: None,
            path: None,
            expires: None,
            secure: false,
            http_only: false,
            same_site: None,
        };
        for attribute in parts {
            let attribute = attribute.trim();
            if attribute.is_empty() {
                continue;
            }
            let (key, attribute_value) = match attribute.split_once('=') {
                Some((key, value)) => (key.trim(), value.trim()),
                None => (attribute, ""),
            };
            match key.to_ascii_lowercase().as_str() {
                "domain" => cookie.domain = Some(attribute_value.to_string()),
                "path" => cookie.path = Some(attribute_value.to_string()),
                "expires" => cookie.expires = Some(attribute_value.to_string()),
                "max-age" => cookie.expires = Some(format!("Max-Age={attribute_value}")),
                "secure" => cookie.secure = true,
                "httponly" => cookie.http_only = true,
                "samesite" => cookie.same_site = Some(attribute_value.to_string()),
                _ => {}
            }
        }
        Some(cookie)
    }
}

/// Classify a response body from its content type and bytes.
///
/// Never assumes JSON: the content type is authoritative when present, and the
/// bytes decide the fallback between text and binary.
pub fn classify_body(content_type: Option<&str>, bytes: &[u8]) -> BodyKind {
    if bytes.is_empty() {
        return BodyKind::Empty;
    }
    if let Some(content_type) = content_type {
        let lowered = content_type.to_ascii_lowercase();
        let essence = lowered.split(';').next().unwrap_or("").trim();
        if essence.contains("json") || essence.ends_with("+json") {
            return BodyKind::Json;
        }
        if essence.contains("xml") {
            return BodyKind::Xml;
        }
        if essence.contains("html") {
            return BodyKind::Html;
        }
        if essence.starts_with("text/")
            || essence.contains("javascript")
            || essence.contains("x-www-form-urlencoded")
            || essence.contains("graphql")
            || essence.contains("yaml")
            || essence.contains("csv")
        {
            return BodyKind::Text;
        }
        if essence == "application/octet-stream"
            || essence.starts_with("image/")
            || essence.starts_with("audio/")
            || essence.starts_with("video/")
            || essence.contains("zip")
            || essence.contains("pdf")
            || essence.contains("protobuf")
        {
            return BodyKind::Binary;
        }
    }
    if looks_like_text(bytes) {
        BodyKind::Text
    } else {
        BodyKind::Binary
    }
}

/// Heuristic: valid UTF-8 with no disallowed control characters.
pub fn looks_like_text(bytes: &[u8]) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(text) => !text
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')),
        Err(_) => false,
    }
}

/// Compact information about a response, for the status bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResponseSummary {
    pub status: u16,
    pub status_text: String,
    pub duration_ms: u64,
    pub size_bytes: usize,
    pub content_type: Option<String>,
    pub body_kind: BodyKind,
}

/// A fully captured HTTP response.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<ResponseHeader>,
    pub body: ResponseBody,
    pub content_type: Option<String>,
    pub size_bytes: usize,
    pub duration_ms: u64,
    pub completed_at: String,
    pub url: String,
    #[serde(default)]
    pub cookies: Vec<Cookie>,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn is_redirect(&self) -> bool {
        (300..400).contains(&self.status)
    }

    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }

    pub fn body_kind(&self) -> BodyKind {
        match &self.body {
            ResponseBody::Empty => BodyKind::Empty,
            ResponseBody::Binary { .. } => BodyKind::Binary,
            ResponseBody::Text { text, .. } => {
                classify_body(self.content_type.as_deref(), text.as_bytes())
            }
        }
    }

    /// First header matching `name`, case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.as_str())
    }

    /// All headers matching `name`, case-insensitively.
    pub fn headers_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.headers
            .iter()
            .filter(move |header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.as_str())
    }

    pub fn text(&self) -> Option<&str> {
        self.body.as_text()
    }

    /// Pretty-printed JSON when the body is JSON text that parses.
    ///
    /// Returns `Err` with the parse error when the server claims JSON but sends
    /// something else, so the UI can show a real error state.
    pub fn pretty_json(&self) -> Option<Result<String, String>> {
        if !matches!(self.body_kind(), BodyKind::Json) {
            return None;
        }
        let text = self.body.as_text()?;
        Some(match serde_json::from_str::<serde_json::Value>(text) {
            Ok(value) => serde_json::to_string_pretty(&value)
                .map_err(|err| format!("Could not format JSON: {err}")),
            Err(err) => Err(format!(
                "Response is not valid JSON (line {}, column {}): {err}",
                err.line(),
                err.column()
            )),
        })
    }

    /// `1.4 KB` style size label.
    pub fn size_label(&self) -> String {
        human_bytes(self.size_bytes as u64)
    }

    pub fn summary(&self) -> ResponseSummary {
        ResponseSummary {
            status: self.status,
            status_text: self.status_text.clone(),
            duration_ms: self.duration_ms,
            size_bytes: self.size_bytes,
            content_type: self.content_type.clone(),
            body_kind: self.body_kind(),
        }
    }

    /// Copy with cookie and authorization headers redacted, for logs and history.
    pub fn redacted_headers(&self) -> Vec<ResponseHeader> {
        self.headers
            .iter()
            .map(|header| ResponseHeader {
                name: header.name.clone(),
                value: crate::common::redact_if_sensitive(&header.name, &header.value),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: u16, content_type: Option<&str>, body: ResponseBody) -> HttpResponse {
        HttpResponse {
            status,
            status_text: "OK".into(),
            headers: vec![ResponseHeader {
                name: "Content-Type".into(),
                value: content_type.unwrap_or("").into(),
            }],
            body,
            content_type: content_type.map(str::to_string),
            size_bytes: 0,
            duration_ms: 12,
            completed_at: "2024-01-01T00:00:00.000Z".into(),
            url: "https://example.com".into(),
            cookies: Vec::new(),
        }
    }

    #[test]
    fn classifies_common_content_types() {
        assert_eq!(
            classify_body(Some("application/json; charset=utf-8"), b"{}"),
            BodyKind::Json
        );
        assert_eq!(
            classify_body(Some("application/problem+json"), b"{}"),
            BodyKind::Json
        );
        assert_eq!(classify_body(Some("text/xml"), b"<a/>"), BodyKind::Xml);
        assert_eq!(classify_body(Some("text/html"), b"<p/>"), BodyKind::Html);
        assert_eq!(classify_body(Some("text/plain"), b"hello"), BodyKind::Text);
        assert_eq!(
            classify_body(Some("application/octet-stream"), &[0xff, 0x00]),
            BodyKind::Binary
        );
        assert_eq!(
            classify_body(Some("application/json"), b""),
            BodyKind::Empty
        );
    }

    #[test]
    fn falls_back_to_content_sniffing() {
        assert_eq!(classify_body(None, b"{\"a\":1}"), BodyKind::Text);
        assert_eq!(classify_body(None, &[0, 159, 146, 150]), BodyKind::Binary);
        assert_eq!(
            classify_body(Some("application/unknown"), b"plain"),
            BodyKind::Text
        );
    }

    #[test]
    fn pretty_json_formats_and_reports_errors() {
        let ok = response(
            200,
            Some("application/json"),
            ResponseBody::text("{\"a\":1,\"b\":[1,2]}"),
        );
        let pretty = ok.pretty_json().unwrap().unwrap();
        assert!(pretty.contains("\n"));
        assert!(pretty.contains("\"a\": 1"));

        let broken = response(
            200,
            Some("application/json"),
            ResponseBody::text("not json"),
        );
        assert!(broken.pretty_json().unwrap().is_err());

        let text = response(200, Some("text/plain"), ResponseBody::text("hi"));
        assert!(text.pretty_json().is_none());
    }

    #[test]
    fn status_helpers() {
        let ok = response(204, None, ResponseBody::Empty);
        assert!(ok.is_success());
        assert!(response(302, None, ResponseBody::Empty).is_redirect());
        assert!(response(404, None, ResponseBody::Empty).is_client_error());
        assert!(response(500, None, ResponseBody::Empty).is_server_error());
    }

    #[test]
    fn parses_set_cookie_attributes() {
        let cookie = Cookie::parse_set_cookie(
            "session=abc123; Path=/; Domain=example.com; HttpOnly; Secure; SameSite=Lax",
        )
        .unwrap();
        assert_eq!(cookie.name, "session");
        assert_eq!(cookie.value, "abc123");
        assert_eq!(cookie.path.as_deref(), Some("/"));
        assert_eq!(cookie.domain.as_deref(), Some("example.com"));
        assert!(cookie.http_only && cookie.secure);
        assert_eq!(cookie.same_site.as_deref(), Some("Lax"));
        assert!(Cookie::parse_set_cookie("garbage").is_none());
    }

    #[test]
    fn header_lookup_is_case_insensitive() {
        let response = response(200, Some("application/json"), ResponseBody::text("{}"));
        assert_eq!(response.header("content-type"), Some("application/json"));
        assert_eq!(response.header("missing"), None);
    }

    #[test]
    fn redaction_hides_credentials() {
        let mut response = response(200, None, ResponseBody::Empty);
        response.headers.push(ResponseHeader {
            name: "Set-Cookie".into(),
            value: "session=secret".into(),
        });
        let redacted = response.redacted_headers();
        assert!(redacted
            .iter()
            .any(|header| header.name == "Set-Cookie" && header.value == crate::common::REDACTED));
    }

    #[test]
    fn size_and_kind_labels() {
        assert_eq!(BodyKind::Json.label(), "JSON");
        assert!(BodyKind::Xml.is_textual());
        assert!(!BodyKind::Binary.is_textual());
        let response = response(200, Some("application/json"), ResponseBody::text("{}"));
        assert_eq!(response.size_label(), "0 B");
    }
}
