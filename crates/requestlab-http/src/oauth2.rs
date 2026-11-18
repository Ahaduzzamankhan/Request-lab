//! OAuth 2.0 token acquisition for the client credentials and password grants.
//!
//! Tokens are fetched from the configured token endpoint, cached in memory until
//! shortly before they expire, and never written to disk or to history. The
//! interactive authorization-code flow belongs to the UI and is out of scope
//! here; this module covers the flows that can complete without a browser.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use requestlab_models::{Authentication, OAuth2Grant};
use tokio::sync::Mutex;

use crate::errors::HttpError;

/// A token response from an OAuth 2.0 token endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub scope: Option<String>,
}

/// The form body sent to the token endpoint.
pub fn form_fields(auth: &Authentication) -> Option<Vec<(String, String)>> {
    let Authentication::OAuth2 {
        grant,
        client_id,
        client_secret,
        scope,
        username,
        password,
        ..
    } = auth
    else {
        return None;
    };
    let mut fields = vec![
        ("grant_type".to_string(), grant.grant_type().to_string()),
        ("client_id".to_string(), client_id.clone()),
    ];
    if !client_secret.is_empty() {
        fields.push(("client_secret".to_string(), client_secret.clone()));
    }
    if !scope.trim().is_empty() {
        fields.push(("scope".to_string(), scope.trim().to_string()));
    }
    if *grant == OAuth2Grant::Password {
        fields.push(("username".to_string(), username.clone()));
        fields.push(("password".to_string(), password.clone()));
    }
    Some(fields)
}

/// Parse a token endpoint response body.
///
/// Error responses are turned into a message that names the OAuth error code
/// but never echoes client credentials.
pub fn parse_token_response(body: &str) -> Result<Token, HttpError> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|err| HttpError::Auth(format!("the token endpoint did not return JSON: {err}")))?;
    if let Some(error) = value.get("error").and_then(|error| error.as_str()) {
        let description = value
            .get("error_description")
            .and_then(|description| description.as_str())
            .unwrap_or("");
        let message = if description.is_empty() {
            format!("the token endpoint rejected the request: {error}")
        } else {
            format!("the token endpoint rejected the request: {error} ({description})")
        };
        return Err(HttpError::Auth(message));
    }
    let access_token = value
        .get("access_token")
        .and_then(|token| token.as_str())
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            HttpError::Auth("the token endpoint response has no access_token".to_string())
        })?;
    Ok(Token {
        access_token: access_token.to_string(),
        token_type: value
            .get("token_type")
            .and_then(|kind| kind.as_str())
            .unwrap_or("Bearer")
            .to_string(),
        expires_in: value.get("expires_in").and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        }),
        scope: value
            .get("scope")
            .and_then(|scope| scope.as_str())
            .map(str::to_string),
    })
}

/// Round down the lifetime so a token is refreshed before it expires.
fn refresh_margin(expires_in: Option<u64>) -> Option<Duration> {
    expires_in.map(|seconds| Duration::from_secs(seconds.saturating_sub(30)))
}

/// Identity of a token: two requests with the same configuration share it.
pub fn cache_key(auth: &Authentication) -> Option<String> {
    let Authentication::OAuth2 {
        grant,
        token_url,
        client_id,
        scope,
        username,
        ..
    } = auth
    else {
        return None;
    };
    Some(format!(
        "{}|{}|{}|{}|{}|{}",
        token_url.trim(),
        client_id.trim(),
        grant.grant_type(),
        scope.trim(),
        username.trim(),
        // The secret is hashed into the key so a rotated secret never reuses a
        // token, without keeping a second copy of it around.
        match auth {
            Authentication::OAuth2 { client_secret, .. } => {
                format!("{:x}", fnv1a(client_secret.as_bytes()))
            }
            _ => String::new(),
        }
    ))
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

struct CachedToken {
    token: Token,
    expires_at: Option<Instant>,
}

impl CachedToken {
    fn is_fresh(&self) -> bool {
        self.expires_at
            .map(|expires_at| Instant::now() < expires_at)
            .unwrap_or(true)
    }
}

/// In-memory token cache, shared by every request the app sends.
#[derive(Default)]
pub struct TokenCache {
    entries: Mutex<HashMap<String, CachedToken>>,
}

impl std::fmt::Debug for TokenCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print token values, even when the cache is logged.
        f.write_str("TokenCache")
    }
}

impl TokenCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop cached tokens, for example after the user edits credentials.
    pub async fn clear(&self) {
        self.entries.lock().await.clear();
    }

    /// Number of cached tokens, for tests and diagnostics.
    pub async fn len(&self) -> usize {
        self.entries.lock().await.len()
    }

    /// Whether no token is cached.
    pub async fn is_empty(&self) -> bool {
        self.entries.lock().await.is_empty()
    }

    /// Return a usable access token, fetching one when needed.
    pub async fn access_token(
        &self,
        client: &reqwest::Client,
        auth: &Authentication,
        force_refresh: bool,
    ) -> Result<String, HttpError> {
        let key = cache_key(auth)
            .ok_or_else(|| HttpError::Auth("OAuth 2.0 settings are incomplete".to_string()))?;

        if !force_refresh {
            let entries = self.entries.lock().await;
            if let Some(cached) = entries.get(&key) {
                if cached.is_fresh() {
                    return Ok(cached.token.access_token.clone());
                }
            }
        }

        let token = request_token(client, auth).await?;
        let mut entries = self.entries.lock().await;
        entries.insert(
            key,
            CachedToken {
                expires_at: refresh_margin(token.expires_in).map(|ttl| Instant::now() + ttl),
                token: token.clone(),
            },
        );
        Ok(token.access_token)
    }
}

