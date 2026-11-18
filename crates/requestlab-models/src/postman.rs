//! Postman collection import and export (schema v2.1).
//!
//! The importer is deliberately strict about structure and lenient about extra
//! fields, which is what real exports contain. Anything that cannot be mapped
//! to a [`Request`] is reported instead of being dropped silently.

use serde_json::{json, Value};

use crate::{
    ApiKeyLocation, Authentication, CollectionTree, EnvironmentVariable, HeaderEntry, HttpMethod,
    QueryParameter, Request, RequestBody,
};

/// A parsed collection, independent of the database.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedCollection {
    pub name: String,
    pub description: Option<String>,
    pub variables: Vec<EnvironmentVariable>,
    /// Requests with the folder path (outermost first) they live in.
    pub requests: Vec<ImportedRequest>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedRequest {
    pub path: Vec<String>,
    pub name: String,
    pub request: Request,
}

/// A readable import/export failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostmanError {
    pub message: String,
}

impl PostmanError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for PostmanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for PostmanError {}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn description_field(value: &Value) -> Option<String> {
    match value.get("description") {
        Some(Value::String(text)) if !text.trim().is_empty() => Some(text.clone()),
        Some(Value::Object(object)) => object
            .get("content")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(str::to_string),
        _ => None,
    }
}

/// Import a Postman collection document.
pub fn import(json_text: &str) -> Result<ImportedCollection, PostmanError> {
    let document: Value = serde_json::from_str(json_text)
        .map_err(|err| PostmanError::new(format!("that is not valid JSON: {err}")))?;

    let info = document
        .get("info")
        .ok_or_else(|| PostmanError::new("the document has no 'info' object"))?;
    let name = string_field(info, "name")
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| PostmanError::new("the collection has no name"))?;

    if let Some(schema) = string_field(info, "schema") {
        if !schema.contains("v2") && !schema.contains("collection") {
            return Err(PostmanError::new(format!(
                "unsupported collection schema '{schema}'; Postman v2 collections are supported"
            )));
        }
    }

    let mut collection = ImportedCollection {
        name,
        description: description_field(info),
        variables: document
            .get("variable")
            .and_then(Value::as_array)
            .map(|variables| variables.iter().filter_map(import_variable).collect())
            .unwrap_or_default(),
        requests: Vec::new(),
    };

    let items = document
        .get("item")
        .and_then(Value::as_array)
        .ok_or_else(|| PostmanError::new("the collection has no 'item' array"))?;
    walk_items(items, &mut Vec::new(), &mut collection.requests)?;
    Ok(collection)
}

fn walk_items(
    items: &[Value],
    path: &mut Vec<String>,
    out: &mut Vec<ImportedRequest>,
) -> Result<(), PostmanError> {
    for item in items {
        let name = string_field(item, "name").unwrap_or_else(|| "Untitled".to_string());
        if let Some(request) = item.get("request") {
            let parsed = import_request(request, &name)?;
            out.push(ImportedRequest {
                path: path.clone(),
                name,
                request: parsed,
            });
        } else if let Some(children) = item.get("item").and_then(Value::as_array) {
            path.push(name);
            walk_items(children, path, out)?;
            path.pop();
        }
        // Items with neither are skipped: Postman exports sometimes contain
        // empty placeholders, and there is nothing to import.
    }
    Ok(())
}

fn import_variable(value: &Value) -> Option<EnvironmentVariable> {
    let key = value.get("key").and_then(Value::as_str)?.trim().to_string();
    if key.is_empty() {
        return None;
    }
    let raw = value.get("value").cloned().unwrap_or(Value::Null);
    let text = match raw {
        Value::String(text) => text,
        Value::Null => String::new(),
        other => other.to_string(),
    };
    let mut variable = EnvironmentVariable::new(key, text);
    if value
        .get("disabled")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        variable.enabled = false;
    }
    if value.get("type").and_then(Value::as_str) == Some("secret") {
        variable.is_secret = true;
    }
    Some(variable)
}

