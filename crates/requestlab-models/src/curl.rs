//! cURL interoperability: parse a cURL command into a [`Request`] and render a
//! [`Request`] back into a cURL command.
//!
//! Only options that map onto a RequestLab request are accepted. Anything else is
//! an explicit error rather than a silent drop, because a request that quietly
//! loses a header, a body or a certificate option is worse than a command that
//! refuses to import.

use crate::request::{
    ApiKeyLocation, Authentication, HeaderEntry, HttpMethod, QueryParameter, Request, RequestBody,
};

/// Why a cURL command could not be turned into a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurlError {
    message: String,
}

impl CurlError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for CurlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CurlError {}

/// Split a command line into tokens, following POSIX shell quoting rules closely
/// enough for pasted cURL commands: single quotes are literal, double quotes
/// allow backslash escapes, and a backslash outside quotes escapes the next
/// character — including a newline, which makes multi-line commands work.
fn tokenize(input: &str) -> Result<Vec<String>, CurlError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut chars = input.chars().peekable();

    while let Some(character) = chars.next() {
        match character {
            ' ' | '\t' | '\n' | '\r' => {
                if started {
                    tokens.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            '\'' => {
                started = true;
                for quoted in chars.by_ref() {
                    if quoted == '\'' {
                        break;
                    }
                    current.push(quoted);
                }
            }
            '"' => {
                started = true;
                while let Some(quoted) = chars.next() {
                    match quoted {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('\n') => {}
                            Some(escaped) => current.push(escaped),
                            None => return Err(CurlError::new("the command ends inside a quote")),
                        },
                        other => current.push(other),
                    }
                }
            }
            '\\' => match chars.next() {
                Some('\n') => {}
                Some('\\') => {
                    started = true;
                    current.push('\\');
                }
                Some(escaped) => {
                    started = true;
                    current.push(escaped);
                }
                None => return Err(CurlError::new("the command ends with a backslash")),
            },
            other => {
                started = true;
                current.push(other);
            }
        }
    }

    if started {
        tokens.push(current);
    }
    Ok(tokens)
}

fn split_header(value: &str) -> Result<HeaderEntry, CurlError> {
    let (key, value) = value
        .split_once(':')
        .ok_or_else(|| CurlError::new(format!("header '{value}' is not 'Name: value'")))?;
    let key = key.trim();
    if key.is_empty() {
        return Err(CurlError::new("a header has an empty name"));
    }
    Ok(HeaderEntry::new(key, value.trim_start()))
}

