//! Argument parsing for the update helper and the release tooling.
//!
//! Parsing is pure so it can be tested without touching the filesystem.

use std::path::PathBuf;
use std::time::Duration;

use requestlab_manager::manifest::Channel;

/// What the helper was asked to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Help,
    Version,
    /// Wait for the application to exit, then apply a staged update.
    Apply(ApplyArgs),
    /// Build a release manifest from packaged archives.
    BuildManifest(ManifestArgs),
    /// Sign a manifest with the release key.
    Sign(SignArgs),
    /// Print the public key for the configured signing seed.
    PublicKey {
        key_file: Option<PathBuf>,
    },
    /// Create a new signing key pair and install the public half.
    GenerateKey(KeyArgs),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApplyArgs {
    /// Staged record the caller believes it wrote, validated against the install.
    pub record: Option<PathBuf>,
    pub install_dir: PathBuf,
    pub pid: Option<u32>,
    pub launch: bool,
    pub timeout: Duration,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ManifestArgs {
    pub dist: PathBuf,
    pub version: String,
    pub channel: Channel,
    pub notes_url: Option<String>,
    /// Base URL the package is published under; the file name is appended.
    pub base_url: Option<String>,
    pub min_updater: Option<String>,
    pub out: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignArgs {
    pub manifest: PathBuf,
    pub out: PathBuf,
    pub key_file: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyArgs {
    pub out: PathBuf,
    /// Where to install the public key so this machine trusts the releases.
    pub trust_dir: Option<PathBuf>,
}

pub const USAGE: &str = "\
RequestLab update helper

USAGE:
    requestlab-update-helper [--help] [--version]
    requestlab-update-helper --install-dir DIR [--pid PID] [--launch]
                            [--record FILE] [--timeout SECONDS]
    requestlab-update-helper build-manifest --dist DIR --version VERSION --out FILE
                            [--channel stable|beta] [--notes-url URL]
                            [--base-url URL] [--min-updater VERSION]
    requestlab-update-helper sign-manifest --manifest FILE --out FILE
                            [--key-file FILE]
    requestlab-update-helper generate-key [--out DIR] [--trust-dir DIR]

The signing key is read from --key-file or the REQUESTLAB_SIGNING_KEY
environment variable (base64 seed). It is never printed.

COMMANDS:
    --install-dir DIR   Apply the staged update for this installation.
    --pid PID           Wait for this process to exit first.
    --launch            Start RequestLab after the update.
    --timeout SECONDS   How long to wait for the process (default 120).
    build-manifest      Write a manifest describing the packaged archives.
    sign-manifest       Write the detached Ed25519 signature for a manifest.
    public-key          Print the public key for the signing seed, so a build can
                        embed it.
    generate-key        Create a signing key pair and install the public key.";

/// Parse program arguments (without the executable name).
pub fn parse<I: IntoIterator<Item = String>>(arguments: I) -> Result<Mode, String> {
    let mut args = arguments.into_iter().peekable();
    let mut record = None;
    let mut install_dir = None;
    let mut pid = None;
    let mut launch = false;
    let mut timeout = 120u64;

    // Release tooling options.
    let mut dist = None;
    let mut version = None;
    let mut channel = Channel::Stable;
    let mut notes_url = None;
    let mut base_url = None;
    let mut min_updater = None;
    let mut out = None;
    let mut manifest = None;
    let mut key_file = None;
    let mut trust_dir = None;

    let mut command: Option<String> = None;
    if let Some(first) = args.peek() {
        match first.as_str() {
            "build-manifest" | "sign-manifest" | "public-key" | "generate-key" => {
                command = args.next();
            }
            _ => {}
        }
    }

    while let Some(arg) = args.next() {
        let mut value = |name: &str| -> Result<String, String> {
            args.next()
                .ok_or_else(|| format!("{name} needs a value"))
                .map(|value| value.trim().to_string())
        };
        match arg.as_str() {
            "--help" | "-h" => return Ok(Mode::Help),
            "--version" | "-V" => return Ok(Mode::Version),
            "--install-dir" => install_dir = Some(PathBuf::from(value("--install-dir")?)),
            "--record" | "--staged" => record = Some(PathBuf::from(value("--record")?)),
            "--pid" => {
                let raw = value("--pid")?;
                pid = Some(
                    raw.parse::<u32>()
                        .map_err(|_| format!("--pid expects a number, got '{raw}'"))?,
                );
            }
            "--launch" => launch = true,
            "--timeout" => {
                let raw = value("--timeout")?;
                timeout = raw
                    .parse::<u64>()
                    .map_err(|_| format!("--timeout expects seconds, got '{raw}'"))?;
            }
            "--dist" => dist = Some(PathBuf::from(value("--dist")?)),
            "--version-for" => version = Some(value("--version-for")?),
            "--channel" => {
                let raw = value("--channel")?;
                channel = Channel::parse(&raw)?;
            }
            "--notes-url" => notes_url = Some(value("--notes-url")?),
            "--base-url" => base_url = Some(value("--base-url")?),
            "--min-updater" => min_updater = Some(value("--min-updater")?),
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            "--manifest" => manifest = Some(PathBuf::from(value("--manifest")?)),
            "--key-file" => key_file = Some(PathBuf::from(value("--key-file")?)),
            "--trust-dir" => trust_dir = Some(PathBuf::from(value("--trust-dir")?)),
            other => return Err(format!("unknown argument '{other}'")),
        }
    }

    match command.as_deref() {
        Some("build-manifest") => Ok(Mode::BuildManifest(ManifestArgs {
            dist: dist.ok_or("build-manifest needs --dist")?,
            // `--version` is taken by the version flag, so the release version
            // is passed as `--version-for` to keep both unambiguous.
            version: version.ok_or("build-manifest needs --version-for")?,
            channel,
            notes_url,
            base_url,
            min_updater,
            out: out.ok_or("build-manifest needs --out")?,
        })),
        Some("sign-manifest") => Ok(Mode::Sign(SignArgs {
            manifest: manifest.ok_or("sign-manifest needs --manifest")?,
            out: out.ok_or("sign-manifest needs --out")?,
            key_file,
        })),
        Some("public-key") => Ok(Mode::PublicKey { key_file }),
        Some("generate-key") => Ok(Mode::GenerateKey(KeyArgs {
            out: out.unwrap_or_else(|| PathBuf::from("keys")),
            trust_dir,
        })),
        Some(other) => Err(format!("unknown command '{other}'")),
        None => {
            let install_dir = install_dir.ok_or("--install-dir is required")?;
            Ok(Mode::Apply(ApplyArgs {
                record,
                install_dir,
                pid,
                launch,
                timeout: Duration::from_secs(timeout.max(1)),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(args: &[&str]) -> Result<Mode, String> {
        parse(args.iter().map(|value| value.to_string()))
    }

    #[test]
    fn the_apply_mode_needs_an_install_directory() {
        assert!(parse_str(&[]).unwrap_err().contains("--install-dir"));
        let mode = parse_str(&["--install-dir", "C:/Apps/RequestLab", "--launch"]).unwrap();
        match mode {
            Mode::Apply(args) => {
                assert_eq!(args.install_dir, PathBuf::from("C:/Apps/RequestLab"));
                assert!(args.launch);
                assert_eq!(args.timeout, Duration::from_secs(120));
                assert!(args.pid.is_none());
            }
            other => panic!("unexpected mode {other:?}"),
        }
    }

    #[test]
    fn numbers_are_validated() {
        assert!(parse_str(&["--install-dir", "x", "--pid", "abc"])
            .unwrap_err()
            .contains("--pid"));
        let mode = parse_str(&["--install-dir", "x", "--pid", "42", "--timeout", "5"]).unwrap();
        match mode {
            Mode::Apply(args) => {
                assert_eq!(args.pid, Some(42));
                assert_eq!(args.timeout, Duration::from_secs(5));
            }
            other => panic!("unexpected mode {other:?}"),
        }
    }

    #[test]
    fn help_and_version_win_immediately() {
        assert_eq!(parse_str(&["--help"]).unwrap(), Mode::Help);
        assert_eq!(parse_str(&["--version"]).unwrap(), Mode::Version);
    }

    #[test]
    fn build_manifest_requires_its_inputs() {
        let error = parse_str(&["build-manifest", "--dist", "dist"]).unwrap_err();
        assert!(error.contains("--version-for"));
        let mode = parse_str(&[
            "build-manifest",
            "--dist",
            "dist",
            "--version-for",
            "0.1.2",
            "--out",
            "manifest.json",
            "--channel",
            "stable",
        ])
        .unwrap();
        match mode {
            Mode::BuildManifest(args) => {
                assert_eq!(args.dist, PathBuf::from("dist"));
                assert_eq!(args.version, "0.1.2");
                assert_eq!(args.channel, Channel::Stable);
                assert_eq!(args.out, PathBuf::from("manifest.json"));
            }
            other => panic!("unexpected mode {other:?}"),
        }
    }

    #[test]
    fn public_key_mode_is_recognised() {
        assert_eq!(
            parse_str(&["public-key"]).unwrap(),
            Mode::PublicKey { key_file: None }
        );
        assert_eq!(
            parse_str(&["public-key", "--key-file", "keys/signing-key.txt"]).unwrap(),
            Mode::PublicKey {
                key_file: Some(PathBuf::from("keys/signing-key.txt"))
            }
        );
    }

    #[test]
    fn unknown_arguments_are_rejected() {
        assert!(parse_str(&["--install-dir", "x", "--nope"])
            .unwrap_err()
            .contains("--nope"));
    }
}
