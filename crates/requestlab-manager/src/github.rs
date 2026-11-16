//! Minimal GitHub Releases API client.
//!
//! Requests are authenticated when a token is available, rate-limit responses
//! become a typed error naming the reset time, and the API base is configurable
//! so tests and enterprise mirrors do not need a rebuild.

use std::time::Duration;

use chrono::{DateTime, TimeZone as _, Utc};
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::{Response, StatusCode};

use crate::downloader::require_https;
use crate::error::{ManagerError, ManagerResult};
use crate::releases::{GitHubAsset, GitHubRelease};
use crate::{REPO_NAME, REPO_OWNER};

/// Public GitHub API root.
pub const DEFAULT_API_BASE: &str = "https://api.github.com";
/// Token variables read in order.
pub const TOKEN_ENV: [&str; 2] = ["REQUESTLAB_GITHUB_TOKEN", "GITHUB_TOKEN"];
/// Override for the API base (tests, enterprise mirrors).
pub const API_BASE_ENV: &str = "REQUESTLAB_GITHUB_API";
/// Largest metadata asset accepted from a release.
pub const MAX_METADATA_BYTES: u64 = 4 * 1024 * 1024;

/// GitHub Releases client for one repository.
#[derive(Debug, Clone)]
pub struct GitHubClient {
    http: reqwest::Client,
    base: String,
    owner: String,
    repo: String,
    token: Option<String>,
}

impl GitHubClient {
    pub fn new() -> ManagerResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(user_agent())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| {
                ManagerError::Network(format!("cannot build the HTTP client: {error}"))
            })?;
        Ok(Self {
            http,
            base: std::env::var(API_BASE_ENV).unwrap_or_else(|_| DEFAULT_API_BASE.to_string()),
            owner: REPO_OWNER.to_string(),
            repo: REPO_NAME.to_string(),
            token: None,
        })
    }

    /// A client with a token from the environment, if one is set.
    pub fn from_env() -> ManagerResult<Self> {
        let mut client = Self::new()?;
        client.token = token_from_env();
        Ok(client)
    }

    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into().trim_end_matches('/').to_string();
        self
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn repository(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    pub fn is_authenticated(&self) -> bool {
        self.token.is_some()
    }

    fn api_url(&self, path: &str) -> String {
        format!(
            "{}/repos/{}/{}/{}",
            self.base,
            self.owner,
            self.repo,
            path.trim_start_matches('/')
        )
    }

    /// Fetch releases, newest first.
    pub async fn list_releases(&self, per_page: u32) -> ManagerResult<Vec<GitHubRelease>> {
        let per_page = per_page.clamp(1, 100);
        let url = format!("{}?per_page={per_page}", self.api_url("releases"));
        let response = self.get(&url).await?;
        let body = read_limited(response, MAX_METADATA_BYTES).await?;
        serde_json::from_slice(&body).map_err(|error| {
            ManagerError::GitHub(format!("GitHub returned unexpected release JSON: {error}"))
        })
    }

    /// Fetch one release by tag, e.g. `v0.2.0`.
    pub async fn release_by_tag(&self, tag: &str) -> ManagerResult<GitHubRelease> {
        if tag.trim().is_empty() {
            return Err(ManagerError::GitHub(
                "a release tag is required".to_string(),
            ));
        }
        let url = self.api_url(&format!("releases/tags/{}", tag.trim()));
        let response = self.get(&url).await.map_err(|error| {
            if error.to_string().contains("HTTP 404") {
                ManagerError::ReleaseNotFound(tag.to_string())
            } else {
                error
            }
        })?;
        let body = read_limited(response, MAX_METADATA_BYTES).await?;
        serde_json::from_slice(&body).map_err(|error| {
            ManagerError::GitHub(format!("GitHub returned unexpected release JSON: {error}"))
        })
    }

    /// Download a small release asset (manifest or its signature).
    pub async fn download_asset(&self, asset: &GitHubAsset) -> ManagerResult<Vec<u8>> {
        require_https(&asset.browser_download_url, "release asset")?;
        let response = self.get(&asset.browser_download_url).await?;
        read_limited(response, MAX_METADATA_BYTES).await
    }

    async fn get(&self, url: &str) -> ManagerResult<Response> {
        let mut request = self
            .http
            .get(url)
            .header(ACCEPT, "application/vnd.github+json")
            .header(USER_AGENT, user_agent())
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = &self.token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }

        let response = request.send().await.map_err(|error| {
            if error.is_timeout() {
                ManagerError::Network(format!("request to {url} timed out"))
            } else if error.is_connect() {
                ManagerError::Network(format!("cannot reach {url}: {error}"))
            } else {
                ManagerError::Network(format!("request to {url} failed: {error}"))
            }
        })?;

        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
            let remaining = response
                .headers()
                .get("x-ratelimit-remaining")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok());
            if remaining == Some(0) {
                let retry_after = response
                    .headers()
                    .get("x-ratelimit-reset")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<i64>().ok())
                    .and_then(|seconds| Utc.timestamp_opt(seconds, 0).single())
                    .map(describe_reset)
                    .unwrap_or_else(|| "an unknown time".to_string());
                return Err(ManagerError::RateLimited { retry_after });
            }
        }

        let message = read_limited(response, 8 * 1024)
            .await
            .ok()
            .and_then(|body| String::from_utf8(body).ok())
            .map(|text| text.trim().chars().take(200).collect::<String>())
            .unwrap_or_default();
        Err(ManagerError::GitHub(format!(
            "HTTP {status} for {url}{}{}",
            if message.is_empty() { "" } else { ": " },
            message
        )))
    }
}

fn describe_reset(reset: DateTime<Utc>) -> String {
    let now = Utc::now();
    if reset <= now {
        return "now".to_string();
    }
    format!(
        "{reset} (about {} minute(s))",
        (reset - now).num_minutes().max(1)
    )
}

fn token_from_env() -> Option<String> {
    TOKEN_ENV.iter().find_map(|name| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    })
}

/// GitHub rejects requests without a user agent.
pub fn user_agent() -> String {
    format!("RequestLab/{}", crate::VERSION)
}

async fn read_limited(mut response: Response, limit: u64) -> ManagerResult<Vec<u8>> {
    if let Some(length) = response.content_length() {
        if length > limit {
            return Err(ManagerError::GitHub(format!(
                "response is {length} bytes, larger than the {limit} byte limit"
            )));
        }
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| ManagerError::Network(format!("download interrupted: {error}")))?
    {
        if body.len() as u64 + chunk.len() as u64 > limit {
            return Err(ManagerError::GitHub(format!(
                "response exceeded the {limit} byte limit"
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_point_at_the_requestlab_repository() {
        let client = GitHubClient::new().unwrap();
        assert_eq!(client.repository(), "Ahaduzzamankhan/Request-lab");
        assert!(!client.is_authenticated());
    }

    #[test]
    fn base_resource_path_and_token_are_configurable() {
        let client = GitHubClient::new()
            .unwrap()
            .with_base("http://127.0.0.1:8080/")
            .with_token("token");
        assert_eq!(client.base(), "http://127.0.0.1:8080");
        assert!(client.is_authenticated());
        assert_eq!(
            client.api_url("releases/latest"),
            "http://127.0.0.1:8080/repos/Ahaduzzamankhan/Request-lab/releases/latest"
        );
    }

    #[test]
    fn user_agent_names_the_engine_version() {
        assert_eq!(user_agent(), format!("RequestLab/{}", crate::VERSION));
    }
}
