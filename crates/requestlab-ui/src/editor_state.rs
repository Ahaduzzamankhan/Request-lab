//! Editor state for a single request tab.
//!
//! The text fields are GPUI entities; the request itself is rebuilt from them on
//! demand. That keeps one source of truth for every character the user types and
//! means the saved request is never mutated by environment resolution.

use gpui::{App, AppContext, Entity};
use requestlab_models::{
    ApiKeyLocation, Authentication, HeaderEntry, HttpMethod, QueryParameter, Request, RequestBody,
    RequestBodyKind,
};
use uuid::Uuid;

use crate::input::TextInput;

/// Which request configuration tab is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorTab {
    Params,
    Headers,
    Body,
    Auth,
}

impl EditorTab {
    pub const ALL: [EditorTab; 4] = [
        EditorTab::Params,
        EditorTab::Headers,
        EditorTab::Body,
        EditorTab::Auth,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EditorTab::Params => "Params",
            EditorTab::Headers => "Headers",
            EditorTab::Body => "Body",
            EditorTab::Auth => "Auth",
        }
    }
}

/// Authentication selector mirrored as a plain enum for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
    None,
    Basic,
    Bearer,
    ApiKey,
    Digest,
    /// AWS SigV4 and OAuth 2.0 are edited through their own fields; the editor
    /// keeps the original configuration so switching tabs never drops it.
    External,
}

impl AuthKind {
    pub const ALL: [AuthKind; 5] = [
        AuthKind::None,
        AuthKind::Basic,
        AuthKind::Bearer,
        AuthKind::ApiKey,
        AuthKind::Digest,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AuthKind::None => "No auth",
            AuthKind::Basic => "Basic",
            AuthKind::Bearer => "Bearer",
            AuthKind::ApiKey => "API Key",
            AuthKind::Digest => "Digest",
            AuthKind::External => "Configured elsewhere",
        }
    }

    pub fn from_authentication(auth: &Authentication) -> Self {
        match auth {
            Authentication::None => AuthKind::None,
            Authentication::Basic { .. } => AuthKind::Basic,
            Authentication::Bearer { .. } => AuthKind::Bearer,
            Authentication::ApiKey { .. } => AuthKind::ApiKey,
            Authentication::Digest { .. } => AuthKind::Digest,
            Authentication::AwsSigV4 { .. } | Authentication::OAuth2 { .. } => AuthKind::External,
        }
    }

    /// Label for the first credential field.
    pub fn primary_label(self) -> &'static str {
        match self {
            AuthKind::None => "",
            AuthKind::Basic => "Username",
            AuthKind::Bearer => "Token",
            AuthKind::ApiKey => "Key",
            AuthKind::Digest => "Username",
            AuthKind::External => "",
        }
    }

    /// Label for the second credential field.
    pub fn secondary_label(self) -> &'static str {
        match self {
            AuthKind::None => "",
            AuthKind::Basic => "Password",
            AuthKind::Bearer => "",
            AuthKind::ApiKey => "Value",
            AuthKind::Digest => "Password",
            AuthKind::External => "",
        }
    }
}

/// A key/value row with editable cells.
pub struct KeyValueRow {
    pub id: Uuid,
    pub key: Entity<TextInput>,
    pub value: Entity<TextInput>,
    pub enabled: bool,
}

impl KeyValueRow {
    fn new(cx: &mut App, key: &str, value: &str, enabled: bool, placeholder_key: &str) -> Self {
        let key_text = key.to_string();
        let value_text = value.to_string();
        let placeholder = placeholder_key.to_string();
        let key = cx.new(move |cx| {
            let mut input = TextInput::new(cx, placeholder, false);
            input.set_content(key_text);
            input
        });
        let value = cx.new(move |cx| {
            let mut input = TextInput::new(cx, "Value", false);
            input.set_content(value_text);
            input
        });
        Self {
            id: Uuid::new_v4(),
            key,
            value,
            enabled,
        }
    }

