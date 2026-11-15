//! The JSON inspector model.
//!
//! Turns a `serde_json::Value` into flat, expandable rows. Keeping this pure
//! makes the tree behaviour testable without a window, and lets the view
//! virtualise by slicing the row list.

use std::collections::HashSet;

use serde_json::Value;

use crate::theme::JsonToken;

/// One rendered row of the JSON tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonRow {
    /// JSON-pointer-ish path used as the expansion key.
    pub path: String,
    pub depth: usize,
    pub key: Option<String>,
    /// Rendered value text; empty for container rows that are expanded.
    pub value: String,
    pub token: JsonToken,
    /// `true` when this row is an object or array.
    pub expandable: bool,
    pub expanded: bool,
    /// Preview shown when a container is collapsed, e.g. `{ 3 keys }`.
    pub summary: Option<String>,
    pub trailing_comma: bool,
    /// Whether this row matched the current search.
    pub matched: bool,
}

impl JsonRow {
    pub fn toggle_key(&self) -> Option<&str> {
        self.expandable.then_some(self.path.as_str())
    }
}

fn escape_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn describe(value: &Value) -> (String, JsonToken) {
    match value {
        Value::Null => ("null".to_string(), JsonToken::Null),
        Value::Bool(flag) => (flag.to_string(), JsonToken::Boolean),
        Value::Number(number) => (number.to_string(), JsonToken::Number),
        Value::String(text) => (format!("\"{}\"", escape_string(text)), JsonToken::String),
        Value::Array(items) => (format!("[{} items]", items.len()), JsonToken::Punctuation),
        Value::Object(map) => (format!("{{{} keys}}", map.len()), JsonToken::Punctuation),
    }
}

fn escape_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

/// Flatten `value` into rows, honouring `expanded` and highlighting `search`.
///
/// When a search term is present every container is treated as expanded so
/// matches deep in the document are visible, and rows that contain the term are
/// flagged.
pub fn build_rows(value: &Value, expanded: &HashSet<String>, search: &str) -> Vec<JsonRow> {
    let needle = search.trim().to_lowercase();
    let searching = !needle.is_empty();
    let mut rows = Vec::new();
    walk(
        value, "", 0, None, expanded, &needle, searching, false, &mut rows,
    );
    rows
}

/// Number of rows that match the search term.
pub fn match_count(rows: &[JsonRow]) -> usize {
    rows.iter().filter(|row| row.matched).count()
}

#[allow(clippy::too_many_arguments)]
fn walk(
    value: &Value,
    path: &str,
    depth: usize,
    key: Option<&str>,
    expanded: &HashSet<String>,
    needle: &str,
    searching: bool,
    trailing_comma: bool,
    out: &mut Vec<JsonRow>,
) {
    let (description, token) = describe(value);
    let is_container = matches!(value, Value::Array(_) | Value::Object(_));
    // `expanded` holds the paths of open containers; callers seed it with `""`
    // so the document root starts open. Searching opens everything.
    let node_expanded = is_container && (searching || expanded.contains(path));

    let label_text = key.map(|key| key.to_string());
    let matches_key = label_text
        .as_ref()
        .is_some_and(|key| key.to_lowercase().contains(needle));
    let matches_value = !needle.is_empty() && description.to_lowercase().contains(needle);

    out.push(JsonRow {
        path: path.to_string(),
        depth,
        key: label_text,
        value: if is_container {
            String::new()
        } else {
            description.clone()
        },
        token,
        expandable: is_container,
        expanded: node_expanded,
        summary: is_container.then(|| description.clone()),
        trailing_comma,
        matched: !needle.is_empty() && (matches_key || matches_value),
    });

    if !node_expanded {
        return;
    }

    match value {
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                let child_path = format!("{path}/{}", index);
                walk(
                    item,
                    &child_path,
                    depth + 1,
                    None,
                    expanded,
                    needle,
                    searching,
                    index + 1 < items.len(),
                    out,
                );
            }
        }
        Value::Object(map) => {
            let total = map.len();
            for (index, (key, item)) in map.iter().enumerate() {
                let child_path = format!("{path}/{}", escape_pointer(key));
                walk(
                    item,
                    &child_path,
                    depth + 1,
                    Some(key),
                    expanded,
                    needle,
                    searching,
                    index + 1 < total,
                    out,
                );
            }
        }
        _ => {}
    }
}

