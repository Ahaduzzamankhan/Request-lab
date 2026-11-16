//! GitHub release metadata and deterministic release selection.
//!
//! Drafts are ignored, prereleases are visible only on the beta channel, and the
//! newest release is chosen by parsed semantic version rather than by the order
//! GitHub returns.

use serde::Deserialize;

use crate::error::{ManagerError, ManagerResult};
use crate::manifest::{
    package_file_name, Architecture, Channel, OperatingSystem, MANIFEST_ASSET, SIGNATURE_ASSET,
};
use crate::version::Version;

/// One asset attached to a GitHub release.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GitHubAsset {
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
    #[serde(default)]
    pub content_type: String,
}

/// A release as returned by the GitHub API.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GitHubRelease {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub published_at: Option<chrono::DateTime<chrono::Utc>>,
    pub assets: Vec<GitHubAsset>,
}

impl GitHubRelease {
    /// Version parsed from the tag, e.g. `v0.2.0` → `0.2.0`.
    pub fn version(&self) -> ManagerResult<Version> {
        self.tag_name.parse::<Version>().map_err(|error| {
            ManagerError::GitHub(format!(
                "release tag '{}' is not a version: {error}",
                self.tag_name
            ))
        })
    }

    pub fn title(&self) -> String {
        self.name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| self.tag_name.clone())
    }

    pub fn asset(&self, name: &str) -> Option<&GitHubAsset> {
        self.assets.iter().find(|asset| asset.name == name)
    }

    pub fn manifest_asset(&self) -> ManagerResult<&GitHubAsset> {
        self.asset(MANIFEST_ASSET)
            .ok_or_else(|| ManagerError::AssetMissing {
                version: self.tag_name.clone(),
                asset: MANIFEST_ASSET.to_string(),
            })
    }

    pub fn signature_asset(&self) -> ManagerResult<&GitHubAsset> {
        self.asset(SIGNATURE_ASSET)
            .ok_or_else(|| ManagerError::AssetMissing {
                version: self.tag_name.clone(),
                asset: SIGNATURE_ASSET.to_string(),
            })
    }

    /// Package asset for a platform by the published naming convention.
    pub fn package_asset(
        &self,
        os: OperatingSystem,
        arch: Architecture,
    ) -> ManagerResult<&GitHubAsset> {
        let name = package_file_name(os, arch);
        self.asset(&name).ok_or_else(|| {
            let available = self
                .assets
                .iter()
                .map(|asset| asset.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            ManagerError::NoCompatiblePackage(format!(
                "release {} has no '{name}' asset (available: {available})",
                self.tag_name
            ))
        })
    }

    /// Whether this release is a candidate on `channel`.
    pub fn is_candidate_for(&self, channel: Channel) -> bool {
        if self.draft {
            return false;
        }
        let Ok(version) = self.version() else {
            return false;
        };
        channel.accepts(&version, self.prerelease)
    }
}

/// Newest candidate on `channel` that is newer than `installed`.
///
/// Malformed releases are skipped with a log line: one bad tag must not hide a
/// good release.
pub fn select_update<'a>(
    releases: &'a [GitHubRelease],
    channel: Channel,
    installed: &Version,
) -> ManagerResult<Option<&'a GitHubRelease>> {
    let mut best: Option<(Version, &GitHubRelease)> = None;

    for release in releases {
        if release.draft {
            continue;
        }
        let version = match release.version() {
            Ok(version) => version,
            Err(error) => {
                tracing::debug!(tag = %release.tag_name, %error, "skipping a release with an unparsable tag");
                continue;
            }
        };
        if !channel.accepts(&version, release.prerelease) || version <= *installed {
            continue;
        }
        if release.manifest_asset().is_err() {
            tracing::debug!(tag = %release.tag_name, "skipping a release without a signed manifest");
            continue;
        }
        match &best {
            Some((best_version, _)) if *best_version >= version => {}
            _ => best = Some((version, release)),
        }
    }

    Ok(best.map(|(_, release)| release))
}