    pub fn key_text(&self, cx: &App) -> String {
        self.key.read(cx).content().to_string()
    }

    pub fn value_text(&self, cx: &App) -> String {
        self.value.read(cx).content().to_string()
    }

    pub fn is_populated(&self, cx: &App) -> bool {
        !self.key_text(cx).trim().is_empty() || !self.value_text(cx).trim().is_empty()
    }
}

/// Where a saved request lives, so Save can update instead of duplicating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedLocation {
    pub saved_request_id: Uuid,
    pub collection_id: Uuid,
    pub folder_id: Option<Uuid>,
}

/// Everything the editor needs to show one request.
pub struct RequestEditor {
    pub request_id: Uuid,
    pub name: Entity<TextInput>,
    pub method: HttpMethod,
    pub url: Entity<TextInput>,
    pub tab: EditorTab,
    pub params: Vec<KeyValueRow>,
    pub headers: Vec<KeyValueRow>,
    pub body_kind: RequestBodyKind,
    pub body: Entity<TextInput>,
    pub body_content_type: Entity<TextInput>,
    pub form: Vec<KeyValueRow>,
    pub auth_kind: AuthKind,
    pub auth_primary: Entity<TextInput>,
    pub auth_secondary: Entity<TextInput>,
    /// The request's authentication as loaded, kept so a scheme the editor does
    /// not expose (AWS SigV4, OAuth 2.0) survives edits to the other tabs.
    pub preserved_auth: Authentication,
    pub api_key_location: ApiKeyLocation,
    pub saved: Option<SavedLocation>,
}

impl RequestEditor {
    pub fn new(cx: &mut App, theme: &crate::theme::Theme) -> Self {
        let mut editor = Self::blank_editor(cx, theme);
        editor.load(&Request::blank(), cx);
        editor
    }

    fn blank_editor(cx: &mut App, _theme: &crate::theme::Theme) -> Self {
        let name = cx.new(|cx| TextInput::new(cx, "Request name", false));
        let url = cx.new(|cx| TextInput::new(cx, "https://api.example.com/v1/users", false));
        let body = cx.new(|cx| TextInput::new(cx, "{\n  \"key\": \"value\"\n}", true));
        let body_type = cx.new(|cx| TextInput::new(cx, "text/plain", false));
        let auth_primary = cx.new(|cx| TextInput::new(cx, "Username", false));
        let auth_secondary = cx.new(|cx| TextInput::new(cx, "Password", false));
        Self {
            request_id: Uuid::new_v4(),
            name,
            method: HttpMethod::Get,
            url,
            tab: EditorTab::Params,
            params: Vec::new(),
            headers: Vec::new(),
            body_kind: RequestBodyKind::None,
            body,
            body_content_type: body_type,
            form: Vec::new(),
            auth_kind: AuthKind::None,
            auth_primary,
            auth_secondary,
            preserved_auth: Authentication::None,
            api_key_location: ApiKeyLocation::Header,
            saved: None,
        }
    }

