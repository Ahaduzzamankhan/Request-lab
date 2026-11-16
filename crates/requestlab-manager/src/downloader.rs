//! Streaming downloads with progress, cancellation, resume and verification.
//!
//! Data is written to `<file>.part` as it arrives, hashed incrementally, and only
//! renamed into place once size and SHA-256 match. A corrupt file is removed
//! rather than resumed from.

use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::header::RANGE;
use reqwest::StatusCode;
use sha2::{Digest, Sha256};
use url::Url;

use crate::error::{ManagerError, ManagerResult};
use crate::events::Transfer;

/// Cancellation flag.
pub type CancelToken = Arc<AtomicBool>;

/// A fresh, uncancelled token.
pub fn cancel_token() -> CancelToken {
    Arc::new(AtomicBool::new(false))
}

/// Enforce HTTPS, allowing loopback so local release servers can be tested
/// without weakening transport security anywhere else.
pub fn require_https(url: &str, what: &str) -> ManagerResult<()> {
    let parsed = Url::parse(url)
        .map_err(|error| ManagerError::Other(format!("{what} URL '{url}' is invalid: {error}")))?;
    let loopback = parsed.host_str().is_some_and(|host| {
        let host = host.trim_start_matches('[').trim_end_matches(']');
        host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
    });
    match parsed.scheme() {
        "https" => Ok(()),
        "http" if loopback => Ok(()),
        "http" => Err(ManagerError::Other(format!(
            "{what} URL must use https, got '{url}'"
        ))),
        scheme => Err(ManagerError::Other(format!(
            "{what} URL uses unsupported scheme '{scheme}'"
        ))),
    }
}

/// What the signed manifest says a package must be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expected {
    pub size: u64,
    pub sha256: String,
}

/// Result of a successful download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadOutcome {
    pub path: PathBuf,
    pub size: u64,
    pub sha256: String,
    pub resumed: bool,
}

/// Default ceiling for one package.
pub const DEFAULT_MAX_BYTES: u64 = 1024 * 1024 * 1024;

/// Downloads packages to disk.
#[derive(Debug, Clone)]
pub struct Downloader {
    http: reqwest::Client,
    max_bytes: u64,
    progress_interval: Duration,
}

impl Downloader {
    pub fn new() -> ManagerResult<Self> {
        let http = reqwest::Client::builder()
            .user_agent(crate::github::user_agent())
            .connect_timeout(Duration::from_secs(15))
            .build()
            .map_err(|error| {
                ManagerError::Network(format!("cannot build the HTTP client: {error}"))
            })?;
        Ok(Self {
            http,
            max_bytes: DEFAULT_MAX_BYTES,
            progress_interval: Duration::from_millis(100),
        })
    }

    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// Download `url` to `destination`, verifying it when expectations are given.
    pub async fn download(
        &self,
        url: &str,
        destination: &Path,
        expected: Option<&Expected>,
        cancel: Option<&CancelToken>,
        progress: &mut dyn FnMut(Transfer),
    ) -> ManagerResult<DownloadOutcome> {
        require_https(url, "package")?;
        if let Some(expected) = expected {
            if expected.size > self.max_bytes {
                return Err(ManagerError::Other(format!(
                    "package is {} bytes, above the {} byte limit",
                    expected.size, self.max_bytes
                )));
            }
        }
        let limit = expected
            .map(|e| e.size)
            .unwrap_or(self.max_bytes)
            .min(self.max_bytes);

        if let Some(parent) = destination.parent() {
            crate::paths::ensure_dir(parent)?;
        }
        let part = part_path(destination);

        let mut offset = std::fs::metadata(&part)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        if let Some(expected) = expected {
            if offset > expected.size {
                crate::paths::remove_file_if_exists(&part)?;
                offset = 0;
            }
        }

        let mut request = self.http.get(url);
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }
        let response = request
            .send()
            .await
            .map_err(|error| ManagerError::Network(format!("cannot download {url}: {error}")))?;

        let status = response.status();
        if !status.is_success() {
            if status == StatusCode::RANGE_NOT_SATISFIABLE && offset > 0 {
                crate::paths::remove_file_if_exists(&part)?;
                return Err(ManagerError::Network(
                    "the server rejected the resume request; retry the download".to_string(),
                ));
            }
            return Err(ManagerError::Network(format!(
                "download of {url} failed with HTTP {status}"
            )));
        }

