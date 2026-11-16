//! The signed release manifest published with every RequestLab release.
//!
//! GitHub release metadata is a pointer, never proof. The manifest lists the
//! exact package per OS/architecture with its size and SHA-256, and is only
//! trusted after its Ed25519 signature verifies against a trusted key.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::version::Version;

/// Manifest asset name.
pub const MANIFEST_ASSET: &str = "requestlab-manifest.json";
/// Detached signature over the exact manifest bytes.
pub const SIGNATURE_ASSET: &str = "requestlab-manifest.json.sig";
/// Schema this build understands.
pub const SCHEMA_VERSION: u32 = 1;
/// Application identifier every manifest must carry.
pub const APP_ID: &str = "requestlab";

/// Release channel; beta releases are GitHub prereleases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
}

impl Channel {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim() {
            "stable" => Ok(Channel::Stable),
            "beta" => Ok(Channel::Beta),
            other => Err(format!(
                "unknown release channel '{other}' (expected stable or beta)"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
        }
    }

    /// Whether a release belongs to this channel.
    pub fn accepts(self, version: &Version, prerelease_flag: bool) -> bool {
        match self {
            Channel::Stable => !prerelease_flag && !version.is_prerelease(),
            Channel::Beta => true,
        }
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Target operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperatingSystem {
    Windows,
    Linux,
    Macos,
}

impl OperatingSystem {
    pub fn as_str(self) -> &'static str {
        match self {
            OperatingSystem::Windows => "windows",
            OperatingSystem::Linux => "linux",
            OperatingSystem::Macos => "macos",
        }
    }

    pub fn current() -> Option<Self> {
        if cfg!(windows) {
            Some(OperatingSystem::Windows)
        } else if cfg!(target_os = "macos") {
            Some(OperatingSystem::Macos)
        } else if cfg!(target_os = "linux") {
            Some(OperatingSystem::Linux)
        } else {
            None
        }
    }
}

impl fmt::Display for OperatingSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Target CPU architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Architecture {
    X86_64,
    Aarch64,
}

impl Architecture {
    pub fn as_str(self) -> &'static str {
        match self {
            Architecture::X86_64 => "x86_64",
            Architecture::Aarch64 => "aarch64",
        }
    }

    pub fn current() -> Option<Self> {
        if cfg!(target_arch = "x86_64") {
            Some(Architecture::X86_64)
        } else if cfg!(target_arch = "aarch64") {
            Some(Architecture::Aarch64)
        } else {
            None
        }
    }
}

impl fmt::Display for Architecture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Deterministic package name, e.g. `requestlab-windows-x86_64.zip`.
pub fn package_file_name(os: OperatingSystem, arch: Architecture) -> String {
    format!("requestlab-{}-{}.zip", os.as_str(), arch.as_str())
}

/// One downloadable package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Package {
    pub os: OperatingSystem,
    pub arch: Architecture,
    pub url: String,
    pub size: u64,
    /// Lowercase hex SHA-256.
    pub sha256: String,
}

impl Package {
    pub fn file_name(&self) -> String {
        package_file_name(self.os, self.arch)
    }

    fn validate(&self) -> Result<(), String> {
        if !self.url.starts_with("https://") {
            return Err(format!(
                "package for {}/{} must use an https URL",
                self.os, self.arch
            ));
        }
        if self.size == 0 {
            return Err(format!(
                "package for {}/{} declares a zero size",
                self.os, self.arch
            ));
        }
        if self.sha256.len() != 64
            || !self.sha256.chars().all(|c| c.is_ascii_hexdigit())
            || self.sha256.chars().any(|c| c.is_ascii_uppercase())
        {
            return Err(format!(
                "package for {}/{} has an invalid SHA-256",
                self.os, self.arch
            ));
        }
        if !self.url.ends_with(&self.file_name()) {
            return Err("package URL must end with the conventional file name".to_string());
        }
        Ok(())
    }
}

/// The signed description of one release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub schema: u32,
    pub app: String,
    pub version: Version,
    pub channel: Channel,
    pub released_at: DateTime<Utc>,
    pub min_updater_version: Version,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes_url: Option<String>,
    pub packages: Vec<Package>,
}

