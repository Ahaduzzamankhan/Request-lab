//! Response capture: headers, bounded body streaming, cookies and classification.

use std::time::Instant;

use requestlab_models::{
    classify_body, now_rfc3339, BodyKind, Cookie, HttpResponse, ResponseBody, ResponseHeader,
};

use crate::errors::HttpError;

/// Stream a Reqwest response into a bounded [`HttpResponse`].
///
/// The body is read in chunks and capped at `max_bytes`; whatever arrives after
/// the cap is discarded and the response is flagged as truncated. This keeps a
/// multi-gigabyte download from freezing the UI or exhausting memory.
pub async fn capture(
    response: reqwest::Response,
    max_bytes: usize,
    started: Instant,
) -> Result<HttpResponse, HttpError> {
    let status = response.status();
    let url = response.url().to_string();
    let headers: Vec<ResponseHeader> = response
        .headers()
        .iter()
        .map(|(name, value)| ResponseHeader {
            name: name.as_str().to_string(),
            value: value.to_str().unwrap_or("<binary>").to_string(),
        })
        .collect();

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);

    let mut cookies = Vec::new();
    for value in response.headers().get_all(reqwest::header::SET_COOKIE) {
        if let Ok(text) = value.to_str() {
            if let Some(cookie) = Cookie::parse_set_cookie(text) {
                cookies.push(cookie);
            }
        }
    }

    let mut response = response;
    let mut buffer: Vec<u8> = Vec::new();
    let mut received: usize = 0;
    let mut truncated = false;

    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(err) => return Err(HttpError::from_reqwest(err)),
        };
        received += chunk.len();
        if buffer.len() < max_bytes {
            let remaining = max_bytes - buffer.len();
            if chunk.len() <= remaining {
                buffer.extend_from_slice(&chunk);
            } else {
                buffer.extend_from_slice(&chunk[..remaining]);
                truncated = true;
            }
        } else {
            truncated = true;
        }
        if truncated {
            break;
        }
    }

    let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;

    let body = match classify_body(content_type.as_deref(), &buffer) {
        BodyKind::Empty => ResponseBody::Empty,
        kind if kind.is_textual() => match String::from_utf8(buffer) {
            Ok(text) => ResponseBody::Text { text, truncated },
            Err(err) => ResponseBody::Binary {
                bytes: err.into_bytes(),
                truncated,
            },
        },
        _ => ResponseBody::Binary {
            bytes: buffer,
            truncated,
        },
    };

    Ok(HttpResponse {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        headers,
        body,
        content_type,
        size_bytes: received,
        duration_ms,
        completed_at: now_rfc3339(),
        url,
        cookies,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use requestlab_models::human_bytes;

    #[test]
    fn truncation_flag_is_reported() {
        let body = ResponseBody::Text {
            text: "abc".into(),
            truncated: true,
        };
        assert!(body.is_truncated());
        assert_eq!(body.len(), 3);
    }

    #[test]
    fn size_labels_are_human_readable() {
        assert_eq!(human_bytes(2048), "2.0 KB");
    }
}