    /// Replace the editor's contents with `request`.
    pub fn load(&mut self, request: &Request, cx: &mut App) {
        self.request_id = request.id;
        self.name
            .update(cx, |input, _| input.set_content(request.name.clone()));
        self.url
            .update(cx, |input, _| input.set_content(request.url.clone()));
        self.method = request.method;

        self.params = request
            .query_params
            .iter()
            .map(|param| KeyValueRow::new(cx, &param.key, &param.value, param.enabled, "Parameter"))
            .collect();
        if self.params.is_empty() {
            self.params
                .push(KeyValueRow::new(cx, "", "", true, "Parameter"));
        }

        self.headers = request
            .headers
            .iter()
            .map(|header| {
                KeyValueRow::new(cx, &header.key, &header.value, header.enabled, "Header")
            })
            .collect();
        if self.headers.is_empty() {
            self.headers
                .push(KeyValueRow::new(cx, "", "", true, "Header"));
        }

        self.body_kind = request.body.kind();
        match &request.body {
            RequestBody::None => {
                self.body.update(cx, |input, _| input.set_content(""));
                self.form.clear();
            }
            RequestBody::Json { content } => {
                self.body
                    .update(cx, |input, _| input.set_content(content.clone()));
                self.form.clear();
            }
            RequestBody::Raw {
                content,
                content_type,
            } => {
                self.body
                    .update(cx, |input, _| input.set_content(content.clone()));
                self.body_content_type
                    .update(cx, |input, _| input.set_content(content_type.clone()));
                self.form.clear();
            }
            RequestBody::FormUrlEncoded { fields } => {
                self.body.update(cx, |input, _| input.set_content(""));
                self.form = fields
                    .iter()
                    .map(|field| {
                        KeyValueRow::new(cx, &field.key, &field.value, field.enabled, "Field")
                    })
                    .collect();
                if self.form.is_empty() {
                    self.form.push(KeyValueRow::new(cx, "", "", true, "Field"));
                }
            }
        }

        self.auth_kind = AuthKind::from_authentication(&request.authentication);
        self.preserved_auth = request.authentication.clone();
        match &request.authentication {
            Authentication::None => {
                self.auth_primary.update(cx, |i, _| i.set_content(""));
                self.auth_secondary.update(cx, |i, _| i.set_content(""));
            }
            Authentication::Basic { username, password } => {
                self.auth_primary
                    .update(cx, |i, _| i.set_content(username.clone()));
                self.auth_secondary
                    .update(cx, |i, _| i.set_content(password.clone()));
            }
            Authentication::Bearer { token } => {
                self.auth_primary
                    .update(cx, |i, _| i.set_content(token.clone()));
                self.auth_secondary.update(cx, |i, _| i.set_content(""));
            }
            Authentication::ApiKey {
                key,
                value,
                location,
            } => {
                self.auth_primary
                    .update(cx, |i, _| i.set_content(key.clone()));
                self.auth_secondary
                    .update(cx, |i, _| i.set_content(value.clone()));
                self.api_key_location = *location;
            }
            Authentication::Digest { username, password } => {
                self.auth_primary
                    .update(cx, |i, _| i.set_content(username.clone()));
                self.auth_secondary
                    .update(cx, |i, _| i.set_content(password.clone()));
            }
            // Edited in their own panels; the fields above stay untouched.
            Authentication::AwsSigV4 { .. } | Authentication::OAuth2 { .. } => {}
        }
    }

    /// Build the request from the current editor contents.
    pub fn to_request(&self, cx: &App) -> Request {
        let mut request = Request::new(self.method, self.url.read(cx).content().to_string());
        request.id = self.request_id;
        request.name = self.name.read(cx).content().to_string();

        request.query_params = self
            .params
            .iter()
            .filter(|row| row.is_populated(cx))
            .map(|row| QueryParameter {
                id: row.id,
                key: row.key_text(cx),
                value: row.value_text(cx),
                enabled: row.enabled,
            })
            .collect();

        request.headers = self
            .headers
            .iter()
            .filter(|row| row.is_populated(cx))
            .map(|row| HeaderEntry {
                id: row.id,
                key: row.key_text(cx),
                value: row.value_text(cx),
                enabled: row.enabled,
            })
            .collect();

        request.body = match self.body_kind {
            RequestBodyKind::None => RequestBody::None,
            RequestBodyKind::Json => RequestBody::Json {
                content: self.body.read(cx).content().to_string(),
            },
            RequestBodyKind::Raw => {
                let content_type = self.body_content_type.read(cx).content().to_string();
                RequestBody::Raw {
                    content: self.body.read(cx).content().to_string(),
                    content_type: if content_type.trim().is_empty() {
                        "text/plain".to_string()
                    } else {
                        content_type
                    },
                }
            }
            RequestBodyKind::FormUrlEncoded => RequestBody::FormUrlEncoded {
                fields: self
                    .form
                    .iter()
                    .filter(|row| row.is_populated(cx))
                    .map(|row| QueryParameter {
                        id: row.id,
                        key: row.key_text(cx),
                        value: row.value_text(cx),
                        enabled: row.enabled,
                    })
                    .collect(),
            },
        };

        let primary = self.auth_primary.read(cx).content().to_string();
        let secondary = self.auth_secondary.read(cx).content().to_string();
        request.authentication = match self.auth_kind {
            AuthKind::None => Authentication::None,
            AuthKind::Basic => Authentication::Basic {
                username: primary,
                password: secondary,
            },
            AuthKind::Bearer => Authentication::Bearer { token: primary },
            AuthKind::ApiKey => Authentication::ApiKey {
                key: primary,
                value: secondary,
                location: self.api_key_location,
            },
            AuthKind::Digest => Authentication::Digest {
                username: primary,
                password: secondary,
            },
            AuthKind::External => self.preserved_auth.clone(),
        };

        request
    }

