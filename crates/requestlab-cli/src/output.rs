//! Text and JSON renderers for CLI results.

use requestlab_core::AppSettings;
use requestlab_models::{
    Collection, CollectionTree, Environment, HistoryEntry, HttpResponse, ResponseBody,
};

/// Render one HTTP response for a terminal.
pub fn response_text(response: &HttpResponse, print_body: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{} {} · {} · {} · {}\n",
        response.status,
        if response.status_text.is_empty() {
            "Unknown"
        } else {
            response.status_text.as_str()
        },
        human_bytes(response.size_bytes),
        human_duration(response.duration_ms),
        response
            .content_type
            .as_deref()
            .unwrap_or("no content type"),
    ));
    out.push_str(&format!("{}\n", response.url));

    if !response.headers.is_empty() {
        out.push_str("\nHeaders\n");
        for header in &response.headers {
            out.push_str(&format!("  {}: {}\n", header.name, header.value));
        }
    }

    if !print_body {
        return out;
    }

    match &response.body {
        ResponseBody::Empty => out.push_str("\n(empty body)\n"),
        ResponseBody::Text { text, truncated } => {
            out.push('\n');
            out.push_str(text);
            if !text.ends_with('\n') {
                out.push('\n');
            }
            if *truncated {
                out.push_str("(body truncated)\n");
            }
        }
        ResponseBody::Binary { bytes, truncated } => {
            out.push_str(&format!("\n<binary body, {}>\n", human_bytes(bytes.len())));
            if *truncated {
                out.push_str("(body truncated)\n");
            }
        }
    }
    out
}

/// Render one HTTP response as JSON.
pub fn response_json(response: &HttpResponse, include_body: bool) -> String {
    let headers: Vec<serde_json::Value> = response
        .headers
        .iter()
        .map(|header| serde_json::json!({ "name": header.name, "value": header.value }))
        .collect();

    let body = match &response.body {
        ResponseBody::Empty => serde_json::Value::Null,
        ResponseBody::Text { text, truncated } => serde_json::json!({
            "kind": "text",
            "text": if include_body { text.clone() } else { String::new() },
            "truncated": truncated,
        }),
        ResponseBody::Binary { bytes, truncated } => serde_json::json!({
            "kind": "binary",
            "bytes": bytes.len(),
            "truncated": truncated,
        }),
    };

    serde_json::json!({
        "ok": response.is_success(),
        "status": response.status,
        "status_text": response.status_text,
        "url": response.url,
        "duration_ms": response.duration_ms,
        "size_bytes": response.size_bytes,
        "content_type": response.content_type,
        "completed_at": response.completed_at,
        "headers": headers,
        "body": body,
    })
    .to_string()
}

pub fn collections_text(trees: &[CollectionTree]) -> String {
    if trees.is_empty() {
        return "No collections yet. Create one with: requestlab-cli collections create <name>\n"
            .to_string();
    }
    let mut out = String::new();
    for tree in trees {
        out.push_str(&format!(
            "{} ({}) · {} requests\n",
            tree.collection.name,
            tree.collection.id,
            tree.all_requests().len()
        ));
        for node in &tree.root.children {
            render_node(node, 1, &mut out);
        }
    }
    out
}

fn render_node(node: &requestlab_models::CollectionNode, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    if let Some(folder) = &node.folder {
        out.push_str(&format!("{indent}📁 {}\n", folder.name));
    }
    for saved in &node.requests {
        out.push_str(&format!(
            "{indent}{} {} · {}\n",
            saved.request.method, saved.name, saved.id
        ));
    }
    for child in &node.children {
        render_node(child, depth + 1, out);
    }
}

pub fn collections_json(trees: &[CollectionTree]) -> String {
    let collections: Vec<serde_json::Value> = trees
        .iter()
        .map(|tree| {
            serde_json::json!({
                "id": tree.collection.id,
                "name": tree.collection.name,
                "description": tree.collection.description,
                "request_count": tree.all_requests().len(),
            })
        })
        .collect();
    serde_json::json!({ "collections": collections }).to_string()
}

pub fn collection_created_json(collection: &Collection) -> String {
    serde_json::json!({
        "id": collection.id,
        "name": collection.name,
        "created_at": collection.created_at,
    })
    .to_string()
}

pub fn history_text(rows: &[HistoryEntry]) -> String {
    if rows.is_empty() {
        return "No history entries match.\n".to_string();
    }
    let mut out = String::new();
    for row in rows {
        let status = row
            .status_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "ERR".to_string());
        let duration = row
            .duration_ms
            .map(human_duration)
            .unwrap_or_else(|| "-".to_string());
        out.push_str(&format!(
            "{}  {:>4}  {:>8}  {} {}\n",
            row.created_at, status, duration, row.method, row.url
        ));
        if let Some(error) = &row.error_message {
            out.push_str(&format!("      {error}\n"));
        }
    }
    out
}