/// Parse a cURL command (with or without the leading `curl`) into a request.
pub fn parse(input: &str) -> Result<Request, CurlError> {
    let tokens = tokenize(input)?;
    let mut tokens = tokens.into_iter().peekable();

    match tokens.peek().map(String::as_str) {
        Some("curl") => {
            tokens.next();
        }
        Some(other) => {
            return Err(CurlError::new(format!(
                "'{other}' is not a curl command; it must start with 'curl'"
            )))
        }
        None => return Err(CurlError::new("the command is empty")),
    }

    let mut request = Request::new(HttpMethod::Get, "");
    let mut body_parts: Vec<String> = Vec::new();
    let mut form_fields: Vec<QueryParameter> = Vec::new();
    let mut as_query = false;
    let mut method_set = false;
    let mut digest_requested = false;

    while let Some(token) = tokens.next() {
        let mut value = |flag: &str| -> Result<String, CurlError> {
            tokens
                .next()
                .ok_or_else(|| CurlError::new(format!("{flag} needs a value")))
        };

        match token.as_str() {
            "-X" | "--request" => {
                let method = value("--request")?;
                request.method = HttpMethod::parse(&method).ok_or_else(|| {
                    CurlError::new(format!("'{method}' is not a supported HTTP method"))
                })?;
                method_set = true;
            }
            "-H" | "--header" => request.headers.push(split_header(&value("--header")?)?),
            "--url" => request.url = value("--url")?,
            "-d" | "--data" | "--data-raw" | "--data-ascii" | "--data-binary" => {
                body_parts.push(value("--data")?);
            }
            "--data-urlencode" => {
                let pair = value("--data-urlencode")?;
                let (key, value) = pair
                    .split_once('=')
                    .ok_or_else(|| CurlError::new("--data-urlencode needs name=value"))?;
                form_fields.push(QueryParameter::new(key, value));
            }
            "-F" | "--form" => {
                let pair = value("--form")?;
                let (key, value) = pair
                    .split_once('=')
                    .ok_or_else(|| CurlError::new("--form needs name=value"))?;
                form_fields.push(QueryParameter::new(key, value));
            }
            "-u" | "--user" => {
                let credentials = value("--user")?;
                let (username, password) = credentials
                    .split_once(':')
                    .ok_or_else(|| CurlError::new("--user needs user:password"))?;
                request.authentication = Authentication::Basic {
                    username: username.to_string(),
                    password: password.to_string(),
                };
            }
            "--bearer" => {
                request.authentication = Authentication::Bearer {
                    token: value("--bearer")?,
                };
            }
            "-b" | "--cookie" => request
                .headers
                .push(HeaderEntry::new("Cookie", value("--cookie")?)),
            "-A" | "--user-agent" => request
                .headers
                .push(HeaderEntry::new("User-Agent", value("--user-agent")?)),
            "-e" | "--referer" => request
                .headers
                .push(HeaderEntry::new("Referer", value("--referer")?)),
            "-I" | "--head" => {
                request.method = HttpMethod::Head;
                method_set = true;
            }
            "-G" | "--get" => {
                as_query = true;
                request.method = HttpMethod::Get;
                method_set = true;
            }
            // Options that only affect how curl prints or connects, and that have
            // no equivalent in the saved request.
            "-L"
            | "--location"
            | "--compressed"
            | "-s"
            | "--silent"
            | "-S"
            | "--show-error"
            | "-k"
            | "--insecure"
            | "-v"
            | "--verbose"
            | "-i"
            | "--include"
            | "--http1.1"
            | "--http2"
            | "--http2-prior-knowledge"
            | "--fail"
            | "-f"
            | "--globoff"
            | "-g"
            | "--no-buffer"
            | "-N" => {}
            // Options whose value is irrelevant to the saved request.
            "-o" | "--output" | "-m" | "--max-time" | "--connect-timeout" | "--retry"
            | "--retry-delay" | "--retry-max-time" | "-w" | "--write-out" | "--cacert"
            | "--cert" | "--key" | "-x" | "--proxy" => {
                let _ = value(&token)?;
            }
            "--basic" => {
                request.authentication = Authentication::Basic {
                    username: String::new(),
                    password: String::new(),
                };
            }
            "--digest" => digest_requested = true,
            other if other.starts_with('-') => {
                return Err(CurlError::new(format!(
                    "the curl option '{other}' is not supported; nothing was imported"
                )))
            }
            other => {
                if request.url.is_empty() {
                    request.url = other.to_string();
                } else {
                    return Err(CurlError::new(format!("unexpected second URL '{other}'")));
                }
            }
        }
    }

    if request.url.trim().is_empty() {
        return Err(CurlError::new("the command has no URL"));
    }

    // `--digest --user user:pass` is Digest auth; `--user` alone stays Basic.
    if digest_requested {
        request.authentication = match request.authentication {
            Authentication::Basic { username, password } => {
                Authentication::Digest { username, password }
            }
            other => other,
        };
    }

    // An Authorization header carries the same meaning as the auth fields, so a
    // `Authorization: Bearer <token>` from a real cURL command lands in the auth
    // tab instead of a duplicate header.
    if let Some(index) = request.headers.iter().position(|header| {
        header.enabled
            && header.key.eq_ignore_ascii_case("authorization")
            && header
                .value
                .get(..7)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
    }) {
        let token = request.headers[index].value[7..].trim().to_string();
        request.headers.remove(index);
        if matches!(request.authentication, Authentication::None) {
            request.authentication = Authentication::Bearer { token };
        }
    }

    request.query_params = query_parameters_from_url(&mut request.url);
    if !form_fields.is_empty() {
        if as_query {
            request.query_params.extend(form_fields);
        } else {
            request.body = RequestBody::FormUrlEncoded {
                fields: form_fields,
            };
        }
    } else if !body_parts.is_empty() {
        let body = body_parts.join("&");
        if as_query {
            for pair in body.split('&') {
                let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
                request.query_params.push(QueryParameter::new(key, value));
            }
        } else {
            request.body = if looks_like_json(&body) {
                RequestBody::Json { content: body }
            } else {
                RequestBody::Raw {
                    content: body,
                    content_type: "text/plain".to_string(),
                }
            };
        }
    }

    if !method_set && !matches!(request.body, RequestBody::None) {
        request.method = HttpMethod::Post;
    }

    Ok(request)
}

fn looks_like_json(body: &str) -> bool {
    let trimmed = body.trim_start();
    matches!(trimmed.chars().next(), Some('{') | Some('['))
}

/// Move `?a=b` out of the URL and into `query_params`.
fn query_parameters_from_url(url: &mut String) -> Vec<QueryParameter> {
    let Some((path, query)) = url.split_once('?') else {
        return Vec::new();
    };
    if !query.is_empty() {
        let mut parameters = Vec::new();
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            parameters.push(QueryParameter::new(key, value));
        }
        *url = path.to_string();
        return parameters;
    }
    *url = path.to_string();
    Vec::new()
}

