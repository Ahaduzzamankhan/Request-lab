//! Release tooling: build a signed manifest for a published build.
//!
//! `build-manifest` describes the archives produced by CI, `sign-manifest` signs
//! the exact bytes that are published, and `generate-key` creates the key pair.
//! The signing seed is read from a file or an environment variable and is never
//! echoed.

use std::path::{Path, PathBuf};

use chrono::Utc;
use requestlab_manager::manifest::{
    package_file_name, Architecture, OperatingSystem, Package, ReleaseManifest, APP_ID,
    SCHEMA_VERSION,
};
use requestlab_manager::verifier::{self, TrustStore};
use requestlab_manager::version::Version;
use requestlab_manager::{paths, REPO_NAME, REPO_OWNER};

use crate::args::{KeyArgs, ManifestArgs, SignArgs};

/// Environment variable that carries the release signing seed.
pub const SIGNING_KEY_ENV: &str = "REQUESTLAB_SIGNING_KEY";

fn parse_version(value: &str) -> Result<Version, String> {
    value
        .parse::<Version>()
        .map_err(|error| format!("'{value}' is not a valid version: {error}"))
}

/// Build a manifest describing `dist/requestlab-windows-x86_64.zip`.
pub fn build(args: &ManifestArgs) -> Result<PathBuf, String> {
    let version = parse_version(&args.version)?;
    let min_updater = match &args.min_updater {
        Some(value) => parse_version(value)?,
        None => parse_version(requestlab_manager::VERSION)?,
    };
    let os = OperatingSystem::Windows;
    let arch = Architecture::X86_64;
    let file_name = package_file_name(os, arch);
    let package_path = args.dist.join(&file_name);
    let metadata = std::fs::metadata(&package_path).map_err(|error| {
        format!(
            "{} is missing: the release packages must be built first ({error})",
            package_path.display()
        )
    })?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(format!(
            "{} is not a non-empty file",
            package_path.display()
        ));
    }

    let base = args
        .base_url
        .clone()
        .unwrap_or_else(|| {
            format!("https://github.com/{REPO_OWNER}/{REPO_NAME}/releases/download/v{version}")
        })
        .trim_end_matches('/')
        .to_string();
    let sha256 = verifier::sha256_file(&package_path).map_err(|error| error.to_string())?;

    let manifest = ReleaseManifest {
        schema: SCHEMA_VERSION,
        app: APP_ID.to_string(),
        version: version.clone(),
        channel: args.channel,
        released_at: Utc::now(),
        min_updater_version: min_updater,
        notes_url: args.notes_url.clone(),
        packages: vec![Package {
            os,
            arch,
            url: format!("{base}/{file_name}"),
            size: metadata.len(),
            sha256,
        }],
    };
    manifest.validate()?;
    let bytes = manifest.to_bytes()?;
    if let Some(parent) = args.out.parent() {
        if !parent.as_os_str().is_empty() {
            paths::ensure_dir(parent).map_err(|error| error.to_string())?;
        }
    }
    std::fs::write(&args.out, bytes)
        .map_err(|error| format!("cannot write {}: {error}", args.out.display()))?;
    let package = &manifest.packages[0];
    println!(
        "wrote {} for RequestLab {} ({} bytes, sha256 {})",
        args.out.display(),
        version,
        package.size,
        package.sha256
    );
    Ok(args.out.clone())
}

fn signing_seed(key_file: Option<&Path>) -> Result<String, String> {
    if let Some(path) = key_file {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let seed = text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'))
            .ok_or_else(|| format!("{} holds no key", path.display()))?;
        return Ok(seed.to_string());
    }
    match std::env::var(SIGNING_KEY_ENV) {
        Ok(value) if !value.trim().is_empty() => Ok(value.trim().to_string()),
        _ => Err(format!(
            "no signing key: pass --key-file or set {SIGNING_KEY_ENV} in the CI secrets"
        )),
    }
}

/// The public half of the configured signing key.
pub fn public_key(key_file: Option<&Path>) -> Result<String, String> {
    let seed = signing_seed(key_file)?;
    verifier::public_key_for_seed(&seed).map_err(|error| error.to_string())
}