impl ReleaseManifest {
    /// Parse and structurally validate manifest bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let manifest: Self = serde_json::from_slice(bytes)
            .map_err(|error| format!("manifest is not valid JSON: {error}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Deterministic bytes; the signature covers exactly these.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("cannot serialise the manifest: {error}"))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema != SCHEMA_VERSION {
            return Err(format!(
                "manifest schema {} is not supported (this build reads {SCHEMA_VERSION})",
                self.schema
            ));
        }
        if self.app != APP_ID {
            return Err(format!("manifest describes '{}', not '{APP_ID}'", self.app));
        }
        if self.packages.is_empty() {
            return Err("manifest lists no packages".to_string());
        }
        if let Some(notes) = &self.notes_url {
            if !notes.starts_with("https://") {
                return Err("release notes URL must use https".to_string());
            }
        }
        if self.channel == Channel::Stable && self.version.is_prerelease() {
            return Err(format!("stable release {} is a prerelease", self.version));
        }

        let mut seen: Vec<(OperatingSystem, Architecture)> = Vec::new();
        for package in &self.packages {
            package.validate()?;
            if seen.contains(&(package.os, package.arch)) {
                return Err(format!(
                    "manifest lists more than one package for {}/{}",
                    package.os, package.arch
                ));
            }
            seen.push((package.os, package.arch));
        }
        Ok(())
    }

    pub fn package_for(&self, os: OperatingSystem, arch: Architecture) -> Result<&Package, String> {
        self.packages
            .iter()
            .find(|package| package.os == os && package.arch == arch)
            .ok_or_else(|| {
                let available = self
                    .packages
                    .iter()
                    .map(|package| format!("{}/{}", package.os, package.arch))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "release {} has no package for {os}/{arch} (available: {available})",
                    self.version
                )
            })
    }

    pub fn package_for_current_platform(&self) -> Result<&Package, String> {
        let os = OperatingSystem::current()
            .ok_or_else(|| "this operating system has no package convention".to_string())?;
        let arch = Architecture::current()
            .ok_or_else(|| "this CPU architecture has no package convention".to_string())?;
        self.package_for(os, arch)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn sample() -> String {
        r#"{
            "schema": 1,
            "app": "requestlab",
            "version": "0.2.0",
            "channel": "stable",
            "released_at": "2026-01-02T03:04:05Z",
            "min_updater_version": "0.1.1",
            "notes_url": "https://github.com/Ahaduzzamankhan/Request-lab/releases/tag/v0.2.0",
            "packages": [{
                "os": "windows",
                "arch": "x86_64",
                "url": "https://github.com/Ahaduzzamankhan/Request-lab/releases/download/v0.2.0/requestlab-windows-x86_64.zip",
                "size": 1234,
                "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }]
        }"#
        .to_string()
    }

    #[test]
    fn parses_and_round_trips() {
        let manifest = ReleaseManifest::parse(sample().as_bytes()).unwrap();
        assert_eq!(manifest.version, Version::new(0, 2, 0));
        let bytes = manifest.to_bytes().unwrap();
        assert_eq!(ReleaseManifest::parse(&bytes).unwrap(), manifest);
    }

    #[test]
    fn rejects_insecure_or_inconsistent_packages() {
        for broken in [
            sample().replace("https://github.com", "http://github.com"),
            sample().replace("\"size\": 1234", "\"size\": 0"),
            sample().replace(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "ABC",
            ),
            sample().replace("requestlab-windows-x86_64.zip", "package.zip"),
        ] {
            assert!(ReleaseManifest::parse(broken.as_bytes()).is_err());
        }
    }

    #[test]
    fn rejects_wrong_schema_app_and_stable_prerelease() {
        for broken in [
            sample().replace("\"schema\": 1", "\"schema\": 9"),
            sample().replace("\"app\": \"requestlab\"", "\"app\": \"other\""),
            sample().replace("\"version\": \"0.2.0\"", "\"version\": \"0.2.0-beta.1\""),
        ] {
            assert!(
                ReleaseManifest::parse(broken.as_bytes()).is_err(),
                "{broken}"
            );
        }
    }

    #[test]
    fn channel_accepts_only_its_versions() {
        let stable = Version::new(1, 0, 0);
        let beta: Version = "1.1.0-beta.1".parse().unwrap();
        assert!(Channel::Stable.accepts(&stable, false));
        assert!(!Channel::Stable.accepts(&beta, true));
        assert!(Channel::Beta.accepts(&stable, false));
        assert!(Channel::Beta.accepts(&beta, true));
    }

    #[test]
    fn missing_platform_names_what_is_available() {
        let manifest = ReleaseManifest::parse(sample().as_bytes()).unwrap();
        let error = manifest
            .package_for(OperatingSystem::Linux, Architecture::X86_64)
            .unwrap_err();
        assert!(error.contains("available: windows/x86_64"), "{error}");
    }
}
