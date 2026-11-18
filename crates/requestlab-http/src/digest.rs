//! HTTP Digest authentication (RFC 7616, with the RFC 2617 MD5 form).
//!
//! Digest is negotiated: the server answers the first request with a
//! `WWW-Authenticate: Digest ...` challenge and the client repeats the request
//! with an `Authorization` header computed from that challenge. The nonce is
//! never stored in the saved request.
//!
//! Supported: `MD5`, `MD5-sess`, `SHA-256`, `SHA-256-sess`, with `qop=auth`
//! (and the legacy no-qop form). `auth-int` is refused because it requires the
//! exact bytes of the body the transport will send.

use md5::Md5;
use sha2::{Digest, Sha256};

/// The hash a challenge asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigestAlgorithm {
    Md5,
    Md5Sess,
    Sha256,
    Sha256Sess,
}

impl DigestAlgorithm {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "MD5" => Some(Self::Md5),
            "MD5-SESS" => Some(Self::Md5Sess),
            "SHA-256" => Some(Self::Sha256),
            "SHA-256-SESS" => Some(Self::Sha256Sess),
            _ => None,
        }
    }

    fn is_session(self) -> bool {
        matches!(self, Self::Md5Sess | Self::Sha256Sess)
    }

    fn hash(self, input: &str) -> String {
        match self {
            Self::Md5 | Self::Md5Sess => {
                let mut hasher = Md5::new();
                hasher.update(input.as_bytes());
                hex(&hasher.finalize())
            }
            Self::Sha256 | Self::Sha256Sess => {
                let mut hasher = Sha256::new();
                hasher.update(input.as_bytes());
                hex(&hasher.finalize())
            }
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    out
}

/// A parsed `WWW-Authenticate: Digest ...` challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestChallenge {
    pub realm: String,
    pub nonce: String,
    pub opaque: Option<String>,
    /// Offered quality-of-protection values, lowercased.
    pub qop: Vec<String>,
    pub algorithm: DigestAlgorithm,
    pub stale: bool,
}

impl DigestChallenge {
    /// Parse a `WWW-Authenticate` header value. Returns `None` when the scheme
    /// is not Digest or the required `nonce` is missing.
    pub fn parse(header: &str) -> Option<Self> {
        let (scheme, rest) = header.trim().split_once(char::is_whitespace)?;
        if !scheme.eq_ignore_ascii_case("digest") {
            return None;
        }
        let mut realm = None;
        let mut nonce = None;
        let mut opaque = None;
        let mut qop = Vec::new();
        let mut algorithm = DigestAlgorithm::Md5;
        let mut stale = false;

        for (key, value) in parse_parameters(rest) {
            match key.to_ascii_lowercase().as_str() {
                "realm" => realm = Some(value.clone()),
                "nonce" => nonce = Some(value.clone()),
                "opaque" => opaque = Some(value.clone()),
                "qop" => {
                    qop = value
                        .split(',')
                        .map(|entry| entry.trim().to_ascii_lowercase())
                        .filter(|entry| !entry.is_empty())
                        .collect()
                }
                "algorithm" => {
                    algorithm = DigestAlgorithm::parse(&value)?;
                }
                "stale" => stale = value.eq_ignore_ascii_case("true"),
                _ => {}
            }
        }

        Some(Self {
            realm: realm.unwrap_or_default(),
            nonce: nonce?,
            opaque,
            qop,
            algorithm,
            stale,
        })
    }

    /// Whether the challenge offers a qop this client can compute.
    pub fn supports_qop(&self) -> bool {
        self.qop.is_empty() || self.qop.iter().any(|qop| qop == "auth")
    }
}

/// Split `key=value, key="quoted, value"` pairs.
fn parse_parameters(input: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut key = String::new();
    let mut value = String::new();
    let mut in_key = true;
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '\\' if in_quotes => {
                if let Some(escaped) = chars.next() {
                    value.push(escaped);
                }
            }
            '=' if in_key && !in_quotes => in_key = false,
            ',' if !in_quotes => {
                if !key.trim().is_empty() {
                    pairs.push((key.trim().to_string(), value.trim().to_string()));
                }
                key.clear();
                value.clear();
                in_key = true;
            }
            other => {
                if in_key {
                    key.push(other);
                } else {
                    value.push(other);
                }
            }
        }
    }
    if !key.trim().is_empty() {
        pairs.push((key.trim().to_string(), value.trim().to_string()));
    }
    pairs
}

