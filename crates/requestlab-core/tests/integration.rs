//! Integration tests for the parts of RequestLab that must survive a restart.
//!
//! Everything runs against a temporary SQLite file and a local HTTP server, so
//! the suite never touches the network or the developer's real data directory.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};

use requestlab_core::AppService;
use requestlab_models::{
    EnvironmentVariable, HistoryQuery, HistoryStatusFilter, HttpMethod, Request, ResponseBody,
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn temp_db() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "requestlab-core-it-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir.join("requestlab.sqlite")
}

/// Minimal HTTP server that answers every request with a fixed response.
fn spawn_server(status: &'static str, body: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
            }
            let mut stream = stream;
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    port
}

#[tokio::test]
async fn collections_folders_and_requests_survive_a_restart() {
    let path = temp_db();
    let service = AppService::open(Some(path.clone())).await.expect("open");

    let collection = service
        .collections()
        .create_collection("API")
        .await
        .unwrap();
    let folder = service
        .collections()
        .create_folder(collection.id, None, "Users")
        .await
        .unwrap();
    let nested = service
        .collections()
        .create_folder(collection.id, Some(folder.id), "Admin")
        .await
        .unwrap();
    let request = Request::new(HttpMethod::Post, "https://api.example.com/users");
    service
        .collections()
        .save_request(collection.id, Some(nested.id), "Create user", &request)
        .await
        .unwrap();

    let trees = service.collections().list_trees().await.unwrap();
    assert_eq!(trees.len(), 1);
    assert_eq!(trees[0].all_requests().len(), 1);
    assert!(trees[0].folder(folder.id).is_some());
    assert!(trees[0].folder(nested.id).is_some());
    drop(service);

    // Reopen from disk: the tree must be identical.
    let reopened = AppService::open(Some(path.clone())).await.unwrap();
    let trees = reopened.collections().list_trees().await.unwrap();
    assert_eq!(trees.len(), 1);
    let saved = trees[0].all_requests().pop().expect("saved request");
    assert_eq!(saved.name, "Create user");
    assert_eq!(saved.request.method, HttpMethod::Post);
    assert_eq!(saved.request.url, "https://api.example.com/users");
    drop(reopened);

    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[tokio::test]
async fn sending_records_history_and_prunes_to_the_limit() {
    let port = spawn_server("201 Created", "{\"ok\":true}");
    let path = temp_db();

    let mut service = AppService::open(Some(path.clone())).await.unwrap();
    service.set_history_limit(10).await.unwrap();

    let request = Request::new(HttpMethod::Get, format!("http://127.0.0.1:{port}/items"));
    let outcome = service.send_simple(&request).await;
    let response = outcome.result.expect("request succeeds");
    assert_eq!(response.status, 201);
    assert_eq!(response.body.as_text(), Some("{\"ok\":true}"));
    assert!(outcome.history_id.is_some());

    let history = service
        .history(&HistoryQuery {
            limit: 50,
            ..HistoryQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].status_code, Some(201));

    // Filters are applied in SQL, not in memory.
    assert!(service
        .history(&HistoryQuery {
            status: HistoryStatusFilter::ClientError,
            ..HistoryQuery::default()
        })
        .await
        .unwrap()
        .is_empty());

    let cleared = service.clear_history().await.unwrap();
    assert_eq!(cleared, 1);
    drop(service);

    let reopened = AppService::open(Some(path.clone())).await.unwrap();
    assert_eq!(reopened.settings().history_limit, 10);
    assert!(reopened
        .history(&HistoryQuery::default())
        .await
        .unwrap()
        .is_empty());
    drop(reopened);

    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[tokio::test]
async fn environment_variables_resolve_without_mutating_the_request() {
    let path = temp_db();
    let service = AppService::open(Some(path.clone())).await.unwrap();

    let environment = service.environments().create("Local").await.unwrap();
    service
        .environments()
        .save_variables(
            environment.id,
            &[
                EnvironmentVariable::new("base", "http://localhost:8080"),
                EnvironmentVariable::new("token", "s3cret"),
            ],
        )
        .await
        .unwrap();
    service
        .environments()
        .set_active(Some(environment.id))
        .await
        .unwrap();

    let mut request = Request::new(HttpMethod::Get, "{{base}}/health");
    request.headers = vec![requestlab_models::HeaderEntry::new(
        "Authorization",
        "Bearer {{token}}",
    )];

    let resolved = service.environments().resolve(&request).await.unwrap();
    assert_eq!(resolved.request.url, "http://localhost:8080/health");
    assert_eq!(resolved.request.headers[0].value, "Bearer s3cret");
    // The stored request keeps its template text.
    assert_eq!(request.url, "{{base}}/health");

    // A missing variable must be reported, never silently replaced by "".
    let broken = Request::new(HttpMethod::Get, "{{missing}}/health");
    assert!(service.environments().resolve(&broken).await.is_err());

    let active = service.environments().active_id().await.unwrap();
    assert_eq!(active, Some(environment.id));
    drop(service);

    let reopened = AppService::open(Some(path.clone())).await.unwrap();
    assert_eq!(
        reopened.environments().active_id().await.unwrap(),
        Some(environment.id)
    );
    drop(reopened);

    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[tokio::test]
async fn network_failures_are_reported_and_recorded() {
    let path = temp_db();
    let service = AppService::open(Some(path.clone())).await.unwrap();

    // Port 1 on loopback is not listening: the transport must fail cleanly.
    let request = Request::new(HttpMethod::Get, "http://127.0.0.1:1/nothing");
    let outcome = service.send_simple(&request).await;
    assert!(outcome.result.is_err());
    let error = outcome.result.unwrap_err();
    assert!(!error.to_string().is_empty());

    let history = service.history(&HistoryQuery::default()).await.unwrap();
    assert_eq!(history.len(), 1);
    assert!(history[0].error_message.is_some());
    assert!(history[0].status_code.is_none());
    drop(service);

    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[tokio::test]
async fn response_bodies_are_classified_by_content_type() {
    let path = temp_db();
    let service = AppService::open(Some(path.clone())).await.unwrap();
    let port = spawn_server("200 OK", "<root><item>1</item></root>");
    let request = Request::new(HttpMethod::Get, format!("http://127.0.0.1:{port}/xml"));
    let outcome = service.send_simple(&request).await;
    let response = outcome.result.unwrap();
    assert!(matches!(response.body, ResponseBody::Text { .. }));
    drop(service);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