/// Sign a manifest, writing the detached base64 signature.
pub fn sign(args: &SignArgs) -> Result<String, String> {
    let seed = signing_seed(args.key_file.as_deref())?;
    let bytes = std::fs::read(&args.manifest)
        .map_err(|error| format!("cannot read {}: {error}", args.manifest.display()))?;
    // Refuse to sign something the application would reject.
    ReleaseManifest::parse(&bytes)?;
    let signature = verifier::sign_bytes(&seed, &bytes).map_err(|error| error.to_string())?;
    std::fs::write(&args.out, format!("{signature}\n"))
        .map_err(|error| format!("cannot write {}: {error}", args.out.display()))?;
    let public = verifier::public_key_for_seed(&seed).map_err(|error| error.to_string())?;
    println!(
        "signed {} with key {}",
        args.manifest.display(),
        short_key_id(&public)?
    );
    Ok(public)
}

fn short_key_id(public_base64: &str) -> Result<String, String> {
    let key = verifier::TrustedKey::from_base64(public_base64, "release key")
        .map_err(|error| format!("invalid public key: {error}"))?;
    Ok(key.id())
}

/// Create a key pair, keep the seed private and install the public key.
pub fn generate_key(args: &KeyArgs) -> Result<String, String> {
    let generated = verifier::generate_key().map_err(|error| error.to_string())?;
    paths::ensure_dir(&args.out).map_err(|error| error.to_string())?;
    let seed_path = args.out.join("signing-key.txt");
    let public_path = args.out.join("public-key.txt");
    std::fs::write(
        &seed_path,
        format!(
            "# RequestLab release signing key. Keep this secret; it signs every release.\n{}\n",
            generated.seed_base64
        ),
    )
    .map_err(|error| format!("cannot write {}: {error}", seed_path.display()))?;
    restrict_permissions(&seed_path);
    std::fs::write(
        &public_path,
        format!(
            "# RequestLab release public key. Install this in the application's trust directory.\n{}\n",
            generated.public_base64
        ),
    )
    .map_err(|error| format!("cannot write {}: {error}", public_path.display()))?;

    let id = short_key_id(&generated.public_base64)?;
    if let Some(directory) = &args.trust_dir {
        paths::ensure_dir(directory).map_err(|error| error.to_string())?;
        let target = directory.join(format!("{id}.pub"));
        std::fs::write(&target, format!("{}\n", generated.public_base64))
            .map_err(|error| format!("cannot write {}: {error}", target.display()))?;
        println!("installed trusted key in {}", target.display());
    }

    // The key must actually verify what it signs.
    let probe = b"requestlab-key-check";
    let signature =
        verifier::sign_bytes(&generated.seed_base64, probe).map_err(|error| error.to_string())?;
    let mut store = TrustStore::empty();
    let key = verifier::TrustedKey::from_base64(&generated.public_base64, "generated")
        .map_err(|error| format!("the generated key is invalid: {error}"))?;
    store.add(key);
    store
        .verify_manifest(probe, &signature)
        .map_err(|error| format!("the generated key cannot verify its own signature: {error}"))?;

    println!(
        "wrote {} (secret) and {}",
        seed_path.display(),
        public_path.display()
    );
    println!("public key id: {id}");
    println!("public key: {}", generated.public_base64);
    Ok(generated.public_base64)
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

/// Default trust directory for this user, used when `--trust-dir` is not given.
pub fn default_trust_dir() -> PathBuf {
    verifier::trust_directory(&paths::user_data_dir())
}

#[cfg(test)]
mod tests {
    use super::*;
    use requestlab_manager::manifest::Channel;

    fn args_for(dist: &Path, out: PathBuf) -> ManifestArgs {
        ManifestArgs {
            dist: dist.to_path_buf(),
            version: "0.1.2".to_string(),
            channel: Channel::Stable,
            notes_url: None,
            base_url: None,
            min_updater: None,
            out,
        }
    }

    #[test]
    fn a_manifest_describes_the_packaged_archive() {
        let temp = tempfile::tempdir().unwrap();
        let dist = temp.path().join("dist");
        std::fs::create_dir_all(&dist).unwrap();
        let package = dist.join(package_file_name(
            OperatingSystem::Windows,
            Architecture::X86_64,
        ));
        std::fs::write(&package, b"fake archive bytes").unwrap();
        let out = temp.path().join("manifest.json");

        let written = build(&args_for(&dist, out.clone())).unwrap();
        assert_eq!(written, out);
        let manifest = ReleaseManifest::parse(&std::fs::read(&out).unwrap()).unwrap();
        assert_eq!(manifest.version, Version::new(0, 1, 2));
        assert_eq!(manifest.packages.len(), 1);
        assert_eq!(manifest.packages[0].size, 18);
        assert!(manifest.packages[0]
            .url
            .ends_with("requestlab-windows-x86_64.zip"));
        assert!(manifest.packages[0].url.starts_with("https://github.com/"));
    }

    #[test]
    fn a_custom_base_url_is_used_verbatim() {
        let temp = tempfile::tempdir().unwrap();
        let dist = temp.path().join("dist");
        std::fs::create_dir_all(&dist).unwrap();
        std::fs::write(
            dist.join(package_file_name(
                OperatingSystem::Windows,
                Architecture::X86_64,
            )),
            b"bytes",
        )
        .unwrap();
        let mut args = args_for(&dist, temp.path().join("manifest.json"));
        args.base_url = Some("https://example.com/downloads/".to_string());
        build(&args).unwrap();
        let manifest = ReleaseManifest::parse(&std::fs::read(&args.out).unwrap()).unwrap();
        assert_eq!(
            manifest.packages[0].url,
            "https://example.com/downloads/requestlab-windows-x86_64.zip"
        );
    }

    #[test]
    fn a_missing_package_is_reported_clearly() {
        let temp = tempfile::tempdir().unwrap();
        let error = build(&args_for(temp.path(), temp.path().join("manifest.json"))).unwrap_err();
        assert!(error.contains("must be built first"));
    }

    #[test]
    fn a_generated_key_signs_what_it_verifies() {
        let temp = tempfile::tempdir().unwrap();
        let trust = temp.path().join("trust");
        let public = generate_key(&KeyArgs {
            out: temp.path().join("keys"),
            trust_dir: Some(trust.clone()),
        })
        .unwrap();
        assert!(temp.path().join("keys").join("signing-key.txt").is_file());
        let entries: Vec<_> = std::fs::read_dir(&trust).unwrap().flatten().collect();
        assert_eq!(entries.len(), 1);

        let dist = temp.path().join("dist");
        std::fs::create_dir_all(&dist).unwrap();
        std::fs::write(
            dist.join(package_file_name(
                OperatingSystem::Windows,
                Architecture::X86_64,
            )),
            b"archive",
        )
        .unwrap();
        let output = build(&args_for(&dist, temp.path().join("manifest.json"))).unwrap();
        let signature_path = temp.path().join("manifest.sig");
        sign(&SignArgs {
            manifest: output.clone(),
            out: signature_path.clone(),
            key_file: Some(temp.path().join("keys").join("signing-key.txt")),
        })
        .unwrap();

        let signature = std::fs::read_to_string(&signature_path).unwrap();
        verifier::verify_with_public_key(
            &public,
            &std::fs::read(&output).unwrap(),
            signature.trim(),
        )
        .unwrap();
    }

    #[test]
    fn signing_without_a_key_says_how_to_provide_one() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = temp.path().join("manifest.json");
        std::fs::write(&manifest, b"{}").unwrap();
        let error = sign(&SignArgs {
            manifest,
            out: temp.path().join("signature"),
            key_file: None,
        })
        .unwrap_err();
        // Either the key is absent, or the environment of the test machine has
        // one and the manifest itself is rejected - both are useful errors.
        assert!(error.contains("signing key") || error.contains("manifest"));
    }

    #[test]
    fn trust_directory_lives_under_the_data_directory() {
        assert!(default_trust_dir().starts_with(paths::user_data_dir()));
        assert_ne!(default_trust_dir(), paths::user_data_dir());
    }
}
