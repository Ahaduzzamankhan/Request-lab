//! Authentication applied to a prepared request.
//!
//! Credentials are applied at execution time and never merged into the header
//! rows the user edits, which keeps them out of history and out of the UI's
//! request display.

use requestlab_models::{ApiKeyLocation, Authentication};

/// Encode bytes as standard base64 with padding.
///
/// Implemented locally to avoid pulling a dependency in for one small function.
pub fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// The result of applying authentication to a request in flight.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct AppliedAuth {
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
}

/// Compute the headers and query parameters required by `auth`.
pub fn apply(auth: &Authentication) -> AppliedAuth {
    match auth {
        Authentication::None => AppliedAuth::default(),
        Authentication::Basic { username, password } => {
            let token = base64_encode(format!("{username}:{password}").as_bytes());
            AppliedAuth {
                headers: vec![("Authorization".to_string(), format!("Basic {token}"))],
                query: Vec::new(),
            }
        }
        Authentication::Bearer { token } => {
            let token = token.trim();
            if token.is_empty() {
                return AppliedAuth::default();
            }
            AppliedAuth {
                headers: vec![("Authorization".to_string(), format!("Bearer {token}"))],
                query: Vec::new(),
            }
        }
        Authentication::ApiKey {
            key,
            value,
            location,
        } => {
            let key = key.trim();
            if key.is_empty() || value.trim().is_empty() {
                return AppliedAuth::default();
            }
            match location {
                ApiKeyLocation::Header => AppliedAuth {
                    headers: vec![(key.to_string(), value.clone())],
                    query: Vec::new(),
                },
                ApiKeyLocation::Query => AppliedAuth {
                    headers: Vec::new(),
                    query: vec![(key.to_string(), value.clone())],
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        // RFC 4648 test vector
        assert_eq!(
            base64_encode(b"user:password"),
            "dXNlcjpwYXNzd29yZA=="
        );
    }

    #[test]
    fn basic_auth_sets_authorization_header() {
        let applied = apply(&Authentication::Basic {
            username: "user".into(),
            password: "password".into(),
        });
        assert_eq!(
            applied.headers,
            vec![(
                "Authorization".to_string(),
                "Basic dXNlcjpwYXNzd29yZA==".to_string()
            )]
        );
        assert!(applied.query.is_empty());
    }

    #[test]
    fn bearer_auth_trims_token() {
        let applied = apply(&Authentication::Bearer {
            token: "  abc.def  ".into(),
        });
        assert_eq!(applied.headers[0].1, "Bearer abc.def");
    }

    #[test]
    fn api_key_placement() {
        let header = apply(&Authentication::ApiKey {
            key: "X-API-Key".into(),
            value: "k".into(),
            location: ApiKeyLocation::Header,
        });
        assert_eq!(header.headers[0].0, "X-API-Key");
        assert!(header.query.is_empty());

        let query = apply(&Authentication::ApiKey {
            key: "api_key".into(),
            value: "k".into(),
            location: ApiKeyLocation::Query,
        });
        assert!(query.headers.is_empty());
        assert_eq!(query.query[0], ("api_key".to_string(), "k".to_string()));
    }

    #[test]
    fn incomplete_credentials_are_ignored() {
        assert_eq!(
            apply(&Authentication::Bearer {
                token: "  ".into()
            }),
            AppliedAuth::default()
        );
        assert_eq!(
            apply(&Authentication::ApiKey {
                key: String::new(),
                value: "k".into(),
                location: ApiKeyLocation::Header,
            }),
            AppliedAuth::default()
        );
    }
}
