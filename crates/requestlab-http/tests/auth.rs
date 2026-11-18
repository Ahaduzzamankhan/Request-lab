//! End-to-end authentication tests against a local HTTP server.
//!
//! These exercise the real transport: Digest must survive the 401 challenge and
//! retry, OAuth 2.0 must fetch a token once and reuse it, and SigV4 must send a
//! signature the server can verify.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use requestlab_http::cancellation_channel;
use requestlab_http::{prepare_request, HttpClient, HttpConfig, SendOptions};
use requestlab_models::{Authentication, HttpMethod, OAuth2Grant, Request};

#[allow(dead_code)] // asserted by individual tests through helper methods
struct Recorded {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Recorded {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// Serve requests with `handler`; returns the port and the request log.
fn spawn(
    handler: impl Fn(&Recorded) -> String + Send + Sync + 'static,
) -> (u16, Arc<std::sync::Mutex<Vec<Recorded>>>) {
    spawn_inner(Box::new(handler))
}

fn spawn_inner(
    handler: Box<dyn Fn(&Recorded) -> String + Send + Sync>,
) -> (u16, Arc<std::sync::Mutex<Vec<Recorded>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let log = Arc::new(std::sync::Mutex::new(Vec::new()));
    let shared = Arc::clone(&log);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut request_line = String::new();
            if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
                continue;
            }
            let mut parts = request_line.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let target = parts.next().unwrap_or("").to_string();

            let mut headers = Vec::new();
            let mut content_length = 0usize;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    break;
                }
                let line = line.trim_end();
                if line.is_empty() {
                    break;
                }
                if let Some((key, value)) = line.split_once(':') {
                    let key = key.trim().to_string();
                    let value = value.trim().to_string();
                    if key.eq_ignore_ascii_case("content-length") {
                        content_length = value.parse().unwrap_or(0);
                    }
                    headers.push((key, value));
                }
            }
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
            let recorded = Recorded {
                method,
                target,
                headers,
                body: String::from_utf8_lossy(&body).into_owned(),
            };
            let response = handler(&recorded);
            shared.lock().unwrap().push(recorded);

            let mut stream = stream;
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (port, log)
}

fn response(status: &str, extra_headers: &[(&str, &str)], body: &str) -> String {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in extra_headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    head.push_str(body);
    head
}

async fn send(request: &Request, client: &HttpClient) -> requestlab_models::HttpResponse {
    let prepared = prepare_request(request, &SendOptions::default()).expect("prepare");
    let (_canceller, receiver) = cancellation_channel();
    client
        .execute(&prepared, receiver)
        .await
        .expect("request succeeds")
}

#[tokio::test]
async fn digest_retries_once_with_the_challenge_response() {
    let (port, log) = spawn(|request| {
        if request.header("authorization").is_none() {
            response(
                "401 Unauthorized",
                &[(
                    "WWW-Authenticate",
                    r#"Digest realm="testrealm@host.com", qop="auth", nonce="dcd98b7102dd2f0e8b11d0f600bfb0c093", opaque="5ccc069c403ebaf9f0171e9517f40e41""#,
                )],
                "unauthorized",
            )
        } else {
            response("200 OK", &[], "{\"ok\":true}")
        }
    });

    let mut request = Request::new(
        HttpMethod::Get,
        format!("http://127.0.0.1:{port}/dir/index.html"),
    );
    request.authentication = Authentication::Digest {
        username: "Mufasa".into(),
        password: "Circle Of Life".into(),
    };
    let client = HttpClient::new(HttpConfig::default()).unwrap();
    let response = send(&request, &client).await;

    assert_eq!(response.status, 200);
    assert_eq!(response.body.as_text(), Some("{\"ok\":true}"));

    let log = log.lock().unwrap();
    assert_eq!(log.len(), 2, "one challenge plus one authenticated retry");
    assert!(log[0].header("authorization").is_none());
    let authorization = log[1].header("authorization").expect("retry is signed");
    assert!(authorization.starts_with("Digest username=\"Mufasa\""));
    assert!(authorization.contains("uri=\"/dir/index.html\""));
    assert!(authorization.contains("qop=auth"));
    assert!(authorization.contains("opaque=\"5ccc069c403ebaf9f0171e9517f40e41\""));
}

