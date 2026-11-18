//! Response inspection: bounded body views and in-response search.
//!
//! Every function here is pure and bounded, so the UI can call them on the
//! render thread without blocking on large payloads.

use crate::{BodyKind, HttpResponse};

/// How the response body is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyView {
    /// Structured view: pretty JSON, indented XML, formatted text.
    Pretty,
    /// Exactly what the server sent, as text.
    Raw,
    /// HTML source with a preview hint (rendering belongs to the UI).
    Html,
    /// XML source, indented when it can be parsed.
    Xml,
    /// Plain text, wrapped by the UI.
    Text,
}

impl BodyView {
    pub const ALL: [BodyView; 5] = [
        BodyView::Pretty,
        BodyView::Raw,
        BodyView::Html,
        BodyView::Xml,
        BodyView::Text,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BodyView::Pretty => "Pretty",
            BodyView::Raw => "Raw",
            BodyView::Html => "HTML",
            BodyView::Xml => "XML",
            BodyView::Text => "Text",
        }
    }
}

/// A rendered body, always bounded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyPreview {
    pub text: String,
    /// `true` when the view shows less than the stored body.
    pub truncated: bool,
    /// Why the view could not show everything, when that is worth saying.
    pub note: Option<String>,
}

/// The views that make sense for a response body.
pub fn available_views(response: &HttpResponse) -> Vec<BodyView> {
    let kind = response.body_kind();
    match kind {
        BodyKind::Empty => vec![BodyView::Raw],
        BodyKind::Json => vec![BodyView::Pretty, BodyView::Raw, BodyView::Text],
        BodyKind::Xml => vec![BodyView::Pretty, BodyView::Xml, BodyView::Raw],
        BodyKind::Html => vec![BodyView::Pretty, BodyView::Html, BodyView::Raw],
        BodyKind::Text => vec![BodyView::Pretty, BodyView::Raw, BodyView::Text],
        BodyKind::Binary => vec![BodyView::Raw],
    }
}

fn truncate(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_string(), false);
    }
    let cut: String = text.chars().take(max_chars).collect();
    (cut, true)
}

/// Render the body in `view`, never returning more than `max_chars` characters.
pub fn body_preview(response: &HttpResponse, view: BodyView, max_chars: usize) -> BodyPreview {
    let kind = response.body_kind();

    if kind == BodyKind::Binary {
        let size = response.body.len();
        return BodyPreview {
            text: format!(
                "Binary response, {size} bytes. Use Download to save it, or Raw to inspect the first bytes."
            ),
            truncated: false,
            note: Some("binary".to_string()),
        };
    }

    let Some(text) = response.text() else {
        return BodyPreview {
            text: format!("The response has no body ({} bytes).", response.body.len()),
            truncated: false,
            note: None,
        };
    };

    let rendered = match (view, kind) {
        (BodyView::Pretty, BodyKind::Json) => match response.pretty_json() {
            Some(Ok(pretty)) => pretty,
            Some(Err(error)) => {
                // A server that claims JSON but sends something else is shown
                // as-is with the parse error, not hidden behind a blank pane.
                let (text, truncated) = truncate(text, max_chars);
                return BodyPreview {
                    text,
                    truncated,
                    note: Some(error),
                };
            }
            None => text.to_string(),
        },
        (BodyView::Pretty, BodyKind::Xml) | (BodyView::Xml, _) => indent_markup(text),
        (BodyView::Html, _) => text.to_string(),
        (BodyView::Text, _) => text.to_string(),
        (BodyView::Raw, _) => text.to_string(),
        (BodyView::Pretty, _) => text.to_string(),
    };

    let (text, truncated) = truncate(&rendered, max_chars);
    BodyPreview {
        text,
        truncated,
        note: if truncated {
            Some(format!(
                "Showing the first {max_chars} characters of {}.",
                response.body.len()
            ))
        } else {
            None
        },
    }
}

