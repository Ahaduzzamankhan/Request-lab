//! Typed HTTP errors with user-facing messages.

use requestlab_models::ValidationIssue;
use thiserror::Error;

/// Everything that can go wrong while preparing or executing a request.
#[derive(Debug, Clone, Error)]
pub enum HttpError {
    #[error("The request is not valid yet")]
    Validation(Vec<ValidationIssue>),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),

    #[error("Could not build the request: {0}")]
    Build(String),

    #[error("The request timed out after {0} ms")]
    Timeout(u64),

    #[error("Could not connect to the server: {0}")]
    Connect(String),

    #[error("The server refused the connection")]
    ConnectionRefused,

    #[error("Too many redirects")]
    TooManyRedirects,

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("The request was cancelled")]
    Cancelled,

    #[error("Could not read the response body: {0}")]
    Body(String),

    #[error("Network error: {0}")]
    Network(String),
}

impl HttpError {
    /// Convert a Reqwest error, keeping the message free of credentials.
    pub fn from_reqwest(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            return HttpError::Timeout(0);
        }
        if error.is_redirect() {
            return HttpError::TooManyRedirects;
        }
        if error.is_connect() {
            let message = root_cause(&error);
            if message.to_ascii_lowercase().contains("refused") {
                return HttpError::ConnectionRefused;
            }
            if message.to_ascii_lowercase().contains("certificate")
                || message.to_ascii_lowercase().contains("tls")
                || message.to_ascii_lowercase().contains("handshake")
            {
                return HttpError::Tls(message);
            }
            return HttpError::Connect(message);
        }
        if error.is_body() {
            return HttpError::Body(root_cause(&error));
        }
        if error.is_builder() {
            return HttpError::Build(root_cause(&error));
        }
        HttpError::Network(root_cause(&error))
    }

    /// Replace the timeout duration once the elapsed time is known.
    pub fn with_timeout_ms(self, millis: u64) -> Self {
        match self {
            HttpError::Timeout(_) => HttpError::Timeout(millis),
            other => other,
        }
    }

    /// A short category for the response status bar.
    pub fn category(&self) -> &'static str {
        match self {
            HttpError::Validation(_) => "Validation",
            HttpError::InvalidUrl(_) => "URL",
            HttpError::Build(_) => "Build",
            HttpError::Timeout(_) => "Timeout",
            HttpError::Connect(_) | HttpError::ConnectionRefused => "Connection",
            HttpError::TooManyRedirects => "Redirect",
            HttpError::Tls(_) => "TLS",
            HttpError::Cancelled => "Cancelled",
            HttpError::Body(_) => "Body",
            HttpError::Network(_) => "Network",
        }
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, HttpError::Cancelled)
    }
}

/// Walk the error chain to the most specific cause.
fn root_cause(error: &reqwest::Error) -> String {
    let mut source: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(error);
    let mut message = error.to_string();
    let mut depth = 0;
    while let Some(cause) = source {
        message = cause.to_string();
        source = cause.source();
        depth += 1;
        if depth > 8 {
            break;
        }
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_are_stable() {
        assert_eq!(HttpError::Cancelled.category(), "Cancelled");
        assert!(HttpError::Cancelled.is_cancelled());
        assert_eq!(
            HttpError::Timeout(5).to_string(),
            "The request timed out after 5 ms"
        );
        assert_eq!(
            HttpError::Timeout(1).with_timeout_ms(250).to_string(),
            "The request timed out after 250 ms"
        );
    }
}