        let append = status == StatusCode::PARTIAL_CONTENT && offset > 0;
        if !append {
            offset = 0;
        }
        let from_response = response.content_length();
        let total = expected.map(|e| e.size).or_else(|| {
            if append {
                from_response.map(|length| length + offset)
            } else {
                from_response
            }
        });
        if let Some(total) = total {
            if total > limit {
                return Err(ManagerError::Other(format!(
                    "download is {total} bytes, larger than the allowed {limit} bytes"
                )));
            }
        }

        let mut file = if append {
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&part)
                .map_err(|error| ManagerError::io(&part, error))?;
            file.seek(SeekFrom::End(0))
                .map_err(|error| ManagerError::io(&part, error))?;
            file
        } else {
            std::fs::File::create(&part).map_err(|error| ManagerError::io(&part, error))?
        };

        let mut hasher = Sha256::new();
        if offset > 0 {
            let mut existing =
                std::fs::File::open(&part).map_err(|error| ManagerError::io(&part, error))?;
            let mut buffer = vec![0u8; 128 * 1024];
            let mut hashed = 0u64;
            while hashed < offset {
                let want = ((offset - hashed) as usize).min(buffer.len());
                let read = existing
                    .read(&mut buffer[..want])
                    .map_err(|error| ManagerError::io(&part, error))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                hashed += read as u64;
            }
        }

        let started = Instant::now();
        let mut downloaded = offset;
        let mut last_report = started;
        progress(Transfer {
            downloaded,
            total,
            bytes_per_second: 0.0,
        });

        let mut response = response;
        loop {
            if cancel.is_some_and(|token| token.load(Ordering::Relaxed)) {
                file.flush()
                    .map_err(|error| ManagerError::io(&part, error))?;
                return Err(ManagerError::Cancelled);
            }
            let chunk = match response.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break,
                Err(error) => {
                    if cancel.is_some_and(|token| token.load(Ordering::Relaxed)) {
                        return Err(ManagerError::Cancelled);
                    }
                    return Err(ManagerError::Network(format!(
                        "download interrupted after {downloaded} bytes: {error}"
                    )));
                }
            };
            downloaded += chunk.len() as u64;
            if downloaded > limit {
                crate::paths::remove_file_if_exists(&part)?;
                return Err(ManagerError::Other(format!(
                    "download exceeded the allowed {limit} bytes; it does not match the manifest"
                )));
            }
            file.write_all(&chunk)
                .map_err(|error| ManagerError::io(&part, error))?;
            hasher.update(&chunk);

            if last_report.elapsed() >= self.progress_interval {
                last_report = Instant::now();
                progress(Transfer {
                    downloaded,
                    total,
                    bytes_per_second: rate(downloaded, started),
                });
            }
        }

        file.flush()
            .map_err(|error| ManagerError::io(&part, error))?;
        file.sync_all()
            .map_err(|error| ManagerError::io(&part, error))?;
        drop(file);

        let actual_sha256 = crate::verifier::hex_encode(&hasher.finalize());
        progress(Transfer {
            downloaded,
            total,
            bytes_per_second: rate(downloaded, started),
        });

        if cancel.is_some_and(|token| token.load(Ordering::Relaxed)) {
            return Err(ManagerError::Cancelled);
        }

        if let Some(expected) = expected {
            if downloaded != expected.size {
                crate::paths::remove_file_if_exists(&part)?;
                return Err(ManagerError::SizeMismatch {
                    file: destination.display().to_string(),
                    expected: expected.size,
                    actual: downloaded,
                });
            }
            if actual_sha256 != expected.sha256 {
                crate::paths::remove_file_if_exists(&part)?;
                return Err(ManagerError::Checksum {
                    file: destination.display().to_string(),
                    expected: expected.sha256.clone(),
                    actual: actual_sha256,
                });
            }
        } else if let Some(total) = total {
            if downloaded != total {
                crate::paths::remove_file_if_exists(&part)?;
                return Err(ManagerError::Network(format!(
                    "download ended after {downloaded} of {total} bytes"
                )));
            }
        }

        if destination.exists() {
            crate::paths::remove_file_if_exists(destination)?;
        }
        std::fs::rename(&part, destination)
            .map_err(|error| ManagerError::io(destination, error))?;

        Ok(DownloadOutcome {
            path: destination.to_path_buf(),
            size: downloaded,
            sha256: actual_sha256,
            resumed: offset > 0,
        })
    }
}