/// Indent markup for readability without validating it.
///
/// Deliberately simple: it never rewrites the content, only inserts line breaks
/// and indentation between tags that are already on their own.
pub fn indent_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    let mut depth = 0usize;
    let mut index = 0;
    let bytes = text.as_bytes();
    while index < bytes.len() {
        if bytes[index] == b'<' {
            let end = match text[index..].find('>') {
                Some(offset) => index + offset,
                None => break,
            };
            let tag = &text[index..=end];
            let closing = tag.starts_with("</");
            let self_closing =
                tag.ends_with("/>") || tag.starts_with("<?") || tag.starts_with("<!");
            if closing {
                depth = depth.saturating_sub(1);
            }
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&"  ".repeat(depth));
            out.push_str(tag);
            if !closing && !self_closing {
                depth += 1;
            }
            index = end + 1;
        } else {
            let next = text[index..].find('<').map(|offset| index + offset);
            let chunk_end = next.unwrap_or(bytes.len());
            let chunk = text[index..chunk_end].trim();
            if !chunk.is_empty() {
                if !out.is_empty() && !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&"  ".repeat(depth));
                out.push_str(chunk);
            }
            index = chunk_end;
        }
    }
    out
}

/// One match of a query inside a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// 1-based line number.
    pub line: usize,
    /// 0-based character column.
    pub column: usize,
    /// A bounded excerpt around the match, without newlines.
    pub preview: String,
}

/// Result of searching a body.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    /// Total matches, which can exceed `hits.len()` when capped.
    pub total: usize,
    pub truncated: bool,
}

/// Search a body, returning at most `max_hits` hits with bounded previews.
///
/// Matching is literal (no regex), case-insensitive unless `case_sensitive`.
pub fn search(text: &str, query: &str, case_sensitive: bool, max_hits: usize) -> SearchResult {
    let query = query.trim();
    if query.is_empty() || max_hits == 0 {
        return SearchResult::default();
    }
    let needle = if case_sensitive {
        query.to_string()
    } else {
        query.to_lowercase()
    };

    let mut hits = Vec::new();
    let mut total = 0usize;
    for (line_index, line) in text.lines().enumerate() {
        let haystack = if case_sensitive {
            line.to_string()
        } else {
            line.to_lowercase()
        };
        let mut from = 0;
        while let Some(offset) = haystack[from..].find(&needle) {
            let start = from + offset;
            total += 1;
            if hits.len() < max_hits {
                hits.push(SearchHit {
                    line: line_index + 1,
                    column: line[..start].chars().count(),
                    preview: excerpt(line, start, needle.chars().count()),
                });
            }
            from = start + needle.len().max(1);
        }
    }

    let truncated = total > hits.len();
    SearchResult {
        hits,
        total,
        truncated,
    }
}

/// At most 60 characters either side of the match, with the match in place.
fn excerpt(line: &str, start: usize, length: usize) -> String {
    const WINDOW: usize = 60;
    let chars: Vec<char> = line.chars().collect();
    let start_chars = line[..start].chars().count();
    let from = start_chars.saturating_sub(WINDOW);
    let to = (start_chars + length + WINDOW).min(chars.len());
    let mut out = String::new();
    if from > 0 {
        out.push('…');
    }
    out.extend(&chars[from..to]);
    if to < chars.len() {
        out.push('…');
    }
    out
}

