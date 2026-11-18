//! Shared, UI-independent data models for RequestLab.
//!
//! This crate has no knowledge of GPUI, SQLx or Reqwest. Every other crate in the
//! workspace may depend on it, and it depends only on serialization primitives.

pub mod collection;
pub mod common;
pub mod curl;
pub mod environment;
pub mod history;
pub mod inspector;
pub mod postman;
pub mod request;
pub mod response;

pub use collection::{
    build_tree, folder_depth, Collection, CollectionNode, CollectionTree, Folder, SavedRequest,
};
pub use common::{
    contains_template_variable, human_bytes, human_duration, is_sensitive_header,
    is_valid_header_name, is_valid_header_value, normalize_url, now_rfc3339, redact_if_sensitive,
    redact_url, truncate_utf8, validate_url, RequestField, ValidationIssue, DEFAULT_HISTORY_LIMIT,
    MAX_FOLDER_DEPTH, MAX_HISTORY_BODY_BYTES, MAX_RESPONSE_BYTES, REDACTED,
};
pub use curl::CurlError;
pub use environment::{Environment, EnvironmentVariable};
pub use history::{HistoryEntry, HistoryQuery, HistoryStatusFilter};
pub use inspector::{
    available_views, body_preview, download_name, indent_markup, search, BodyPreview, BodyView,
    SearchHit, SearchResult,
};
pub use postman::{
    export as export_postman, export_warnings as postman_export_warnings, import as import_postman,
    ImportedCollection, ImportedRequest, PostmanError,
};
pub use request::{
    ApiKeyLocation, Authentication, HeaderEntry, HttpMethod, OAuth2Grant, QueryParameter, Request,
    RequestBody, RequestBodyKind,
};
pub use response::{
    classify_body, looks_like_text, BodyKind, Cookie, HttpResponse, ResponseBody, ResponseHeader,
    ResponseSummary,
};