/// Quote a value so the rendered command survives a shell.
fn quote(value: &str) -> String {
    if !value.is_empty()
        && !value
            .chars()
            .any(|c| c.is_whitespace() || "'\"\\$`&|;<>()*?[]#!{}".contains(c))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// The URL with its enabled query parameters appended.
pub fn full_url(request: &Request) -> String {
    let pairs: Vec<String> = request
        .query_params
        .iter()
        .filter(|parameter| parameter.enabled)
        .map(|parameter| format!("{}={}", parameter.key, parameter.value))
        .collect();
    if pairs.is_empty() {
        request.url.clone()
    } else if request.url.contains('?') {
        format!("{}&{}", request.url, pairs.join("&"))
    } else {
        format!("{}?{}", request.url, pairs.join("&"))
    }
}

/// Render a request as a cURL command.
/// Authentication that [`render`] cannot express as cURL options.
///
/// Rendering is lossy for schemes that need a live token or a signature, and
/// callers are expected to surface these strings instead of dropping them.
pub fn render_warnings(request: &Request) -> Vec<&'static str> {
    let mut warnings = Vec::new();
    match &request.authentication {
        Authentication::AwsSigV4 { .. } => {
            warnings.push("AWS Signature v4 credentials are not part of a cURL command");
        }
        Authentication::OAuth2 { .. } => {
            warnings.push("OAuth 2.0 settings are not part of a cURL command");
        }
        _ => {}
    }
    if matches!(request.body, RequestBody::FormUrlEncoded { .. }) {
        // Rendered as --data-urlencode rows, which is exact only for text values.
    }
    warnings
}

