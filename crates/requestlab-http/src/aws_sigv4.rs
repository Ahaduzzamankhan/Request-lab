//! AWS Signature Version 4 request signing.
//!
//! Implements the documented algorithm: canonical request, string to sign,
//! derived signing key and the final `Authorization` header. Signing is
//! deterministic, so it is tested against AWS's published example vectors.
//!
//! Only header-based signing is supported, which is what every AWS and
//! S3-compatible service accepts.

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// Long-lived or temporary AWS credentials plus the signing scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsCredentials {
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
    pub service: String,
    pub session_token: String,
}

/// Headers that must be added to the request, in the order given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRequest {
    pub headers: Vec<(String, String)>,
    /// The canonical request, exposed for debugging and tests.
    pub canonical_request: String,
    pub string_to_sign: String,
    pub signature: String,
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

fn sha256_hex(input: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input);
    hex(&hasher.finalize())
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Percent-encode a string the way SigV4 requires: uppercase hex, everything
/// except unreserved characters, and `/` preserved when `encode_slash` is false.
fn uri_encode(input: &str, encode_slash: bool) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'.' | b'_' | b'~')
            || (*byte == b'/' && !encode_slash);
        if keep {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Decode percent escapes so re-encoding is stable (`%2f` and `%2F` differ).
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
            if let Ok(value) = u8::from_str_radix(hex, 16) {
                out.push(value);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn canonical_query(url: &url::Url) -> String {
    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| (uri_encode(&key, true), uri_encode(&value, true)))
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// Sign a request.
///
/// `body` is the exact payload that will be sent; its SHA-256 is part of the
/// signature. `now` is the signing time and must be UTC.
pub fn sign(
    method: &str,
    url: &url::Url,
    body: &[u8],
    credentials: &AwsCredentials,
    now: DateTime<Utc>,
) -> SignedRequest {
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let datestamp = now.format("%Y%m%d").to_string();
    let host = match (url.host_str(), url.port()) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_string(),
        (None, _) => String::new(),
    };
    let payload_hash = sha256_hex(body);

    let mut signed_headers: Vec<(String, String)> = vec![
        ("host".to_string(), host.clone()),
        ("x-amz-content-sha256".to_string(), payload_hash.clone()),
        ("x-amz-date".to_string(), amz_date.clone()),
    ];
    if !credentials.session_token.trim().is_empty() {
        signed_headers.push((
            "x-amz-security-token".to_string(),
            credentials.session_token.clone(),
        ));
    }
    signed_headers.sort_by(|a, b| a.0.cmp(&b.0));

    let canonical_headers: String = signed_headers
        .iter()
        .map(|(name, value)| format!("{name}:{}\n", value.trim()))
        .collect();
    let signed_header_names = signed_headers
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(";");

    let canonical_uri = {
        let decoded = percent_decode(url.path());
        let path = if decoded.is_empty() { "/" } else { &decoded };
        uri_encode(path, false)
    };

    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{}\n{canonical_headers}\n{signed_header_names}\n{payload_hash}",
        canonical_query(url)
    );

    let scope = format!(
        "{datestamp}/{}/{}/aws4_request",
        credentials.region, credentials.service
    );
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );

    let date_key = hmac(
        format!("AWS4{}", credentials.secret_key).as_bytes(),
        &datestamp,
    );
    let region_key = hmac(&date_key, &credentials.region);
    let service_key = hmac(&region_key, &credentials.service);
    let signing_key = hmac(&service_key, "aws4_request");
    let signature = hex(&hmac(&signing_key, &string_to_sign));

    let mut headers: Vec<(String, String)> = signed_headers
        .into_iter()
        .map(|(name, value)| {
            let display = match name.as_str() {
                "host" => "Host".to_string(),
                "x-amz-content-sha256" => "x-amz-content-sha256".to_string(),
                "x-amz-date" => "x-amz-date".to_string(),
                other => other.to_string(),
            };
            (display, value)
        })
        .collect();
    headers.push((
        "Authorization".to_string(),
        format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_header_names}, Signature={signature}",
            credentials.access_key
        ),
    ));

    SignedRequest {
        headers,
        canonical_request,
        string_to_sign,
        signature,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> AwsCredentials {
        AwsCredentials {
            access_key: "AKIDEXAMPLE".into(),
            secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
            region: "us-east-1".into(),
            service: "service".into(),
            session_token: String::new(),
        }
    }

    /// AWS `aws-sig-v4-test-suite` — `get-vanilla`, extended with the
    /// `x-amz-content-sha256` header this client always signs. The expected
    /// canonical request and signature were verified with an independent
    /// HMAC-SHA256 implementation.
    #[test]
    fn matches_the_aws_get_vanilla_vector() {
        let url = url::Url::parse("https://example.amazonaws.com/").unwrap();
        let now = DateTime::parse_from_rfc3339("2015-08-30T12:36:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let signed = sign("GET", &url, b"", &credentials(), now);

        assert_eq!(
            signed.canonical_request,
            "GET\n/\n\nhost:example.amazonaws.com\nx-amz-content-sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\nx-amz-date:20150830T123600Z\n\nhost;x-amz-content-sha256;x-amz-date\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            signed.string_to_sign,
            "AWS4-HMAC-SHA256\n20150830T123600Z\n20150830/us-east-1/service/aws4_request\nbd2af82b09d2569ab8594ef6bcc1638c8675cb753915d0f401b2f40ecde6f823"
        );
        assert_eq!(
            signed.signature,
            "726c5c4879a6b4ccbbd3b24edbd6b8826d34f87450fbbf4e85546fc7ba9c1642"
        );
        let authorization = signed
            .headers
            .iter()
            .find(|(name, _)| name == "Authorization")
            .map(|(_, value)| value.as_str())
            .unwrap();
        assert!(authorization.starts_with(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature="
        ));
        assert!(authorization.contains("x-amz-date"));
    }

    #[test]
    fn signing_is_deterministic_and_covers_the_body() {
        let url = url::Url::parse("https://s3.us-east-1.amazonaws.com/bucket/key.txt").unwrap();
        let now = DateTime::parse_from_rfc3339("2024-05-01T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let first = sign("PUT", &url, b"hello", &credentials(), now);
        let second = sign("PUT", &url, b"hello", &credentials(), now);
        assert_eq!(first.signature, second.signature);

        let other_body = sign("PUT", &url, b"hello!", &credentials(), now);
        assert_ne!(first.signature, other_body.signature);
        // The payload hash header matches what was signed.
        let hash = other_body
            .headers
            .iter()
            .find(|(name, _)| name == "x-amz-content-sha256")
            .unwrap()
            .1
            .clone();
        assert_eq!(hash, sha256_hex(b"hello!"));
    }

    #[test]
    fn session_tokens_are_signed_and_sent() {
        let mut credentials = credentials();
        credentials.session_token = "session-token".into();
        let url = url::Url::parse("https://example.amazonaws.com/").unwrap();
        let now = DateTime::parse_from_rfc3339("2015-08-30T12:36:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let signed = sign("GET", &url, b"", &credentials, now);
        let authorization = signed
            .headers
            .iter()
            .find(|(name, _)| name == "Authorization")
            .unwrap()
            .1
            .clone();
        assert!(authorization.contains("x-amz-security-token"));
        assert!(signed
            .headers
            .iter()
            .any(|(name, value)| name == "x-amz-security-token" && value == "session-token"));
    }

    #[test]
    fn query_parameters_are_sorted_and_encoded() {
        let url = url::Url::parse("https://example.amazonaws.com/?b=2&a=hello%20world").unwrap();
        let now = DateTime::parse_from_rfc3339("2015-08-30T12:36:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let signed = sign("GET", &url, b"", &credentials(), now);
        assert!(signed.canonical_request.contains("\na=hello%20world&b=2\n"));
    }

    #[test]
    fn ports_are_part_of_the_host_header() {
        let url = url::Url::parse("http://localhost:9000/bucket").unwrap();
        let now = DateTime::parse_from_rfc3339("2015-08-30T12:36:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let signed = sign("GET", &url, b"", &credentials(), now);
        assert!(signed.canonical_request.contains("host:localhost:9000\n"));
    }
}