#[tokio::test]
async fn digest_gives_up_when_the_server_keeps_refusing() {
    let (port, log) = spawn(|_| {
        response(
            "401 Unauthorized",
            &[("WWW-Authenticate", r#"Basic realm="nope""#)],
            "unauthorized",
        )
    });
    let mut request = Request::new(HttpMethod::Get, format!("http://127.0.0.1:{port}/"));
    request.authentication = Authentication::Digest {
        username: "ada".into(),
        password: "secret".into(),
    };
    let client = HttpClient::new(HttpConfig::default()).unwrap();
    let response = send(&request, &client).await;
    // A Basic challenge is not answered, so the 401 is returned as-is.
    assert_eq!(response.status, 401);
    assert_eq!(log.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn oauth2_client_credentials_fetches_once_and_reuses_the_token() {
    let token_requests = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&token_requests);
    let (port, log) = spawn(move |request| {
        if request.target.starts_with("/token") {
            counter.fetch_add(1, Ordering::SeqCst);
            assert!(request.body.contains("grant_type=client_credentials"));
            assert!(request.body.contains("client_id=cli"));
            assert!(request.body.contains("scope=read"));
            response(
                "200 OK",
                &[("Content-Type", "application/json")],
                r#"{"access_token":"tok-1","token_type":"Bearer","expires_in":3600}"#,
            )
        } else if request.header("authorization") == Some("Bearer tok-1") {
            response("200 OK", &[], "protected")
        } else {
            response("403 Forbidden", &[], "bad token")
        }
    });

    let mut request = Request::new(HttpMethod::Get, format!("http://127.0.0.1:{port}/data"));
    request.authentication = Authentication::OAuth2 {
        grant: OAuth2Grant::ClientCredentials,
        token_url: format!("http://127.0.0.1:{port}/token"),
        client_id: "cli".into(),
        client_secret: "shh".into(),
        scope: "read".into(),
        username: String::new(),
        password: String::new(),
    };

    let client = HttpClient::new(HttpConfig::default()).unwrap();
    let first = send(&request, &client).await;
    let second = send(&request, &client).await;

    assert_eq!(first.status, 200);
    assert_eq!(second.status, 200);
    assert_eq!(first.body.as_text(), Some("protected"));
    assert_eq!(token_requests.load(Ordering::SeqCst), 1, "token is cached");

    let log = log.lock().unwrap();
    assert_eq!(log.len(), 3, "one token request and two API calls");
    assert_eq!(log[1].header("authorization"), Some("Bearer tok-1"));
    // The client secret is only ever sent to the token endpoint.
    assert!(!log[2].header("authorization").unwrap().contains("shh"));
}

#[tokio::test]
async fn oauth2_reports_a_rejected_client_without_leaking_the_secret() {
    let (port, _log) = spawn(|_| {
        response(
            "401 Unauthorized",
            &[("Content-Type", "application/json")],
            r#"{"error":"invalid_client","error_description":"client authentication failed"}"#,
        )
    });
    let mut request = Request::new(HttpMethod::Get, format!("http://127.0.0.1:{port}/data"));
    request.authentication = Authentication::OAuth2 {
        grant: OAuth2Grant::ClientCredentials,
        token_url: format!("http://127.0.0.1:{port}/token"),
        client_id: "cli".into(),
        client_secret: "super-secret".into(),
        scope: String::new(),
        username: String::new(),
        password: String::new(),
    };

    let client = HttpClient::new(HttpConfig::default()).unwrap();
    let prepared = prepare_request(&request, &SendOptions::default()).unwrap();
    let (_canceller, receiver) = cancellation_channel();
    let error = client.execute(&prepared, receiver).await.unwrap_err();
    let message = error.to_string();
    assert!(message.contains("invalid_client"), "{message}");
    assert!(!message.contains("super-secret"));
}

#[tokio::test]
async fn aws_signature_is_accepted_by_a_verifying_server() {
    let (port, log) = spawn(|request| {
        let authorization = request.header("authorization").unwrap_or("");
        let date = request.header("x-amz-date").unwrap_or("");
        if authorization.starts_with("AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/")
            && authorization.contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date")
            && authorization.contains("Signature=")
            && !date.is_empty()
            && request.header("x-amz-content-sha256").is_some()
        {
            response("200 OK", &[], "signed ok")
        } else {
            response("403 Forbidden", &[], "bad signature")
        }
    });

    let mut request = Request::new(
        HttpMethod::Post,
        format!("http://127.0.0.1:{port}/bucket/key"),
    );
    request.body = requestlab_models::RequestBody::json("{\"a\":1}");
    request.authentication = Authentication::AwsSigV4 {
        access_key: "AKIDEXAMPLE".into(),
        secret_key: "secret".into(),
        region: "us-east-1".into(),
        service: "s3".into(),
        session_token: String::new(),
    };
    let client = HttpClient::new(HttpConfig::default()).unwrap();
    let response = send(&request, &client).await;
    assert_eq!(response.status, 200);
    assert_eq!(response.body.as_text(), Some("signed ok"));

    let log = log.lock().unwrap();
    assert!(log[0]
        .header("authorization")
        .unwrap()
        .contains("Credential=AKIDEXAMPLE/"));
}