pub fn render(request: &Request) -> String {
    let mut parts = vec!["curl".to_string()];
    if request.method != HttpMethod::Get {
        parts.push(format!("--request {}", request.method.as_str()));
    }
    parts.push(quote(&full_url(request)));

    for header in request.headers.iter().filter(|header| header.enabled) {
        parts.push(format!(
            "--header {}",
            quote(&format!("{}: {}", header.key, header.value))
        ));
    }

    match &request.authentication {
        Authentication::None => {}
        Authentication::Basic { username, password } => {
            parts.push(format!(
                "--user {}",
                quote(&format!("{username}:{password}"))
            ));
        }
        Authentication::Bearer { token } => parts.push(format!(
            "--header {}",
            quote(&format!("Authorization: Bearer {token}"))
        )),
        Authentication::Digest { username, password } => {
            parts.push("--digest".to_string());
            parts.push(format!(
                "--user {}",
                quote(&format!("{username}:{password}"))
            ));
        }
        // AWS SigV4 and OAuth 2.0 are negotiated at send time and have no cURL
        // spelling here; `render_warnings` reports the omission to the caller.
        Authentication::AwsSigV4 { .. } | Authentication::OAuth2 { .. } => {}
        Authentication::ApiKey {
            key,
            value,
            location: ApiKeyLocation::Header,
        } => parts.push(format!("--header {}", quote(&format!("{key}: {value}")))),
        Authentication::ApiKey {
            key,
            value,
            location: ApiKeyLocation::Query,
        } => {
            // Already carried by the URL when the request is built, so the
            // rendered command adds it there rather than as a header.
            parts.push(format!("--url {}", quote(&format!("{}:{}", key, value))));
        }
    }

    match &request.body {
        RequestBody::None => {}
        RequestBody::Json { content } | RequestBody::Raw { content, .. } => {
            parts.push(format!("--data {}", quote(content)));
        }
        RequestBody::FormUrlEncoded { fields } => {
            for field in fields.iter().filter(|field| field.enabled) {
                parts.push(format!(
                    "--data-urlencode {}",
                    quote(&format!("{}={}", field.key, field.value))
                ));
            }
        }
    }

    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_simple_get() {
        let request = parse("curl https://api.example.com/users").unwrap();
        assert_eq!(request.method, HttpMethod::Get);
        assert_eq!(request.url, "https://api.example.com/users");
        assert_eq!(request.body, RequestBody::None);
    }

    #[test]
    fn parses_method_headers_and_body() {
        let request = parse(
            "curl -X POST 'https://api.example.com/users' \
             -H 'Content-Type: application/json' \
             -H 'X-Trace: abc' \
             -d '{\"name\":\"Ada\"}'",
        )
        .unwrap();
        assert_eq!(request.method, HttpMethod::Post);
        assert_eq!(request.url, "https://api.example.com/users");
        assert_eq!(request.headers.len(), 2);
        assert_eq!(request.headers[0].key, "Content-Type");
        assert_eq!(request.headers[1].value, "abc");
        match request.body {
            RequestBody::Json { content } => assert_eq!(content, "{\"name\":\"Ada\"}"),
            other => panic!("expected a JSON body, got {other:?}"),
        }
    }

    #[test]
    fn infers_post_for_a_body() {
        let request = parse("curl https://api.example.com -d hello").unwrap();
        assert_eq!(request.method, HttpMethod::Post);
        assert!(matches!(request.body, RequestBody::Raw { .. }));
    }

    #[test]
    fn splits_query_parameters() {
        let request = parse("curl 'https://api.example.com/search?q=rust&page=2'").unwrap();
        assert_eq!(request.url, "https://api.example.com/search");
        assert_eq!(request.query_params.len(), 2);
        assert_eq!(request.query_params[0].key, "q");
        assert_eq!(request.query_params[1].value, "2");
    }

    #[test]
    fn parses_basic_and_bearer_auth() {
        let basic = parse("curl -u user:secret https://api.example.com").unwrap();
        assert_eq!(
            basic.authentication,
            Authentication::Basic {
                username: "user".to_string(),
                password: "secret".to_string()
            }
        );
        let bearer = parse("curl --bearer token-123 https://api.example.com").unwrap();
        assert_eq!(
            bearer.authentication,
            Authentication::Bearer {
                token: "token-123".to_string()
            }
        );
    }

    #[test]
    fn parses_form_fields() {
        let request = parse(
            "curl https://api.example.com -F 'name=Ada' -F 'role=engineer' --data-urlencode 'note=a b'",
        )
        .unwrap();
        match request.body {
            RequestBody::FormUrlEncoded { fields } => {
                assert_eq!(fields.len(), 3);
                assert_eq!(fields[2].value, "a b");
            }
            other => panic!("expected a form body, got {other:?}"),
        }
    }

    #[test]
    fn data_becomes_query_with_get() {
        let request = parse("curl -G https://api.example.com -d 'q=rust'").unwrap();
        assert_eq!(request.method, HttpMethod::Get);
        assert_eq!(request.query_params.len(), 1);
        assert_eq!(request.query_params[0].key, "q");
    }

    #[test]
    fn refuses_unsupported_options() {
        let error = parse("curl --ntlm -u a:b https://api.example.com").unwrap_err();
        assert!(error.message().contains("--ntlm"), "{error}");
    }

    #[test]
    fn digest_credentials_round_trip() {
        let request = parse("curl --digest -u ada:secret https://api.example.com").unwrap();
        assert_eq!(
            request.authentication,
            Authentication::Digest {
                username: "ada".into(),
                password: "secret".into()
            }
        );
        let rendered = render(&request);
        assert!(rendered.contains("--digest"), "{rendered}");
        assert!(rendered.contains("ada:secret"), "{rendered}");
        assert_eq!(
            parse(&rendered).unwrap().authentication,
            request.authentication
        );

        // Without --digest, -u stays Basic.
        let basic = parse("curl -u ada:secret https://api.example.com").unwrap();
        assert!(matches!(basic.authentication, Authentication::Basic { .. }));
    }

    #[test]
    fn refuses_a_command_without_a_url() {
        assert!(parse("curl -X POST").is_err());
    }

    #[test]
    fn renders_and_reparses() {
        let mut request = Request::new(HttpMethod::Put, "https://api.example.com/items");
        request
            .headers
            .push(HeaderEntry::new("Content-Type", "application/json"));
        request.query_params.push(QueryParameter::new("dry", "1"));
        request.body = RequestBody::json("{\"id\":7}");
        request.authentication = Authentication::Bearer {
            token: "abc".to_string(),
        };

        let rendered = render(&request);
        assert!(rendered.contains("--request PUT"), "{rendered}");
        assert!(rendered.contains("--data"), "{rendered}");

        let parsed = parse(&rendered).unwrap();
        assert_eq!(parsed.method, HttpMethod::Put);
        assert_eq!(parsed.url, "https://api.example.com/items");
        assert_eq!(parsed.query_params.len(), 1);
        // Content-Type only: the Authorization header becomes the auth fields.
        assert_eq!(parsed.headers.len(), 1);
        assert_eq!(parsed.authentication, request.authentication);
        assert_eq!(parsed.body, request.body);
    }

    #[test]
    fn quoting_survives_spaces_and_quotes() {
        let request =
            parse("curl 'https://api.example.com/a b' -H 'X-Note: it'\\''s fine'").unwrap();
        assert_eq!(request.url, "https://api.example.com/a b");
        assert_eq!(request.headers[0].value, "it's fine");
        let rendered = render(&request);
        assert_eq!(parse(&rendered).unwrap().headers[0].value, "it's fine");
    }
}
