//! Integrity and authenticity: SHA-256, Ed25519 signatures and trusted keys.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use sha2::{Digest, Sha256};

use crate::error::{ManagerError, ManagerResult};

/// Environment variable carrying a trusted public key.
pub const TRUST_ENV: &str = "REQUESTLAB_MANIFEST_PUBLIC_KEY";
/// Key compiled in at build time, when the release build set one.
pub const BUILD_KEY: Option<&str> = option_env!("REQUESTLAB_MANIFEST_PUBLIC_KEY");
/// Directory inside the data directory holding trusted keys.
pub const TRUST_DIR: &str = "trusted-keys";
/// Environment variable carrying the signing seed during a release.
pub const SIGNING_KEY_ENV: &str = "REQUESTLAB_MANIFEST_SIGNING_KEY";

/// Base64 (standard, padded).
pub fn b64_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Decode base64, ignoring surrounding whitespace.
pub fn b64_decode(value: &str) -> Result<Vec<u8>, String> {
    base64::engine::general_purpose::STANDARD
        .decode(value.trim())
        .map_err(|error| format!("'{value}' is not valid base64: {error}"))
}

/// Lowercase hex.
pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    bytes
        .iter()
        .flat_map(|byte| {
            [
                HEX[(byte >> 4) as usize] as char,
                HEX[(byte & 0x0f) as usize] as char,
            ]
        })
        .collect()
}

/// SHA-256 of a buffer, lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex_encode(&Sha256::digest(bytes))
}

/// SHA-256 of a file, streamed.
pub fn sha256_file(path: &Path) -> ManagerResult<String> {
    use std::io::Read as _;

    let mut file = std::fs::File::open(path).map_err(|error| ManagerError::io(path, error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| ManagerError::io(path, error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_encode(&hasher.finalize()))
}

/// Constant-time digest comparison.
pub fn digests_match(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

/// Verify a file's size and SHA-256 against the manifest.
pub fn verify_package(path: &Path, expected_sha256: &str, expected_size: u64) -> ManagerResult<()> {
    let actual_size = std::fs::metadata(path)
        .map_err(|error| ManagerError::io(path, error))?
        .len();
    if actual_size != expected_size {
        return Err(ManagerError::SizeMismatch {
            file: path.display().to_string(),
            expected: expected_size,
            actual: actual_size,
        });
    }
    let actual = sha256_file(path)?;
    if digests_match(&actual, expected_sha256) {
        Ok(())
    } else {
        Err(ManagerError::Checksum {
            file: path.display().to_string(),
            expected: expected_sha256.to_string(),
            actual,
        })
    }
}

/// A public key trusted to sign release manifests.
#[derive(Clone)]
pub struct TrustedKey {
    pub key: VerifyingKey,
    pub source: String,
}

impl TrustedKey {
    /// Short stable identifier: the first eight bytes of the key's SHA-256.
    pub fn id(&self) -> String {
        sha256_hex(self.key.as_bytes())[..16].to_string()
    }

    pub fn from_base64(value: &str, source: impl Into<String>) -> Result<Self, String> {
        let bytes = b64_decode(value)?;
        let array: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| format!("an Ed25519 public key is 32 bytes, got {}", bytes.len()))?;
        let key = VerifyingKey::from_bytes(&array)
            .map_err(|error| format!("invalid Ed25519 public key: {error}"))?;
        Ok(Self {
            key,
            source: source.into(),
        })
    }
}

impl std::fmt::Debug for TrustedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrustedKey")
            .field("id", &self.id())
            .field("source", &self.source)
            .finish()
    }
}

/// Every key this process trusts.
#[derive(Debug, Default, Clone)]
pub struct TrustStore {
    keys: Vec<TrustedKey>,
}