fn import_request(value: &Value, name: &str) -> Result<Request, PostmanError> {
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .and_then(HttpMethod::parse)
        .unwrap_or(HttpMethod::Get);

    let mut request = Request::new(method, String::new());
    request.name = name.to_string();

    match value.get("url") {
        Some(Value::String(raw)) => request.url = raw.clone(),
        Some(Value::Object(url)) => {
            request.url = url
                .get("raw")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if let Some(query) = url.get("query").and_then(Value::as_array) {
                for parameter in query {
                    let key = parameter
                        .get("key")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    if key.is_empty() {
                        continue;
                    }
                    let value = parameter
                        .get("value")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let mut row = QueryParameter::new(key, value);
                    row.enabled = !parameter
                        .get("disabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    request.query_params.push(row);
                }
            }
        }
        _ => {}
    }

    if let Some(headers) = value.get("header").and_then(Value::as_array) {
        for header in headers {
            let key = header
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if key.is_empty() {
                continue;
            }
            let mut row = HeaderEntry::new(
                key,
                header
                    .get("value")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            );
            row.enabled = !header
                .get("disabled")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            request.headers.push(row);
        }
    }

    // An explicit auth block wins over an Authorization header.
    if let Some(auth) = value.get("auth") {
        if let Some(authentication) = import_auth(auth) {
            request.authentication = authentication;
            request
                .headers
                .retain(|header| !header.key.eq_ignore_ascii_case("authorization"));
        }
    }

    if let Some(body) = value.get("body") {
        request.body = import_body(body);
    }
    Ok(request)
}

/// Read `auth.<kind>[]` entry whose `key` matches, which is Postman's shape:
/// `{"type":"basic","basic":[{"key":"username","value":"ada"}]}`.
fn auth_parameter(auth: &Value, kind: &str, key: &str) -> Option<String> {
    auth.get(kind)
        .and_then(Value::as_array)
        .and_then(|entries| {
            entries.iter().find_map(|entry| {
                if entry.get("key").and_then(Value::as_str) == Some(key) {
                    entry
                        .get("value")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                } else {
                    None
                }
            })
        })
}

fn import_auth(auth: &Value) -> Option<Authentication> {
    let kind = auth.get("type").and_then(Value::as_str)?;
    Some(match kind.to_ascii_lowercase().as_str() {
        "basic" => Authentication::Basic {
            username: auth_parameter(auth, "basic", "username")
                .or_else(|| auth_parameter(auth, "basic", "key"))
                .unwrap_or_default(),
            password: auth_parameter(auth, "basic", "password").unwrap_or_default(),
        },
        "bearer" => Authentication::Bearer {
            token: auth_parameter(auth, "bearer", "token").unwrap_or_default(),
        },
        "apikey" => Authentication::ApiKey {
            key: auth_parameter(auth, "apikey", "key").unwrap_or_default(),
            value: auth_parameter(auth, "apikey", "value").unwrap_or_default(),
            location: match auth_parameter(auth, "apikey", "in").as_deref() {
                Some("query") => ApiKeyLocation::Query,
                _ => ApiKeyLocation::Header,
            },
        },
        "digest" => Authentication::Digest {
            username: auth_parameter(auth, "digest", "username").unwrap_or_default(),
            password: auth_parameter(auth, "digest", "password").unwrap_or_default(),
        },
        "noauth" => Authentication::None,
        _ => return None,
    })
}

fn import_body(body: &Value) -> RequestBody {
    match body.get("mode").and_then(Value::as_str).unwrap_or("") {
        "raw" => {
            let raw = body
                .get("raw")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let language = body
                .get("options")
                .and_then(|options| options.get("raw"))
                .and_then(|raw| raw.get("language"))
                .and_then(Value::as_str);
            if language == Some("json") {
                RequestBody::Json { content: raw }
            } else {
                let content_type = body
                    .get("options")
                    .and_then(|options| options.get("raw"))
                    .and_then(|raw| raw.get("header"))
                    .and_then(Value::as_str)
                    .unwrap_or("text/plain")
                    .to_string();
                RequestBody::Raw {
                    content: raw,
                    content_type,
                }
            }
        }
        "urlencoded" | "formdata" => {
            let fields = body
                .get(
                    if body.get("mode").and_then(Value::as_str) == Some("formdata") {
                        "formdata"
                    } else {
                        "urlencoded"
                    },
                )
                .and_then(Value::as_array)
                .map(|fields| {
                    fields
                        .iter()
                        .filter_map(|field| {
                            let key = field.get("key").and_then(Value::as_str)?;
                            if key.is_empty()
                                || field.get("type").and_then(Value::as_str) == Some("file")
                            {
                                return None;
                            }
                            Some(QueryParameter::new(
                                key,
                                field.get("value").and_then(Value::as_str).unwrap_or(""),
                            ))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            RequestBody::FormUrlEncoded { fields }
        }
        _ => RequestBody::None,
    }
}

/// Requests whose authentication has no Postman representation.
pub fn export_warnings(tree: &CollectionTree) -> Vec<String> {
    tree.all_requests()
        .into_iter()
        .filter(|saved| {
            matches!(
                saved.request.authentication,
                Authentication::AwsSigV4 { .. } | Authentication::OAuth2 { .. }
            )
        })
        .map(|saved| {
            format!(
                "'{}' uses {} authentication, which Postman cannot represent",
                saved.display_name(),
                saved.request.authentication.label()
            )
        })
        .collect()
}

fn export_auth(auth: &Authentication) -> Value {
    match auth {
        Authentication::Basic { username, password } => json!({
            "type": "basic",
            "basic": [
                { "key": "username", "value": username, "type": "string" },
                { "key": "password", "value": password, "type": "string" }
            ]
        }),
        Authentication::Bearer { token } => json!({
            "type": "bearer",
            "bearer": [{ "key": "token", "value": token, "type": "string" }]
        }),
        Authentication::ApiKey {
            key,
            value,
            location,
        } => json!({
            "type": "apikey",
            "apikey": [
                { "key": "key", "value": key, "type": "string" },
                { "key": "value", "value": value, "type": "string" },
                { "key": "in", "value": match location {
                    ApiKeyLocation::Header => "header",
                    ApiKeyLocation::Query => "query",
                }, "type": "string" }
            ]
        }),
        Authentication::Digest { username, password } => json!({
            "type": "digest",
            "digest": [
                { "key": "username", "value": username, "type": "string" },
                { "key": "password", "value": password, "type": "string" }
            ]
        }),
        Authentication::None | Authentication::AwsSigV4 { .. } | Authentication::OAuth2 { .. } => {
            json!({ "type": "noauth" })
        }
    }
}

fn export_request(name: &str, request: &Request) -> Value {
    let mut raw_url = request.url.clone();
    let query: Vec<Value> = request
        .query_params
        .iter()
        .filter(|parameter| parameter.enabled && !parameter.key.trim().is_empty())
        .map(|parameter| json!({ "key": parameter.key, "value": parameter.value }))
        .collect();
    if !query.is_empty() {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for parameter in request
            .query_params
            .iter()
            .filter(|parameter| parameter.enabled)
        {
            serializer.append_pair(&parameter.key, &parameter.value);
        }
        let encoded = serializer.finish();
        if !encoded.is_empty() {
            raw_url = format!(
                "{raw_url}{}{encoded}",
                if raw_url.contains('?') { "&" } else { "?" }
            );
        }
    }
    let url = json!({ "raw": raw_url, "query": query });

    let body = match &request.body {
        RequestBody::None => Value::Null,
        RequestBody::Json { content } => json!({
            "mode": "raw",
            "raw": content,
            "options": { "raw": { "language": "json" } }
        }),
        RequestBody::Raw {
            content,
            content_type,
        } => json!({
            "mode": "raw",
            "raw": content,
            "options": { "raw": { "language": "text", "header": content_type } }
        }),
        RequestBody::FormUrlEncoded { fields } => json!({
            "mode": "urlencoded",
            "urlencoded": fields.iter().filter(|field| field.enabled).map(|field| json!({
                "key": field.key,
                "value": field.value,
                "type": "text"
            })).collect::<Vec<_>>()
        }),
    };

    let mut request_value = json!({
        "method": request.method.as_str(),
        "header": request.headers.iter().filter(|header| header.enabled).map(|header| json!({
            "key": header.key,
            "value": header.value,
            "type": "text"
        })).collect::<Vec<_>>(),
        "url": url,
        "auth": export_auth(&request.authentication)
    });
    if !body.is_null() {
        request_value["body"] = body;
    }

    json!({
        "name": name,
        "request": request_value
    })
}

/// Export a collection tree to a Postman v2.1 document.
pub fn export(tree: &CollectionTree) -> String {
    fn export_node(node: &crate::CollectionNode) -> Vec<Value> {
        let mut items: Vec<Value> = node
            .requests
            .iter()
            .map(|saved| export_request(&saved.display_name(), &saved.request))
            .collect();
        for child in &node.children {
            items.push(json!({
                "name": child.name(),
                "item": export_node(child)
            }));
        }
        items
    }

    let mut document = json!({
        "info": {
            "name": tree.collection.name,
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
        },
        "item": export_node(&tree.root)
    });
    if let Some(description) = &tree.collection.description {
        if !description.trim().is_empty() {
            document["info"]["description"] = json!(description);
        }
    }
    serde_json::to_string_pretty(&document).expect("a JSON document always serialises")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{build_tree, Collection, Folder, SavedRequest};
    use uuid::Uuid;

    fn sample_document() -> &'static str {
        r#"{
          "info": {
            "name": "Payments API",
            "description": "Card and refund endpoints",
            "schema": "https://schema.getpostman.com/json/collection/v2.1.0/collection.json"
          },
          "variable": [
            { "key": "baseUrl", "value": "https://api.example.com" },
            { "key": "token", "value": "abc", "type": "secret" }
          ],
          "item": [
            {
              "name": "Charges",
              "item": [
                {
                  "name": "Create charge",
                  "request": {
                    "method": "POST",
                    "header": [
                      { "key": "Content-Type", "value": "application/json" },
                      { "key": "X-Debug", "value": "1", "disabled": true }
                    ],
                    "url": {
                      "raw": "https://api.example.com/charges?expand=customer",
                      "query": [ { "key": "expand", "value": "customer" } ]
                    },
                    "auth": {
                      "type": "basic",
                      "basic": [
                        { "key": "username", "value": "ada" },
                        { "key": "password", "value": "secret" }
                      ]
                    },
                    "body": { "mode": "raw", "raw": "{\"amount\":100}", "options": { "raw": { "language": "json" } } }
                  }
                }
              ]
            },
            {
              "name": "Refund",
              "request": {
                "method": "DELETE",
                "url": "https://api.example.com/refunds/1",
                "header": [],
                "body": {
                  "mode": "urlencoded",
                  "urlencoded": [ { "key": "reason", "value": "duplicate" } ]
                }
              }
            },
            {
              "name": "Search",
              "request": {
                "method": "GET",
                "url": "https://api.example.com/search",
                "auth": { "type": "apikey", "apikey": [
                  { "key": "key", "value": "api_key" },
                  { "key": "value", "value": "k-1" },
                  { "key": "in", "value": "query" }
                ] }
              }
            }
          ]
        }"#
    }

    #[test]
    fn imports_folders_requests_and_variables() {
        let collection = import(sample_document()).unwrap();
        assert_eq!(collection.name, "Payments API");
        assert_eq!(
            collection.description.as_deref(),
            Some("Card and refund endpoints")
        );
        assert_eq!(collection.variables.len(), 2);
        assert_eq!(collection.variables[0].key, "baseUrl");
        assert!(collection.variables[1].is_secret);

        assert_eq!(collection.requests.len(), 3);
        let create = &collection.requests[0];
        assert_eq!(create.path, vec!["Charges".to_string()]);
        assert_eq!(create.name, "Create charge");
        assert_eq!(create.request.method, HttpMethod::Post);
        assert_eq!(
            create.request.url,
            "https://api.example.com/charges?expand=customer"
        );
        assert_eq!(create.request.query_params.len(), 1);
        assert_eq!(create.request.headers.len(), 2);
        assert!(!create.request.headers[1].enabled);
        assert_eq!(
            create.request.authentication,
            Authentication::Basic {
                username: "ada".into(),
                password: "secret".into()
            }
        );
        assert_eq!(
            create.request.body.text(),
            Some("{\"amount\":100}"),
            "JSON bodies keep their language"
        );
        assert_eq!(create.request.body.kind(), crate::RequestBodyKind::Json);

        assert_eq!(collection.requests[1].request.method, HttpMethod::Delete);
        match &collection.requests[1].request.body {
            RequestBody::FormUrlEncoded { fields } => {
                assert_eq!(fields.len(), 1);
                assert_eq!(fields[0].key, "reason");
                assert_eq!(fields[0].value, "duplicate");
            }
            other => panic!("expected a form body, got {other:?}"),
        }

        assert_eq!(
            collection.requests[2].request.authentication,
            Authentication::ApiKey {
                key: "api_key".into(),
                value: "k-1".into(),
                location: ApiKeyLocation::Query
            }
        );
    }

    #[test]
    fn rejects_documents_that_are_not_collections() {
        assert!(import("not json").is_err());
        assert!(import("{}").is_err());
        assert!(import(r#"{"info":{"name":"x"}}"#).is_err());
        assert!(import(r#"{"info":{"name":"x","schema":"v1"},"item":[]}"#).is_err());
    }

    fn tree_fixture() -> CollectionTree {
        let collection = Collection {
            id: Uuid::new_v4(),
            name: "Round trip".into(),
            description: Some("Exported and re-imported".into()),
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let folder = Folder {
            id: Uuid::new_v4(),
            collection_id: collection.id,
            parent_id: None,
            name: "Users".into(),
            sort_order: 1,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let mut request = Request::new(HttpMethod::Post, "https://api.example.com/users");
        request
            .headers
            .push(HeaderEntry::new("Accept", "application/json"));
        request.body = RequestBody::json("{\"name\":\"Ada\"}");
        request.authentication = Authentication::Bearer {
            token: "tok".into(),
        };
        let root_request = Request::new(HttpMethod::Get, "https://api.example.com/health");
        let saved = SavedRequest {
            id: Uuid::new_v4(),
            collection_id: collection.id,
            folder_id: Some(folder.id),
            name: "Create user".into(),
            request,
            sort_order: 1,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let saved_root = SavedRequest {
            id: Uuid::new_v4(),
            collection_id: collection.id,
            folder_id: None,
            name: "Health".into(),
            request: root_request,
            sort_order: 1,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        build_tree(collection, vec![folder], vec![saved, saved_root])
    }

    #[test]
    fn export_then_import_preserves_the_collection() {
        let tree = tree_fixture();
        let document = export(&tree);
        let imported = import(&document).unwrap();

        assert_eq!(imported.name, "Round trip");
        assert_eq!(
            imported.description.as_deref(),
            Some("Exported and re-imported")
        );
        assert_eq!(imported.requests.len(), 2);

        let create = imported
            .requests
            .iter()
            .find(|request| request.name == "Create user")
            .unwrap();
        assert_eq!(create.path, vec!["Users".to_string()]);
        assert_eq!(create.request.method, HttpMethod::Post);
        assert_eq!(create.request.url, "https://api.example.com/users");
        assert_eq!(create.request.headers.len(), 1);
        assert_eq!(
            create.request.authentication,
            Authentication::Bearer {
                token: "tok".into()
            }
        );
        assert_eq!(create.request.body, RequestBody::json("{\"name\":\"Ada\"}"));

        let health = imported
            .requests
            .iter()
            .find(|request| request.name == "Health")
            .unwrap();
        assert!(health.path.is_empty());
        assert_eq!(health.request.method, HttpMethod::Get);
        assert!(matches!(
            health.request.authentication,
            Authentication::None
        ));
    }

    #[test]
    fn unrepresentable_auth_is_reported_not_dropped_silently() {
        let mut tree = tree_fixture();
        let saved = &mut tree.root.requests[0];
        saved.request.authentication = Authentication::AwsSigV4 {
            access_key: "AKID".into(),
            secret_key: "secret".into(),
            region: "us-east-1".into(),
            service: "s3".into(),
            session_token: String::new(),
        };
        let warnings = export_warnings(&tree);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("Health"));
        assert!(warnings[0].contains("AWS"));
    }
}