    pub fn add_row(&mut self, tab: EditorTab, cx: &mut App) {
        let row = KeyValueRow::new(
            cx,
            "",
            "",
            true,
            if tab == EditorTab::Params {
                "Parameter"
            } else {
                "Header"
            },
        );
        match tab {
            EditorTab::Params => self.params.push(row),
            EditorTab::Headers => self.headers.push(row),
            _ => {}
        }
    }

    pub fn add_form_row(&mut self, cx: &mut App) {
        self.form.push(KeyValueRow::new(cx, "", "", true, "Field"));
    }

    pub fn remove_row(&mut self, tab: EditorTab, id: Uuid) {
        match tab {
            EditorTab::Params => self.params.retain(|row| row.id != id),
            EditorTab::Headers => self.headers.retain(|row| row.id != id),
            _ => {}
        }
    }

    pub fn remove_form_row(&mut self, id: Uuid) {
        self.form.retain(|row| row.id != id);
    }

    /// Number of populated, enabled rows, shown as a tab badge.
    pub fn count(&self, tab: EditorTab, cx: &App) -> usize {
        match tab {
            EditorTab::Params => self
                .params
                .iter()
                .filter(|row| row.enabled && !row.key_text(cx).trim().is_empty())
                .count(),
            EditorTab::Headers => self
                .headers
                .iter()
                .filter(|row| row.enabled && !row.key_text(cx).trim().is_empty())
                .count(),
            EditorTab::Body => usize::from(self.body_kind != RequestBodyKind::None),
            EditorTab::Auth => usize::from(self.auth_kind != AuthKind::None),
        }
    }

    /// Label for the tab strip: the name, or the URL host when unnamed.
    pub fn title(&self, cx: &App) -> String {
        let name = self.name.read(cx).content().trim().to_string();
        if !name.is_empty() {
            return name;
        }
        let url = self.url.read(cx).content().trim().to_string();
        if url.is_empty() {
            return "Untitled request".to_string();
        }
        let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(&url);
        requestlab_models::truncate_utf8(without_scheme, 48).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_labels_are_unique() {
        let labels: Vec<&str> = EditorTab::ALL.iter().map(|tab| tab.label()).collect();
        assert_eq!(labels, vec!["Params", "Headers", "Body", "Auth"]);
    }

    #[test]
    fn auth_kinds_map_from_authentication() {
        assert_eq!(
            AuthKind::from_authentication(&Authentication::None),
            AuthKind::None
        );
        assert_eq!(
            AuthKind::from_authentication(&Authentication::Bearer { token: "t".into() }),
            AuthKind::Bearer
        );
        assert_eq!(AuthKind::ApiKey.primary_label(), "Key");
        assert_eq!(AuthKind::Basic.secondary_label(), "Password");
    }

    #[test]
    fn saved_location_is_copy() {
        let location = SavedLocation {
            saved_request_id: Uuid::new_v4(),
            collection_id: Uuid::new_v4(),
            folder_id: None,
        };
        let copy = location;
        assert_eq!(copy, location);
    }
}