/// Indentation in pixels for a row's depth.
pub fn indent(depth: usize) -> f32 {
    depth as f32 * 14.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn default_expanded() -> HashSet<String> {
        let mut set = HashSet::new();
        set.insert(String::new());
        set
    }

    #[test]
    fn root_object_is_expanded_by_default() {
        let value = json!({"a": 1, "b": "two"});
        let rows = build_rows(&value, &default_expanded(), "");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].path, "");
        assert!(rows[0].expanded);
        assert_eq!(rows[1].key.as_deref(), Some("a"));
        assert_eq!(rows[1].value, "1");
        assert_eq!(rows[1].token, JsonToken::Number);
        assert!(rows[1].trailing_comma);
        assert_eq!(rows[2].value, "\"two\"");
        assert!(!rows[2].trailing_comma);
    }

    #[test]
    fn nested_containers_collapse_until_expanded() {
        let value = json!({"user": {"name": "Ada", "tags": ["a", "b"]}});
        let rows = build_rows(&value, &default_expanded(), "");
        assert_eq!(rows.len(), 2, "collapsed child should hide its contents");

        let mut expanded = default_expanded();
        expanded.insert("/user".to_string());
        let rows = build_rows(&value, &expanded, "");
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[2].key.as_deref(), Some("name"));
        assert_eq!(rows[2].value, "\"Ada\"");
        assert_eq!(rows[2].depth, 2);

        expanded.insert("/user/tags".to_string());
        let rows = build_rows(&value, &expanded, "");
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[4].path, "/user/tags/0");
        assert_eq!(rows[4].value, "\"a\"");
    }

    #[test]
    fn arrays_render_each_item_with_index_paths() {
        let value = json!([1, 2, 3]);
        let rows = build_rows(&value, &default_expanded(), "");
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[1].path, "/0");
        assert_eq!(rows[1].token, JsonToken::Number);
        assert_eq!(rows[3].path, "/2");
        assert!(!rows[3].trailing_comma);
    }

    #[test]
    fn search_expands_and_flags_matches() {
        let value = json!({"user": {"name": "Ada", "role": "admin"}});
        let rows = build_rows(&value, &default_expanded(), "admin");
        assert_eq!(match_count(&rows), 1);
        let hit = rows.iter().find(|row| row.matched).unwrap();
        assert_eq!(hit.path, "/user/role");
        assert_eq!(rows.len(), 5, "searching should expand containers");
    }

    #[test]
    fn values_cover_every_json_type() {
        let value = json!({
            "string": "text",
            "number": 1.5,
            "boolean": true,
            "null": null,
            "array": [],
            "object": {}
        });
        let rows = build_rows(&value, &default_expanded(), "");
        let tokens: Vec<JsonToken> = rows.iter().skip(1).map(|row| row.token).collect();
        assert!(tokens.contains(&JsonToken::String));
        assert!(tokens.contains(&JsonToken::Number));
        assert!(tokens.contains(&JsonToken::Boolean));
        assert!(tokens.contains(&JsonToken::Null));
        assert!(tokens.contains(&JsonToken::Punctuation));
    }

    #[test]
    fn strings_are_escaped() {
        let value = json!({"quote": "a\"b\n"});
        let rows = build_rows(&value, &default_expanded(), "");
        assert!(rows[1].value.contains("\\\""));
        assert!(rows[1].value.contains("\\n"));
        assert!(!rows[1].value.contains('\n'));
    }

    #[test]
    fn pointer_paths_escape_slashes_and_tildes() {
        let value = json!({"a/b": 1, "c~d": 2});
        let rows = build_rows(&value, &default_expanded(), "");
        assert!(rows.iter().any(|row| row.path == "/a~1b"));
        assert!(rows.iter().any(|row| row.path == "/c~0d"));
    }

    #[test]
    fn indent_grows_with_depth() {
        assert!(indent(2) > indent(1));
        assert_eq!(indent(0), 0.0);
    }
}