impl TrustStore {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Load build-embedded, environment and on-disk keys.
    pub fn load(data_dir: Option<&Path>) -> Self {
        let mut store = Self::empty();

        if let Some(value) = BUILD_KEY {
            match TrustedKey::from_base64(value, "compiled into this build") {
                Ok(key) => store.keys.push(key),
                Err(error) => tracing::error!(%error, "embedded release key is invalid"),
            }
        }
        if let Ok(value) = std::env::var(TRUST_ENV) {
            if !value.trim().is_empty() {
                match TrustedKey::from_base64(&value, format!("${TRUST_ENV}")) {
                    Ok(key) => store.keys.push(key),
                    Err(error) => tracing::warn!(%error, "ignoring invalid {TRUST_ENV}"),
                }
            }
        }
        if let Some(data_dir) = data_dir {
            store.load_key_directory(&data_dir.join(TRUST_DIR));
        }
        store
    }

    /// Load every `*.pub` file in a directory; comments and blank lines allowed.
    pub fn load_key_directory(&mut self, directory: &Path) {
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => {
                tracing::warn!(path = %directory.display(), %error, "cannot read trusted key directory");
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("pub") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let key_text = text
                .lines()
                .find(|line| {
                    let line = line.trim();
                    !line.is_empty() && !line.starts_with('#')
                })
                .unwrap_or_default();
            match TrustedKey::from_base64(key_text, path.display().to_string()) {
                Ok(key) => self.keys.push(key),
                Err(error) => {
                    tracing::warn!(path = %path.display(), %error, "ignoring invalid trusted key")
                }
            }
        }
    }

