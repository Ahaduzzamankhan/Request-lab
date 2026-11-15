//! Async HTTP client built on Reqwest with Rustls.

use std::time::Duration;

use requestlab_models::{HttpMethod, HttpResponse};
use tokio::sync::oneshot;

use crate::builder::PreparedRequest;
use crate::errors::HttpError;
use crate::response;
use crate::timing::RequestTimer;

/// Client-wide settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpConfig {
    /// Default per-request timeout.
    pub timeout: Duration,
    pub connect_timeout: Duration,
    pub max_redirects: usize,
    /// Response body cap in bytes.
    pub max_response_bytes: usize,
    pub user_agent: String,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            max_redirects: 10,
            max_response_bytes: requestlab_models::MAX_RESPONSE_BYTES,
            user_agent: format!("RequestLab/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

/// Handle used by the UI to cancel an in-flight request.
#[derive(Debug)]
pub struct Canceller {
    sender: Option<oneshot::Sender<()>>,
}

impl Canceller {
    /// Request cancellation. Safe to call more than once.
    pub fn cancel(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(());
        }
    }

    /// Whether the cancellation signal can still be sent.
    pub fn is_armed(&self) -> bool {
        self.sender.is_some()
    }
}

/// Create a cancellation pair: hand the [`Canceller`] to the UI and pass the
/// receiver to [`HttpClient::execute`].
pub fn cancellation_channel() -> (Canceller, oneshot::Receiver<()>) {
    let (sender, receiver) = oneshot::channel();
    (
        Canceller {
            sender: Some(sender),
        },
        receiver,
    )
}

/// The HTTP engine. Cheap to clone internally; build once and share.
///
/// Two clients are kept: one following redirects (the default) and one that does
/// not, because Reqwest only allows the redirect policy at client build time.
#[derive(Debug, Clone)]
pub struct HttpClient {
    client: reqwest::Client,
    direct_client: reqwest::Client,
    config: HttpConfig,
}

impl HttpClient {
    pub fn new(config: HttpConfig) -> Result<Self, HttpError> {
        let builder = || {
            reqwest::Client::builder()
                .user_agent(config.user_agent.clone())
                .timeout(config.timeout)
                .connect_timeout(config.connect_timeout)
                .use_rustls_tls()
                // Certificate verification stays on: no insecure escape hatch.
                .danger_accept_invalid_certs(false)
                .danger_accept_invalid_hostnames(false)
                .pool_idle_timeout(Duration::from_secs(90))
        };
        let client = builder()
            .redirect(reqwest::redirect::Policy::limited(config.max_redirects))
            .build()
            .map_err(HttpError::from_reqwest)?;
        let direct_client = builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(HttpError::from_reqwest)?;
        Ok(Self {
            client,
            direct_client,
            config,
        })
    }

    pub fn config(&self) -> &HttpConfig {
        &self.config
    }

    /// Execute a prepared request.
    ///
    /// The future completes with [`HttpError::Cancelled`] as soon as the
    /// cancellation receiver fires, without waiting for the server.
    pub async fn execute(
        &self,
        prepared: &PreparedRequest,
        cancel: oneshot::Receiver<()>,
    ) -> Result<HttpResponse, HttpError> {
        let mut timer = RequestTimer::start();
        let timeout = if prepared.timeout.is_zero() {
            self.config.timeout
        } else {
            prepared.timeout
        };
        let max_bytes = self.config.max_response_bytes;

        let request = self.build_request(prepared, timeout)?;
        let started = timer.started_at();
        tracing::debug!(target = %prepared.log_target(), "sending request");

        let exchange = async move {
            match request.send().await {
                Ok(resp) => response::capture(resp, max_bytes, started).await,
                Err(err) => Err(HttpError::from_reqwest(err)),
            }
        };

        let outcome = tokio::select! {
            biased;
            _ = cancel => Err(HttpError::Cancelled),
            result = exchange => result,
        };

        let elapsed = timer.stop();
        outcome.map_err(|err| {
            let err = err.with_timeout_ms(elapsed);
            if !err.is_cancelled() {
                tracing::warn!(target = %prepared.log_target(), category = err.category(), "request failed: {}", err);
            }
            err
        })
    }

    fn build_request(
        &self,
        prepared: &PreparedRequest,
        timeout: Duration,
    ) -> Result<reqwest::RequestBuilder, HttpError> {
        let method = match prepared.method {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Patch => reqwest::Method::PATCH,
            HttpMethod::Delete => reqwest::Method::DELETE,
            HttpMethod::Head => reqwest::Method::HEAD,
            HttpMethod::Options => reqwest::Method::OPTIONS,
        };

        let client = if prepared.follow_redirects {
            &self.client
        } else {
            &self.direct_client
        };
        let mut builder = client.request(method, &prepared.url).timeout(timeout);

        for (name, value) in &prepared.headers {
            builder = builder.header(name, value);
        }

        if let Some(body) = &prepared.body {
            builder = builder.body(body.bytes());
        }

        Ok(builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sane() {
        let config = HttpConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert_eq!(config.max_redirects, 10);
        assert_eq!(
            config.max_response_bytes,
            requestlab_models::MAX_RESPONSE_BYTES
        );
        assert!(config.user_agent.starts_with("RequestLab/"));
    }

    #[test]
    fn canceller_can_fire_once() {
        let (mut canceller, mut receiver) = cancellation_channel();
        assert!(canceller.is_armed());
        canceller.cancel();
        assert!(!canceller.is_armed());
        assert!(receiver.try_recv().is_ok());
        canceller.cancel();
    }
}