/// A filename for the Download action.
pub fn download_name(response: &HttpResponse) -> String {
    if let Some(name) = response.header("content-disposition").and_then(|value| {
        value.split(';').find_map(|part| {
            part.trim()
                .strip_prefix("filename=")
                .map(|name| name.trim_matches('"').to_string())
        })
    }) {
        if !name.trim().is_empty() && !name.contains(['/', '\\']) {
            return name;
        }
    }
    let extension = match response.body_kind() {
        BodyKind::Json => "json",
        BodyKind::Xml => "xml",
        BodyKind::Html => "html",
        BodyKind::Text => "txt",
        BodyKind::Binary => "bin",
        BodyKind::Empty => "bin",
    };
    format!("response.{extension}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ResponseBody, ResponseHeader};

    fn response(content_type: &str, body: ResponseBody) -> HttpResponse {
        HttpResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![ResponseHeader {
                name: "Content-Type".into(),
                value: content_type.into(),
            }],
            body,
            content_type: Some(content_type.into()),
            size_bytes: 0,
            duration_ms: 1,
            completed_at: "t".into(),
            url: "https://example.com".into(),
            cookies: Vec::new(),
        }
    }

    #[test]
    fn pretty_json_is_indented_and_bounded() {
        let response = response("application/json", ResponseBody::text("{\"a\":[1,2]}"));
        let preview = body_preview(&response, BodyView::Pretty, 10_000);
        assert!(preview.text.contains("\n"));
        assert!(!preview.truncated);

        let bounded = body_preview(&response, BodyView::Pretty, 5);
        assert_eq!(bounded.text.chars().count(), 5);
        assert!(bounded.truncated);
        assert!(bounded.note.unwrap().contains("first 5 characters"));
    }

    #[test]
    fn a_large_body_is_cut_not_formatted_into_a_hang() {
        let items: Vec<String> = (0..200_000)
            .map(|index| format!("{{\"i\":{index}}}"))
            .collect();
        let body = format!("[{}]", items.join(","));
        assert!(body.len() > 2_000_000);
        let response = response("application/json", ResponseBody::text(body));
        let preview = body_preview(&response, BodyView::Raw, 4_000);
        assert_eq!(preview.text.chars().count(), 4_000);
        assert!(preview.truncated);
    }

    #[test]
    fn invalid_json_is_shown_with_the_parse_error() {
        let response = response("application/json", ResponseBody::text("{\"a\":}"));
        let preview = body_preview(&response, BodyView::Pretty, 1_000);
        assert!(preview.note.unwrap().contains("not valid JSON"));
        assert_eq!(preview.text, "{\"a\":}");
    }

    #[test]
    fn binary_bodies_are_described_not_dumped() {
        let response = response(
            "application/octet-stream",
            ResponseBody::binary(vec![0, 1, 2]),
        );
        let preview = body_preview(&response, BodyView::Raw, 1_000);
        assert!(preview.text.contains("Binary response"));
        assert!(preview.text.contains("3 bytes"));
        assert_eq!(available_views(&response), vec![BodyView::Raw]);
    }

    #[test]
    fn views_follow_the_content_type() {
        let json = response("application/json", ResponseBody::text("{}"));
        assert_eq!(
            available_views(&json),
            vec![BodyView::Pretty, BodyView::Raw, BodyView::Text]
        );
        let html = response("text/html", ResponseBody::text("<p/>"));
        assert!(available_views(&html).contains(&BodyView::Html));
    }

    #[test]
    fn markup_is_indented_without_changing_content() {
        let indented = indent_markup("<root><item id=\"1\">text</item></root>");
        assert_eq!(
            indented,
            "<root>\n  <item id=\"1\">\n    text\n  </item>\n</root>"
        );
        // Self-closing tags do not open a level.
        assert_eq!(indent_markup("<a/><b/>"), "<a/>\n<b/>");
    }

    #[test]
    fn search_reports_line_column_and_counts() {
        let body = "{\n  \"name\": \"Ada\",\n  \"note\": \"ada again\"\n}";
        let result = search(body, "ada", false, 10);
        assert_eq!(result.total, 2);
        assert_eq!(result.hits.len(), 2);
        assert_eq!(result.hits[0].line, 2);
        assert_eq!(result.hits[0].column, 11);
        assert!(result.hits[1].preview.contains("ada again"));
        assert!(!result.truncated);

        assert_eq!(search(body, "Ada", true, 10).total, 1);
        assert_eq!(search(body, "zzz", false, 10).total, 0);
        assert_eq!(search(body, "  ", false, 10).total, 0);
    }

    #[test]
    fn search_is_capped_and_says_so() {
        let body = "match\n".repeat(50);
        let result = search(&body, "match", false, 5);
        assert_eq!(result.hits.len(), 5);
        assert_eq!(result.total, 50);
        assert!(result.truncated);
    }

    #[test]
    fn search_previews_stay_small() {
        let long = "x".repeat(500) + "needle" + &"y".repeat(500);
        let result = search(&long, "needle", false, 1);
        assert!(result.hits[0].preview.chars().count() <= 130);
        assert!(result.hits[0].preview.contains("needle"));
        assert!(result.hits[0].preview.starts_with('…'));
    }

    #[test]
    fn download_names_prefer_content_disposition() {
        let mut response = response("application/json", ResponseBody::text("{}"));
        assert_eq!(download_name(&response), "response.json");
        response.headers.push(ResponseHeader {
            name: "Content-Disposition".into(),
            value: "attachment; filename=\"report.json\"".into(),
        });
        assert_eq!(download_name(&response), "report.json");
        // Path separators are refused so a header cannot escape the folder.
        response.headers.last_mut().unwrap().value =
            "attachment; filename=\"../../etc/passwd\"".into();
        assert_eq!(download_name(&response), "response.json");
    }
}