/// Compute the `Authorization` header for a challenge.
///
/// `uri` is the request target as sent (path plus query). `cnonce` is supplied
/// by the caller so tests can pin it; production callers pass a random value.
pub fn authorization_header(
    challenge: &DigestChallenge,
    username: &str,
    password: &str,
    method: &str,
    uri: &str,
    cnonce: &str,
    nonce_count: u32,
) -> Option<String> {
    if !challenge.supports_qop() {
        return None;
    }
    let algorithm = challenge.algorithm;
    let mut ha1 = algorithm.hash(&format!("{username}:{}:{password}", challenge.realm));
    if algorithm.is_session() {
        ha1 = algorithm.hash(&format!("{ha1}:{}:{cnonce}", challenge.nonce));
    }
    let ha2 = algorithm.hash(&format!("{method}:{uri}"));

    let qop = if challenge.qop.iter().any(|qop| qop == "auth") {
        Some("auth")
    } else {
        None
    };

    let response = match qop {
        Some(qop) => {
            let nc = format!("{nonce_count:08x}");
            algorithm.hash(&format!(
                "{ha1}:{}:{nc}:{cnonce}:{qop}:{ha2}",
                challenge.nonce
            ))
        }
        None => algorithm.hash(&format!("{ha1}:{}:{ha2}", challenge.nonce)),
    };

    let mut header = format!(
        "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{}\"",
        quote_value(username),
        quote_value(&challenge.realm),
        quote_value(&challenge.nonce),
        quote_value(uri),
        response
    );
    if let Some(qop) = qop {
        header.push_str(&format!(
            ", qop={qop}, nc={:08x}, cnonce=\"{}\"",
            nonce_count,
            quote_value(cnonce)
        ));
    }
    if let Some(opaque) = &challenge.opaque {
        header.push_str(&format!(", opaque=\"{}\"", quote_value(opaque)));
    }
    if algorithm != DigestAlgorithm::Md5 {
        let name = match algorithm {
            DigestAlgorithm::Md5 | DigestAlgorithm::Md5Sess => "MD5",
            DigestAlgorithm::Sha256 => "SHA-256",
            DigestAlgorithm::Sha256Sess => "SHA-256-sess",
        };
        header.push_str(&format!(", algorithm={name}"));
    }
    Some(header)
}

fn quote_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rfc2617_challenge() -> DigestChallenge {
        DigestChallenge::parse(
            r#"Digest realm="testrealm@host.com", qop="auth,auth-int", nonce="dcd98b7102dd2f0e8b11d0f600bfb0c093", opaque="5ccc069c403ebaf9f0171e9517f40e41""#,
        )
        .unwrap()
    }

    #[test]
    fn parses_a_challenge_with_quoted_commas() {
        let challenge = rfc2617_challenge();
        assert_eq!(challenge.realm, "testrealm@host.com");
        assert_eq!(challenge.qop, vec!["auth", "auth-int"]);
        assert_eq!(challenge.algorithm, DigestAlgorithm::Md5);
        assert_eq!(
            challenge.opaque.as_deref(),
            Some("5ccc069c403ebaf9f0171e9517f40e41")
        );
        assert!(challenge.supports_qop());
    }

    #[test]
    fn rejects_non_digest_and_nonce_less_challenges() {
        assert!(DigestChallenge::parse("Basic realm=\"x\"").is_none());
        assert!(DigestChallenge::parse("Digest realm=\"x\"").is_none());
        assert!(DigestChallenge::parse("Digest nonce=\"n\", algorithm=\"unknown\"").is_none());
    }

    #[test]
    fn matches_the_rfc_2617_example() {
        let header = authorization_header(
            &rfc2617_challenge(),
            "Mufasa",
            "Circle Of Life",
            "GET",
            "/dir/index.html",
            "0a4f113b",
            1,
        )
        .unwrap();
        assert!(
            header.contains(r#"response="6629fae49393a05397450978507c4ef1""#),
            "{header}"
        );
        assert!(header.contains("qop=auth"));
        assert!(header.contains("nc=00000001"));
        assert!(header.contains(r#"opaque="5ccc069c403ebaf9f0171e9517f40e41""#));
    }

    #[test]
    fn supports_sha256_challenges() {
        let challenge = DigestChallenge::parse(
            r#"Digest realm="api", nonce="abc", algorithm=SHA-256, qop="auth""#,
        )
        .unwrap();
        assert_eq!(challenge.algorithm, DigestAlgorithm::Sha256);
        let header =
            authorization_header(&challenge, "ada", "secret", "POST", "/v1/items", "feed", 2)
                .unwrap();
        // Deterministic: the same inputs always produce the same digest.
        let again =
            authorization_header(&challenge, "ada", "secret", "POST", "/v1/items", "feed", 2)
                .unwrap();
        assert_eq!(header, again);
        assert!(header.contains(", algorithm=SHA-256"));
        assert!(header.contains("nc=00000002"));
        assert!(header.contains(r#"username="ada""#));
        assert!(header.contains(r#"uri="/v1/items""#));
        // SHA-256 responses are 64 hex characters.
        let start = header.find("response=\"").unwrap() + 10;
        assert_eq!(
            &header[start..start + 64],
            &header[start..start + 64].to_ascii_lowercase()
        );
        assert!(header[start + 64..].starts_with('"'));
    }

    #[test]
    fn refuses_auth_int_only_challenges() {
        let challenge =
            DigestChallenge::parse(r#"Digest realm="x", nonce="n", qop="auth-int""#).unwrap();
        assert!(!challenge.supports_qop());
        assert!(authorization_header(&challenge, "u", "p", "GET", "/", "c", 1).is_none());
    }

    #[test]
    fn quotes_are_escaped() {
        let challenge = DigestChallenge::parse(r#"Digest realm="a\"b", nonce="n""#).unwrap();
        let header = authorization_header(&challenge, "u\"ser", "p", "GET", "/", "c", 1).unwrap();
        assert!(header.contains(r#"username="u\"ser""#));
    }
}
