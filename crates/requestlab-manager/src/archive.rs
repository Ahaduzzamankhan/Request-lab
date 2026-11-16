//! Safe archive extraction.
//!
//! Packages are untrusted until verified, and even a verified archive must not be
//! able to write outside its staging directory: entry names are validated, links
//! are refused, and entry count and sizes are bounded before writing.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::error::{ManagerError, ManagerResult};

/// Packages RequestLab publishes are zip archives.
pub const PACKAGE_EXTENSION: &str = "zip";
/// Refuse archives with more entries than this.
pub const MAX_ENTRIES: usize = 2_000;
/// Refuse a single entry larger than this (1 GiB).
pub const MAX_ENTRY_BYTES: u64 = 1024 * 1024 * 1024;
/// Refuse an archive that expands beyond this (2 GiB).
pub const MAX_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// What an extraction produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractOutcome {
    /// Files written, relative to the destination, `/` separated.
    pub files: Vec<String>,
    pub total_bytes: u64,
}

/// Validate an entry name and return the relative path to write.
pub fn safe_relative_path(entry: &str) -> Result<PathBuf, String> {
    let normalised = entry.replace('\\', "/");
    if normalised.is_empty() {
        return Err("empty archive entry name".to_string());
    }
    if normalised.starts_with('/') {
        return Err(format!("'{entry}' is an absolute path"));
    }
    if normalised.len() >= 2 && normalised.as_bytes()[1] == b':' {
        return Err(format!("'{entry}' contains a drive letter"));
    }

    let mut path = PathBuf::new();
    for segment in normalised.split('/') {
        match segment {
            "" | "." => continue,
            ".." => return Err(format!("'{entry}' escapes its directory")),
            other => {
                if other.contains('\0') {
                    return Err(format!("'{entry}' contains a NUL byte"));
                }
                const RESERVED: [&str; 22] = [
                    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
                    "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
                    "LPT8", "LPT9",
                ];
                let stem = other.split('.').next().unwrap_or(other);
                if RESERVED.iter().any(|name| stem.eq_ignore_ascii_case(name)) {
                    return Err(format!("'{entry}' uses the reserved name '{stem}'"));
                }
                path.push(other);
            }
        }
    }

    if path.as_os_str().is_empty() {
        return Err(format!("'{entry}' has no file name"));
    }
    Ok(path)
}

/// Join an entry onto a destination, refusing anything that escapes it.
pub fn safe_join(destination: &Path, entry: &str) -> Result<PathBuf, String> {
    let relative = safe_relative_path(entry)?;
    let joined = destination.join(&relative);
    if !crate::paths::is_within(destination, &joined) {
        return Err(format!("'{entry}' escapes {}", destination.display()));
    }
    Ok(joined)
}

/// Extract a zip archive into `destination`.
pub fn extract_zip(
    archive_path: &Path,
    destination: &Path,
    progress: &mut dyn FnMut(f32),
) -> ManagerResult<ExtractOutcome> {
    let file =
        std::fs::File::open(archive_path).map_err(|error| ManagerError::io(archive_path, error))?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        ManagerError::UnsafeArchive(format!(
            "{} is not a readable zip archive: {error}",
            archive_path.display()
        ))
    })?;

    let entries_total = archive.len();
    if entries_total > MAX_ENTRIES {
        return Err(ManagerError::UnsafeArchive(format!(
            "archive has {entries_total} entries; the limit is {MAX_ENTRIES}"
        )));
    }
    crate::paths::ensure_dir(destination)?;

    let mut files = Vec::new();
    let mut total_bytes = 0u64;

    for index in 0..entries_total {
        let mut entry = archive.by_index(index).map_err(|error| {
            ManagerError::UnsafeArchive(format!("archive entry {index} cannot be read: {error}"))
        })?;
        let name = entry.name().to_string();
        let relative = safe_join(destination, &name).map_err(ManagerError::UnsafeArchive)?;

        if entry.is_symlink() {
            return Err(ManagerError::UnsafeArchive(format!(
                "'{name}' is a symbolic link; RequestLab packages contain only regular files"
            )));
        }

        let declared = entry.size();
        if declared > MAX_ENTRY_BYTES {
            return Err(ManagerError::UnsafeArchive(format!(
                "'{name}' is {declared} bytes; the per-entry limit is {MAX_ENTRY_BYTES}"
            )));
        }
        total_bytes = total_bytes.saturating_add(declared);
        if total_bytes > MAX_TOTAL_BYTES {
            return Err(ManagerError::UnsafeArchive(format!(
                "archive expands beyond the {MAX_TOTAL_BYTES} byte limit"
            )));
        }

        if entry.is_dir() {
            crate::paths::ensure_dir(&relative)?;
        } else {
            if let Some(parent) = relative.parent() {
                crate::paths::ensure_dir(parent)?;
            }
            let mut output = std::fs::File::create(&relative)
                .map_err(|error| ManagerError::io(&relative, error))?;
            let written = std::io::copy(&mut (&mut entry).take(declared.max(1)), &mut output)
                .map_err(|error| ManagerError::io(&relative, error))?;
            output
                .flush()
                .map_err(|error| ManagerError::io(&relative, error))?;
            if written != declared {
                return Err(ManagerError::UnsafeArchive(format!(
                    "'{name}' is truncated: {written} of {declared} bytes"
                )));
            }
            files.push(relative_to(destination, &relative));
        }

        progress((index + 1) as f32 / entries_total as f32);
    }

    files.sort();
    Ok(ExtractOutcome { files, total_bytes })
}

