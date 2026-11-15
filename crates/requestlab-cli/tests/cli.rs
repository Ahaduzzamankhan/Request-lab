//! End-to-end CLI tests: a local HTTP server plus a temporary data directory.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use requestlab_cli::args;
use requestlab_cli::commands;

/// A one-endpoint HTTP server that records the last request it received.
struct TestServer {
    port: u16,
    last_request: Arc<std::sync::Mutex<String>>,
}

impl TestServer {
    fn spawn(status: u16, body: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let port = listener.local_addr().unwrap().port();
        let last_request = Arc::new(std::sync::Mutex::new(String::new()));
        let recorder = Arc::clone(&last_request);

        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                handle(stream, status, body, &recorder);
            }
        });

        Self { port, last_request }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.port, path)
    }

    fn last_request(&self) -> String {
        self.last_request.lock().unwrap().clone()
    }
}

fn handle(
    stream: TcpStream,
    status: u16,
    body: &'static str,
    recorder: &Arc<std::sync::Mutex<String>>,
) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut head = String::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap_or(0);
        }
        head.push_str(&line);
        if line == "\r\n" {
            break;
        }
    }
    let mut body_bytes = vec![0u8; content_length];
    if content_length > 0 {
        let _ = reader.read_exact(&mut body_bytes);
    }
    let full = format!("{head}{}", String::from_utf8_lossy(&body_bytes));
    *recorder.lock().unwrap() = full;

    let mut stream = stream;
    let response = format!(
        "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_dir() -> PathBuf {
    let suffix = format!(
        "requestlab-cli-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    );
    let dir = std::env::temp_dir().join(suffix);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

async fn invoke(
    data_dir: &Path,
    argv: &[&str],
) -> Result<commands::CommandOutput, commands::CliError> {
    let mut full: Vec<String> = vec!["--data-dir".to_string(), data_dir.display().to_string()];
    full.extend(argv.iter().map(|arg| arg.to_string()));
    let invocation = args::parse(full).expect("arguments parse");
    commands::run(invocation).await
}

#[tokio::test]
async fn send_get_prints_response_and_records_history() {
    let server = TestServer::spawn(200, "{\"ok\":true}");
    let dir = temp_dir();

    let output = invoke(&dir, &["send", &server.url("/users")])
        .await
        .expect("send succeeds");
    assert_eq!(output.exit_code, 0);
    assert!(output.text.contains("200 OK"), "{}", output.text);
    assert!(output.text.contains("{\"ok\":true}"), "{}", output.text);

    let history = invoke(&dir, &["history", "list"]).await.unwrap();
    assert!(history.text.contains("/users"), "{}", history.text);
    assert!(history.text.contains("200"), "{}", history.text);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn send_posts_json_headers_and_query_parameters() {
    let server = TestServer::spawn(201, "created");
    let dir = temp_dir();

    let output = invoke(
        &dir,
        &[
            "--json",
            "send",
            &server.url("/items?existing=1"),
            "-X",
            "POST",
            "-H",
            "X-Test: yes",
            "-d",
            "{\"name\":\"ada\"}",
            "--no-body",
        ],
    )
    .await
    .unwrap();

    assert_eq!(output.exit_code, 0);
    let parsed: serde_json::Value = serde_json::from_str(&output.text).unwrap();
    assert_eq!(parsed["status"], 201);
    assert_eq!(parsed["body"]["text"], "");

    let recorded = server.last_request();
    assert!(recorded.starts_with("POST /items?existing=1"), "{recorded}");
    assert!(
        recorded.to_ascii_lowercase().contains("x-test: yes"),
        "{recorded}"
    );
    assert!(recorded.contains("{\"name\":\"ada\"}"), "{recorded}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn fail_flag_turns_error_statuses_into_exit_code_22() {
    let server = TestServer::spawn(500, "boom");
    let dir = temp_dir();

    let output = invoke(&dir, &["send", &server.url("/broken"), "--fail"])
        .await
        .unwrap();
    assert_eq!(output.exit_code, 22);
    assert!(output.text.contains("500"), "{}", output.text);

    let ok = invoke(&dir, &["send", &server.url("/broken")])
        .await
        .unwrap();
    assert_eq!(ok.exit_code, 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn invalid_url_is_reported_as_usage_error() {
    let dir = temp_dir();
    let error = invoke(&dir, &["send", "not a url"]).await.unwrap_err();
    assert_eq!(error.exit_code, 2, "{}", error.message);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn collection_create_list_delete_round_trip() {
    let dir = temp_dir();

    let created = invoke(&dir, &["collections", "create", "Payments"])
        .await
        .unwrap();
    assert!(created.text.contains("Payments"), "{}", created.text);

    let listed = invoke(&dir, &["collections", "list"]).await.unwrap();
    assert!(listed.text.contains("Payments"), "{}", listed.text);

    let json = invoke(&dir, &["--json", "collections", "list"])
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json.text).unwrap();
    let id = parsed["collections"][0]["id"].as_str().unwrap().to_string();

    let deleted = invoke(&dir, &["collections", "delete", &id]).await.unwrap();
    assert!(deleted.text.contains("Deleted"), "{}", deleted.text);

    let empty = invoke(&dir, &["collections", "list"]).await.unwrap();
    assert!(empty.text.contains("No collections"), "{}", empty.text);

    let bad = invoke(&dir, &["collections", "delete", "nope"])
        .await
        .unwrap_err();
    assert_eq!(bad.exit_code, 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn environment_lifecycle_and_secret_redaction() {
    let dir = temp_dir();

    invoke(&dir, &["environments", "create", "Development"])
        .await
        .unwrap();
    let used = invoke(&dir, &["environments", "use", "development"])
        .await
        .unwrap();
    assert!(used.text.contains("Development"), "{}", used.text);

    let listed = invoke(&dir, &["environments", "list"]).await.unwrap();
    assert!(listed.text.contains("* Development"), "{}", listed.text);

    let variables = invoke(&dir, &["environments", "variables", "Development"])
        .await
        .unwrap();
    assert!(
        variables.text.contains("no variables"),
        "{}",
        variables.text
    );

    let missing = invoke(&dir, &["environments", "use", "Staging"])
        .await
        .unwrap_err();
    assert_eq!(missing.exit_code, 2);

    let none = invoke(&dir, &["environments", "use", "none"])
        .await
        .unwrap();
    assert!(none.text.contains("No active environment"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn config_round_trip_validates_values() {
    let dir = temp_dir();

    let defaults = invoke(&dir, &["config", "get"]).await.unwrap();
    assert!(defaults.text.contains("timeout = 30s"), "{}", defaults.text);

    invoke(&dir, &["config", "set", "timeout", "45"])
        .await
        .unwrap();
    invoke(&dir, &["config", "set", "theme", "light"])
        .await
        .unwrap();

    let updated = invoke(&dir, &["--json", "config", "get"]).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&updated.text).unwrap();
    assert_eq!(parsed["timeout_seconds"], 45);
    assert_eq!(parsed["theme"], "light");

    assert_eq!(
        invoke(&dir, &["config", "set", "timeout", "0"])
            .await
            .unwrap_err()
            .exit_code,
        2
    );
    assert_eq!(
        invoke(&dir, &["config", "set", "theme", "neon"])
            .await
            .unwrap_err()
            .exit_code,
        2
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn history_can_be_filtered_and_cleared() {
    let ok = TestServer::spawn(200, "fine");
    let dir = temp_dir();

    invoke(&dir, &["send", &ok.url("/ok")]).await.unwrap();
    invoke(&dir, &["send", &ok.url("/ok-two")]).await.unwrap();

    let filtered = invoke(
        &dir,
        &["history", "list", "--status", "2xx", "--limit", "1"],
    )
    .await
    .unwrap();
    assert_eq!(filtered.text.lines().count(), 1, "{}", filtered.text);

    let none = invoke(&dir, &["history", "list", "--status", "5xx"])
        .await
        .unwrap();
    assert!(none.text.contains("No history"), "{}", none.text);

    let cleared = invoke(&dir, &["history", "clear"]).await.unwrap();
    assert!(cleared.text.contains("Cleared 2"), "{}", cleared.text);

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn environment_variables_are_resolved_before_sending() {
    let server = TestServer::spawn(200, "ok");
    let dir = temp_dir();

    // Seed an environment directly through the service so the test focuses on
    // CLI resolution behaviour rather than a missing `variables set` command.
    let service = requestlab_core::AppService::open(Some(dir.clone()))
        .await
        .unwrap();
    let environment = service.environments().create("Local").await.unwrap();
    service
        .environments()
        .save_variables(
            environment.id,
            &[requestlab_models::EnvironmentVariable::new(
                "host",
                format!("127.0.0.1:{}", server.port),
            )],
        )
        .await
        .unwrap();
    drop(service);

    let output = invoke(
        &dir,
        &["send", "http://{{host}}/resolved", "--env", "Local"],
    )
    .await
    .unwrap();
    assert_eq!(output.exit_code, 0);
    assert!(
        server.last_request().starts_with("GET /resolved"),
        "{}",
        server.last_request()
    );

    let missing = invoke(&dir, &["send", "http://{{nope}}/x", "--env", "Local"])
        .await
        .unwrap();
    assert_eq!(missing.exit_code, 1);
    assert!(
        missing.text.to_lowercase().contains("nope"),
        "{}",
        missing.text
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn environment_variables_can_be_set_unset_and_are_redacted() {
    let server = TestServer::spawn(200, "ok");
    let dir = temp_dir();

    invoke(&dir, &["environments", "create", "Local"])
        .await
        .unwrap();
    let host = format!("127.0.0.1:{}", server.port);
    let assignment = format!("base={host}");

    let set = invoke(&dir, &["environments", "set", "Local", &assignment])
        .await
        .unwrap();
    assert!(set.text.contains("base ="), "{}", set.text);

    let secret = invoke(
        &dir,
        &["environments", "set", "Local", "token=abc", "--secret"],
    )
    .await
    .unwrap();
    assert!(secret.text.contains("********"), "{}", secret.text);

    let listed = invoke(&dir, &["--json", "environments", "variables", "Local"])
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&listed.text).unwrap();
    let variables = parsed["variables"].as_array().unwrap();
    assert_eq!(variables.len(), 2);
    assert!(variables.iter().any(|value| value["value"] == "********"));

    // The CLI-set variable is used for real resolution.
    let sent = invoke(&dir, &["send", "http://{{base}}/cli", "--env", "Local"])
        .await
        .unwrap();
    assert_eq!(sent.exit_code, 0);
    assert!(
        server.last_request().starts_with("GET /cli"),
        "{}",
        server.last_request()
    );

    // Re-setting a key updates it instead of duplicating it.
    invoke(&dir, &["environments", "set", "Local", "base=127.0.0.1:1"])
        .await
        .unwrap();
    let listed = invoke(&dir, &["--json", "environments", "variables", "Local"])
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&listed.text).unwrap();
    assert_eq!(parsed["variables"].as_array().unwrap().len(), 2);

    let removed = invoke(&dir, &["environments", "unset", "Local", "token"])
        .await
        .unwrap();
    assert!(removed.text.contains("Removed token"), "{}", removed.text);

    let missing = invoke(&dir, &["environments", "unset", "Local", "token"])
        .await
        .unwrap_err();
    assert_eq!(missing.exit_code, 2);

    let _ = std::fs::remove_dir_all(&dir);
}