pub fn history_json(rows: &[HistoryEntry]) -> String {
    let entries: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "method": row.method,
                "url": row.url,
                "status_code": row.status_code,
                "duration_ms": row.duration_ms,
                "error": row.error_message,
                "created_at": row.created_at,
            })
        })
        .collect();
    serde_json::json!({ "history": entries }).to_string()
}

pub fn environments_text(environments: &[Environment], active: Option<uuid::Uuid>) -> String {
    if environments.is_empty() {
        return "No environments yet. Create one with: requestlab-cli environments create <name>\n"
            .to_string();
    }
    let mut out = String::new();
    for environment in environments {
        let marker = if Some(environment.id) == active {
            "*"
        } else {
            " "
        };
        out.push_str(&format!(
            "{marker} {} ({}) · {} variables\n",
            environment.name,
            environment.id,
            environment.variables.len()
        ));
    }
    out
}

pub fn environments_json(environments: &[Environment], active: Option<uuid::Uuid>) -> String {
    let list: Vec<serde_json::Value> = environments
        .iter()
        .map(|environment| {
            serde_json::json!({
                "id": environment.id,
                "name": environment.name,
                "active": Some(environment.id) == active,
                "variables": environment.variables.len(),
            })
        })
        .collect();
    serde_json::json!({ "environments": list }).to_string()
}

pub fn variables_text(environment: &Environment) -> String {
    if environment.variables.is_empty() {
        return format!("{} has no variables.\n", environment.name);
    }
    let mut out = format!("{}\n", environment.name);
    for variable in &environment.variables {
        let value = if variable.is_secret {
            "********".to_string()
        } else {
            variable.value.clone()
        };
        let state = if variable.enabled { "" } else { " (disabled)" };
        out.push_str(&format!("  {} = {}{state}\n", variable.key, value));
    }
    out
}

pub fn variables_json(environment: &Environment) -> String {
    let variables: Vec<serde_json::Value> = environment
        .variables
        .iter()
        .map(|variable| {
            serde_json::json!({
                "key": variable.key,
                // Secrets are redacted even in machine-readable output.
                "value": if variable.is_secret { "********" } else { variable.value.as_str() },
                "enabled": variable.enabled,
                "secret": variable.is_secret,
            })
        })
        .collect();
    serde_json::json!({ "environment": environment.name, "variables": variables }).to_string()
}

pub fn settings_json(settings: &AppSettings) -> String {
    serde_json::json!({
        "timeout_seconds": settings.timeout_seconds,
        "history_limit": settings.history_limit,
        "theme": settings.theme,
    })
    .to_string()
}

/// Compact byte formatting shared with the UI's status bar.
pub fn human_bytes(bytes: usize) -> String {
    requestlab_models::human_bytes(bytes as u64)
}

pub fn human_duration(ms: u64) -> String {
    requestlab_models::human_duration(ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use requestlab_models::{HttpResponse, ResponseHeader};

    fn response(body: ResponseBody) -> HttpResponse {
        HttpResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![ResponseHeader {
                name: "content-type".into(),
                value: "application/json".into(),
            }],
            body,
            content_type: Some("application/json".into()),
            size_bytes: 17,
            duration_ms: 42,
            completed_at: "2026-01-01T00:00:00Z".into(),
            url: "http://localhost/x".into(),
            cookies: Vec::new(),
        }
    }

    #[test]
    fn text_output_includes_status_and_body() {
        let text = response_text(&response(ResponseBody::text("{\"hello\":true}")), true);
        assert!(text.contains("200 OK"));
        assert!(text.contains("{\"hello\":true}"));
        assert!(text.contains("content-type"));
    }

    #[test]
    fn text_output_respects_no_body() {
        let text = response_text(&response(ResponseBody::text("secret")), false);
        assert!(!text.contains("secret"));
    }

    #[test]
    fn json_output_is_valid_json_and_hides_body_when_asked() {
        let rendered = response_json(&response(ResponseBody::text("payload")), false);
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("valid json");
        assert_eq!(value["status"], 200);
        assert_eq!(value["body"]["text"], "");
        assert_eq!(value["headers"][0]["name"], "content-type");
    }

    #[test]
    fn binary_bodies_report_a_size_instead_of_content() {
        let text = response_text(
            &response(ResponseBody::Binary {
                bytes: vec![0, 1, 2],
                truncated: false,
            }),
            true,
        );
        assert!(text.contains("binary body"));
    }
}