/// Newest release on a channel, regardless of the installed version.
pub fn newest_on_channel(
    releases: &[GitHubRelease],
    channel: Channel,
) -> ManagerResult<Option<&GitHubRelease>> {
    let mut best: Option<(Version, &GitHubRelease)> = None;
    for release in releases {
        if !release.is_candidate_for(channel) {
            continue;
        }
        let Ok(version) = release.version() else {
            continue;
        };
        match &best {
            Some((best_version, _)) if *best_version >= version => {}
            _ => best = Some((version, release)),
        }
    }
    Ok(best.map(|(_, release)| release))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, assets: &[&str]) -> GitHubRelease {
        GitHubRelease {
            tag_name: tag.to_string(),
            name: Some(format!("RequestLab {tag}")),
            body: Some("notes".to_string()),
            html_url: format!("https://github.com/Ahaduzzamankhan/Request-lab/releases/tag/{tag}"),
            draft: false,
            prerelease,
            published_at: Some(chrono::Utc::now()),
            assets: assets
                .iter()
                .map(|name| GitHubAsset {
                    name: name.to_string(),
                    size: 10,
                    browser_download_url: format!(
                        "https://github.com/Ahaduzzamankhan/Request-lab/releases/download/{tag}/{name}"
                    ),
                    content_type: "application/octet-stream".to_string(),
                })
                .collect(),
        }
    }

    const FULL: &[&str] = &[
        "requestlab-manifest.json",
        "requestlab-manifest.json.sig",
        "requestlab-windows-x86_64.zip",
    ];

    #[test]
    fn parses_github_release_json() {
        let json = r##"[
            {
                "tag_name": "v0.2.0",
                "name": "RequestLab 0.2.0",
                "body": "Changes",
                "html_url": "https://github.com/x/y/releases/tag/v0.2.0",
                "draft": false,
                "prerelease": false,
                "published_at": "2026-02-01T10:00:00Z",
                "assets": [{
                    "name": "requestlab-windows-x86_64.zip",
                    "size": 4096,
                    "browser_download_url": "https://github.com/x/y/releases/download/v0.2.0/requestlab-windows-x86_64.zip",
                    "content_type": "application/zip"
                }]
            }
        ]"##;
        let releases: Vec<GitHubRelease> = serde_json::from_str(json).unwrap();
        assert_eq!(releases[0].version().unwrap(), Version::new(0, 2, 0));
        assert_eq!(releases[0].title(), "RequestLab 0.2.0");
        assert!(releases[0].manifest_asset().is_err());
    }

    #[test]
    fn selects_the_highest_newer_release_per_channel() {
        let releases = vec![
            release("v0.1.1", false, FULL),
            release("v0.3.0", false, FULL),
            release("v0.2.5", false, FULL),
            release("v0.4.0-beta.1", true, FULL),
        ];
        let stable = select_update(&releases, Channel::Stable, &Version::new(0, 1, 1))
            .unwrap()
            .unwrap();
        assert_eq!(stable.tag_name, "v0.3.0");
        let beta = select_update(&releases, Channel::Beta, &Version::new(0, 1, 1))
            .unwrap()
            .unwrap();
        assert_eq!(beta.tag_name, "v0.4.0-beta.1");
    }

    #[test]
    fn reports_no_update_when_current_or_ahead() {
        let releases = vec![release("v0.1.1", false, FULL)];
        assert!(
            select_update(&releases, Channel::Stable, &Version::new(0, 1, 1))
                .unwrap()
                .is_none()
        );
        assert!(
            select_update(&releases, Channel::Stable, &Version::new(0, 2, 0))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn ignores_drafts_prereleases_and_unmanifested_releases() {
        let mut draft = release("v9.0.0", false, FULL);
        draft.draft = true;
        let releases = vec![
            draft,
            release("v8.0.0-beta.1", true, FULL),
            release("v7.0.0", false, &["requestlab-windows-x86_64.zip"]),
            release("nightly", false, FULL),
            release("v0.2.0", false, FULL),
        ];
        let chosen = select_update(&releases, Channel::Stable, &Version::new(0, 1, 0))
            .unwrap()
            .unwrap();
        assert_eq!(chosen.tag_name, "v0.2.0");
    }

    #[test]
    fn asset_matching_is_exact_and_names_the_expected_file() {
        let candidate = release("v0.2.0", false, FULL);
        assert_eq!(
            candidate
                .package_asset(OperatingSystem::Windows, Architecture::X86_64)
                .unwrap()
                .name,
            "requestlab-windows-x86_64.zip"
        );
        let error = candidate
            .package_asset(OperatingSystem::Linux, Architecture::X86_64)
            .unwrap_err();
        assert!(
            error.to_string().contains("requestlab-linux-x86_64.zip"),
            "{error}"
        );
    }

    #[test]
    fn newest_on_channel_ignores_installed_version() {
        let releases = vec![
            release("v0.1.1", false, FULL),
            release("v0.3.0", false, FULL),
        ];
        assert_eq!(
            newest_on_channel(&releases, Channel::Stable)
                .unwrap()
                .unwrap()
                .tag_name,
            "v0.3.0"
        );
    }
}