/// POST the configured form to the token endpoint and parse the reply.
pub async fn request_token(
    client: &reqwest::Client,
    auth: &Authentication,
) -> Result<Token, HttpError> {
    let Authentication::OAuth2 {
        token_url,
        client_id,
        ..
    } = auth
    else {
        return Err(HttpError::Auth(
            "this request does not use OAuth 2.0".to_string(),
        ));
    };
    let fields = form_fields(auth)
        .ok_or_else(|| HttpError::Auth("OAuth 2.0 settings are incomplete".to_string()))?;

    let request = client
        .post(token_url.trim())
        .header("Accept", "application/json")
        .form(&fields);
    let response = request.send().await.map_err(HttpError::from_reqwest)?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| HttpError::Auth(format!("could not read the token response: {err}")))?;

    if !status.is_success() {
        // Include the server's error code, but never the request body.
        let detail = parse_token_response(&body)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_else(|| format!("the token endpoint returned HTTP {status}"));
        return Err(HttpError::Auth(format!(
            "client '{client_id}' could not obtain a token: {detail}"
        )));
    }
    parse_token_response(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_credentials() -> Authentication {
        Authentication::OAuth2 {
            grant: OAuth2Grant::ClientCredentials,
            token_url: "https://auth.example.com/token".into(),
            client_id: "client".into(),
            client_secret: "secret".into(),
            scope: "read write".into(),
            username: String::new(),
            password: String::new(),
        }
    }

    #[test]
    fn client_credentials_form() {
        let fields = form_fields(&client_credentials()).unwrap();
        assert_eq!(
            fields,
            vec![
                ("grant_type".to_string(), "client_credentials".to_string()),
                ("client_id".to_string(), "client".to_string()),
                ("client_secret".to_string(), "secret".to_string()),
                ("scope".to_string(), "read write".to_string()),
            ]
        );
        assert!(form_fields(&Authentication::None).is_none());
    }

    #[test]
    fn password_grant_adds_user_and_password() {
        let auth = Authentication::OAuth2 {
            grant: OAuth2Grant::Password,
            token_url: "https://auth.example.com/token".into(),
            client_id: "client".into(),
            client_secret: "secret".into(),
            scope: String::new(),
            username: "ada".into(),
            password: "pw".into(),
        };
        let fields = form_fields(&auth).unwrap();
        assert!(fields.contains(&("grant_type".to_string(), "password".to_string())));
        assert!(fields.contains(&("username".to_string(), "ada".to_string())));
        assert!(fields.contains(&("password".to_string(), "pw".to_string())));
    }

    #[test]
    fn parses_token_responses() {
        let token = parse_token_response(
            r#"{"access_token":"abc","token_type":"Bearer","expires_in":3600,"scope":"read"}"#,
        )
        .unwrap();
        assert_eq!(token.access_token, "abc");
        assert_eq!(token.expires_in, Some(3600));
        assert_eq!(token.scope.as_deref(), Some("read"));

        // expires_in as a string is tolerated.
        let token = parse_token_response(r#"{"access_token":"a","expires_in":"60"}"#).unwrap();
        assert_eq!(token.expires_in, Some(60));
        assert_eq!(token.token_type, "Bearer");
    }

    #[test]
    fn reports_oauth_errors_without_leaking_credentials() {
        let error = parse_token_response(
            r#"{"error":"invalid_client","error_description":"client authentication failed"}"#,
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("invalid_client"));
        assert!(!message.contains("secret"));

        assert!(parse_token_response("<html>oops</html>").is_err());
        assert!(parse_token_response("{}").is_err());
    }

    #[test]
    fn cache_key_ignores_secret_changes_only_when_they_differ() {
        let key = cache_key(&client_credentials()).unwrap();
        assert!(key.contains("https://auth.example.com/token"));
        let mut rotated = client_credentials();
        if let Authentication::OAuth2 { client_secret, .. } = &mut rotated {
            *client_secret = "other".into();
        }
        assert_ne!(cache_key(&rotated).unwrap(), key);
        assert_eq!(cache_key(&Authentication::None), None);
    }

    #[test]
    fn refresh_margin_leaves_room_for_clock_skew() {
        assert_eq!(refresh_margin(Some(60)), Some(Duration::from_secs(30)));
        assert_eq!(refresh_margin(Some(10)), Some(Duration::from_secs(0)));
        assert_eq!(refresh_margin(None), None);
    }
}