fn part_path(destination: &Path) -> PathBuf {
    let mut name = destination
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    destination.with_file_name(name)
}

fn rate(downloaded: u64, started: Instant) -> f64 {
    let seconds = started.elapsed().as_secs_f64();
    if seconds <= 0.0 {
        0.0
    } else {
        downloaded as f64 / seconds
    }
}

/// Human-readable byte count.
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Human-readable transfer rate.
pub fn human_rate(bytes_per_second: f64) -> String {
    if !bytes_per_second.is_finite() || bytes_per_second <= 0.0 {
        "-".to_string()
    } else {
        format!("{}/s", human_bytes(bytes_per_second.round() as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn serve_once(response: Vec<u8>) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 4096];
                let _ = std::io::Read::read(&mut stream, &mut buffer);
                let _ = stream.write_all(&response);
            }
        });
        (format!("http://127.0.0.1:{port}/file.zip"), handle)
    }

    #[test]
    fn human_sizes_and_rates_are_readable() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1024), "1.0 KiB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MiB");
        assert_eq!(human_rate(0.0), "-");
        assert_eq!(human_rate(2048.0), "2.0 KiB/s");
    }

    #[tokio::test]
    async fn downloads_and_verifies_bytes() {
        let payload = b"requestlab package bytes".to_vec();
        let digest = crate::verifier::sha256_hex(&payload);
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            payload.len()
        )
        .into_bytes();
        response.extend_from_slice(&payload);
        let (url, server) = serve_once(response);

        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("package.zip");
        let expected = Expected {
            size: payload.len() as u64,
            sha256: digest.clone(),
        };
        let downloader = Downloader::new().unwrap();
        let mut reports = 0;
        let outcome = downloader
            .download(&url, &destination, Some(&expected), None, &mut |_| {
                reports += 1
            })
            .await
            .unwrap();
        server.join().unwrap();

        assert_eq!(outcome.size, payload.len() as u64);
        assert_eq!(outcome.sha256, digest);
        assert_eq!(std::fs::read(&destination).unwrap(), payload);
        assert!(reports >= 2);
        assert!(!temp.path().join("package.zip.part").exists());
    }

    #[tokio::test]
    async fn rejects_corrupt_payload_and_cleans_up() {
        let payload = b"corrupt".to_vec();
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            payload.len()
        )
        .into_bytes();
        response.extend_from_slice(&payload);
        let (url, server) = serve_once(response);

        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("package.zip");
        let expected = Expected {
            size: payload.len() as u64,
            sha256: crate::verifier::sha256_hex(b"other"),
        };
        let error = Downloader::new()
            .unwrap()
            .download(&url, &destination, Some(&expected), None, &mut |_| {})
            .await
            .unwrap_err();
        server.join().unwrap();

        assert!(matches!(error, ManagerError::Checksum { .. }), "{error}");
        assert!(!destination.exists());
        assert!(!temp.path().join("package.zip.part").exists());
    }

    #[tokio::test]
    async fn refuses_plain_http_off_loopback() {
        let temp = tempfile::tempdir().unwrap();
        let error = Downloader::new()
            .unwrap()
            .download(
                "http://example.com/file.zip",
                &temp.path().join("file.zip"),
                None,
                None,
                &mut |_| {},
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("https"), "{error}");
    }

    #[tokio::test]
    async fn cancellation_keeps_the_partial_file() {
        // A slow response lets a separate thread cancel mid-transfer, which is
        // exactly what the installer and updater cancel buttons do.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 4096];
                let _ = std::io::Read::read(&mut stream, &mut buffer);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 1000000\r\nConnection: close\r\n\r\n",
                );
                for _ in 0..200 {
                    if stream.write_all(&[0u8; 1024]).is_err() {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }
        });

        let temp = tempfile::tempdir().unwrap();
        let destination = temp.path().join("slow.zip");
        let cancel = cancel_token();
        let flag = cancel.clone();
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(80));
            flag.store(true, Ordering::Relaxed);
        });
        let result = Downloader::new()
            .unwrap()
            .download(
                &format!("http://127.0.0.1:{port}/slow.zip"),
                &destination,
                None,
                Some(&cancel),
                &mut |_| {},
            )
            .await;
        canceller.join().unwrap();
        server.join().unwrap();

        assert!(
            matches!(result, Err(ManagerError::Cancelled)),
            "{:?}",
            result.err()
        );
        assert!(temp.path().join("slow.zip.part").exists());
        assert!(!destination.exists());
    }
}