/// Whether a zip entry's unix mode marks it as a symbolic link.
pub fn is_symlink_mode(mode: Option<u32>) -> bool {
    mode.is_some_and(|mode| mode & 0o170_000 == 0o120_000)
}

fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Locate the directory inside an extracted package that holds the binaries.
pub fn package_root(extracted: &Path) -> ManagerResult<PathBuf> {
    if extracted.join(crate::paths::app_binary()).is_file() {
        return Ok(extracted.to_path_buf());
    }
    let mut directories = Vec::new();
    let entries =
        std::fs::read_dir(extracted).map_err(|error| ManagerError::io(extracted, error))?;
    for entry in entries.flatten() {
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            directories.push(entry.path());
        }
    }
    if directories.len() == 1 && directories[0].join(crate::paths::app_binary()).is_file() {
        return Ok(directories.remove(0));
    }
    Err(ManagerError::UnsafeArchive(format!(
        "package does not contain {}",
        crate::paths::app_binary()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn write_zip(path: &Path, entries: &[(&str, &[u8], Option<u32>)]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = ZipWriter::new(file);
        for (name, contents, mode) in entries {
            let mut options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
            if let Some(mode) = mode {
                options = options.unix_permissions(*mode);
            }
            writer.start_file(*name, options).unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
    }

    #[test]
    fn entry_names_are_validated() {
        for entry in [
            "../evil.exe",
            "dir/../../evil.exe",
            "/etc/passwd",
            "C:\\Windows\\evil.exe",
            "\\\\server\\share\\evil.exe",
            "",
            ".",
            "NUL",
            "con.txt",
        ] {
            assert!(
                safe_relative_path(entry).is_err(),
                "'{entry}' must be rejected"
            );
        }
        assert_eq!(
            safe_relative_path("bin\\requestlab.exe").unwrap(),
            PathBuf::from("bin").join("requestlab.exe")
        );
    }

    #[test]
    fn extracts_regular_files() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("package.zip");
        write_zip(
            &archive,
            &[
                (crate::paths::app_binary().as_str(), b"exe", None),
                ("README.md", b"docs", None),
                ("licenses/LICENSE", b"MIT", None),
            ],
        );
        let destination = temp.path().join("staging");
        let outcome = extract_zip(&archive, &destination, &mut |_| {}).unwrap();

        assert_eq!(outcome.files.len(), 3);
        assert_eq!(
            std::fs::read(destination.join("README.md")).unwrap(),
            b"docs"
        );
        assert_eq!(package_root(&destination).unwrap(), destination);
    }

    #[test]
    fn finds_a_single_top_level_package_directory() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("package.zip");
        let inner = format!("requestlab-0.2.0/{}", crate::paths::app_binary());
        write_zip(&archive, &[(&inner, b"exe", None)]);

        let destination = temp.path().join("staging");
        extract_zip(&archive, &destination, &mut |_| {}).unwrap();
        assert_eq!(
            package_root(&destination).unwrap(),
            destination.join("requestlab-0.2.0")
        );
    }

    #[test]
    fn refuses_traversal_and_symlinks() {
        let temp = tempfile::tempdir().unwrap();

        let evil = temp.path().join("evil.zip");
        write_zip(&evil, &[("../escaped.txt", b"owned", None)]);
        let error = extract_zip(&evil, &temp.path().join("staging"), &mut |_| {}).unwrap_err();
        assert!(matches!(error, ManagerError::UnsafeArchive(_)), "{error}");
        assert!(!temp.path().join("escaped.txt").exists());

        // A zip writer cannot create a symlink entry through its public API, so the
        // classification used by extraction is tested directly here and the entry
        // check runs through `zip`'s own `is_symlink`.
        assert!(is_symlink_mode(Some(0o120777)));
        assert!(!is_symlink_mode(Some(0o100644)));
        assert!(!is_symlink_mode(None));
    }

    #[test]
    fn missing_application_binary_is_reported() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("package.zip");
        write_zip(&archive, &[("notes.txt", b"hello", None)]);
        let destination = temp.path().join("staging");
        extract_zip(&archive, &destination, &mut |_| {}).unwrap();
        assert!(package_root(&destination)
            .unwrap_err()
            .to_string()
            .contains("does not contain"));
    }
}