    pub fn add(&mut self, key: TrustedKey) {
        self.keys.push(key);
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn key_ids(&self) -> Vec<String> {
        self.keys
            .iter()
            .map(|key| format!("{} ({})", key.id(), key.source))
            .collect()
    }

    /// Verify a detached signature over exact manifest bytes; returns the key id.
    pub fn verify_manifest(
        &self,
        manifest: &[u8],
        signature_base64: &str,
    ) -> ManagerResult<String> {
        if self.keys.is_empty() {
            return Err(ManagerError::Signature(format!(
                "no trusted signing key is configured. Add a key to <data dir>/{TRUST_DIR}/<name>.pub, set {TRUST_ENV}, or build with {TRUST_ENV} set"
            )));
        }
        let signature_bytes = b64_decode(signature_base64).map_err(|error| {
            ManagerError::Signature(format!("signature is not valid base64: {error}"))
        })?;
        let signature = Signature::from_slice(&signature_bytes).map_err(|error| {
            ManagerError::Signature(format!("signature has the wrong length: {error}"))
        })?;
        for key in &self.keys {
            if key.key.verify(manifest, &signature).is_ok() {
                return Ok(key.id());
            }
        }
        Err(ManagerError::Signature(format!(
            "the manifest signature matches none of the {} trusted key(s) [{}]",
            self.keys.len(),
            self.key_ids().join(", ")
        )))
    }
}

/// Directory holding trusted keys for a data directory.
pub fn trust_directory(data_dir: &Path) -> PathBuf {
    data_dir.join(TRUST_DIR)
}

/// A generated signing key pair; the seed is the secret.
pub struct GeneratedKey {
    pub seed_base64: String,
    pub public_base64: String,
}

/// Generate a new Ed25519 signing key from the OS random source.
pub fn generate_key() -> ManagerResult<GeneratedKey> {
    let mut seed = [0u8; 32];
    fill_random(&mut seed)?;
    let signing = SigningKey::from_bytes(&seed);
    Ok(GeneratedKey {
        seed_base64: b64_encode(&seed),
        public_base64: b64_encode(signing.verifying_key().as_bytes()),
    })
}

#[cfg(windows)]
fn fill_random(buffer: &mut [u8]) -> ManagerResult<()> {
    use windows_sys::Win32::Security::Cryptography::{
        BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
    };
    // SAFETY: the buffer is valid for its length and the system RNG needs no setup.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status != 0 {
        return Err(ManagerError::Other(format!(
            "the system random generator failed (status {status:#x})"
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn fill_random(buffer: &mut [u8]) -> ManagerResult<()> {
    use std::io::Read as _;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(buffer))
        .map_err(|error| ManagerError::Other(format!("cannot read /dev/urandom: {error}")))
}

fn signing_key(seed_base64: &str) -> ManagerResult<SigningKey> {
    let bytes = b64_decode(seed_base64)
        .map_err(|error| ManagerError::Signature(format!("signing key is not base64: {error}")))?;
    let seed: [u8; 32] = bytes.as_slice().try_into().map_err(|_| {
        ManagerError::Signature(format!("a signing seed is 32 bytes, got {}", bytes.len()))
    })?;
    Ok(SigningKey::from_bytes(&seed))
}

/// Public key for a seed.
pub fn public_key_for_seed(seed_base64: &str) -> ManagerResult<String> {
    Ok(b64_encode(
        signing_key(seed_base64)?.verifying_key().as_bytes(),
    ))
}

/// Sign bytes; returns the base64 detached signature.
pub fn sign_bytes(seed_base64: &str, bytes: &[u8]) -> ManagerResult<String> {
    Ok(b64_encode(
        &signing_key(seed_base64)?.sign(bytes).to_bytes(),
    ))
}

/// Verify a signature against one explicit public key.
pub fn verify_with_public_key(
    public_base64: &str,
    bytes: &[u8],
    signature_base64: &str,
) -> ManagerResult<()> {
    let mut store = TrustStore::empty();
    let key =
        TrustedKey::from_base64(public_base64, "supplied key").map_err(ManagerError::Signature)?;
    store.add(key);
    store.verify_manifest(bytes, signature_base64).map(|_| ())
}

/// Read signing material from a file or `REQUESTLAB_MANIFEST_SIGNING_KEY`.
pub fn load_seed(path: Option<&Path>) -> ManagerResult<(String, String)> {
    if let Some(path) = path {
        let text = std::fs::read_to_string(path).map_err(|error| ManagerError::io(path, error))?;
        let seed = text
            .lines()
            .find(|line| {
                let line = line.trim();
                !line.is_empty() && !line.starts_with('#')
            })
            .unwrap_or_default()
            .trim()
            .to_string();
        if seed.is_empty() {
            return Err(ManagerError::Signature(format!(
                "{} contains no key",
                path.display()
            )));
        }
        return Ok((seed, path.display().to_string()));
    }
    match std::env::var(SIGNING_KEY_ENV) {
        Ok(seed) if !seed.trim().is_empty() => {
            Ok((seed.trim().to_string(), format!("${SIGNING_KEY_ENV}")))
        }
        _ => Err(ManagerError::Signature(format!(
            "no signing key: pass --key-file or set {SIGNING_KEY_ENV}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digests_match_known_vectors() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(digests_match(&sha256_hex(b"x"), &sha256_hex(b"x")));
        assert!(!digests_match(&sha256_hex(b"x"), &sha256_hex(b"y")));
        assert!(!digests_match("abc", "abcd"));
    }

    #[test]
    fn signing_round_trips_and_detects_tampering() {
        let key = generate_key().unwrap();
        let signature = sign_bytes(&key.seed_base64, b"manifest").unwrap();
        verify_with_public_key(&key.public_base64, b"manifest", &signature).unwrap();
        assert!(verify_with_public_key(&key.public_base64, b"tampered", &signature).is_err());
    }

    #[test]
    fn trust_store_without_keys_is_an_error() {
        let error = TrustStore::empty()
            .verify_manifest(b"{}", "AAAA")
            .unwrap_err();
        assert!(
            error.to_string().contains("no trusted signing key"),
            "{error}"
        );
    }

    #[test]
    fn package_verification_checks_size_and_digest() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("package.zip");
        std::fs::write(&path, b"payload").unwrap();
        let digest = sha256_hex(b"payload");
        verify_package(&path, &digest, 7).unwrap();
        assert!(matches!(
            verify_package(&path, &digest, 8),
            Err(ManagerError::SizeMismatch { .. })
        ));
        assert!(matches!(
            verify_package(&path, &sha256_hex(b"other"), 7),
            Err(ManagerError::Checksum { .. })
        ));
    }
}
