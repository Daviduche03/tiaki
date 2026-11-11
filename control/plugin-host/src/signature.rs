//! Plugin signature verification.
//!
//! The production design signs with ed25519. This milestone ships a pluggable
//! verifier ABI plus a content-digest verifier so the install path is real:
//! the signature covers the manifest (minus the signature line) and every
//! asset file, addressed by relative path.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use agentic_core_log::{digest, hex};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignatureStatus {
    Verified,
    /// Signature is present but this host cannot verify it (for example
    /// `ed25519:` before the ed25519 verifier lands).
    Unverified(String),
    Rejected(String),
}

impl SignatureStatus {
    pub fn is_verified(&self) -> bool {
        matches!(self, SignatureStatus::Verified)
    }
}

pub trait SignatureVerifier {
    fn verify(&self, root: &Path, manifest_text: &str, signature: &str) -> SignatureStatus;
}

/// Verifies `sha256:<hex>` signatures over the manifest and assets.
pub struct DigestVerifier;

impl SignatureVerifier for DigestVerifier {
    fn verify(&self, root: &Path, manifest_text: &str, signature: &str) -> SignatureStatus {
        let Some(expected) = signature.strip_prefix("sha256:") else {
            return SignatureStatus::Unverified(format!(
                "no verifier for signature scheme '{}'",
                scheme(signature)
            ));
        };
        match compute_signature(root, manifest_text) {
            Ok(actual) => {
                let actual = actual.strip_prefix("sha256:").unwrap_or(actual.as_str());
                if constant_time_eq(expected.as_bytes(), actual.as_bytes()) {
                    SignatureStatus::Verified
                } else {
                    SignatureStatus::Rejected(format!(
                        "digest mismatch: manifest says sha256:{expected}, assets hash to sha256:{actual}"
                    ))
                }
            }
            Err(error) => SignatureStatus::Rejected(format!("could not hash plugin: {error}")),
        }
    }
}

/// Accepts any non-empty signature as `Unverified`. Used only when the caller
/// explicitly allows unverified plugins.
pub struct PresenceVerifier;

impl SignatureVerifier for PresenceVerifier {
    fn verify(&self, _root: &Path, _manifest_text: &str, signature: &str) -> SignatureStatus {
        if signature.trim().is_empty() {
            SignatureStatus::Rejected("missing signature".into())
        } else {
            SignatureStatus::Unverified("signature not checked".into())
        }
    }
}

fn scheme(signature: &str) -> &str {
    signature.split(':').next().unwrap_or(signature)
}

/// Compute the canonical `sha256:<hex>` signature for a plugin directory.
pub fn compute_signature(root: &Path, manifest_text: &str) -> io::Result<String> {
    let mut material = Vec::new();
    material.extend_from_slice(strip_signature(manifest_text).as_bytes());
    material.push(0);

    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));

    for (relative, path) in files {
        if relative == Path::new("plugin.toml") {
            continue;
        }
        let bytes = fs::read(&path)?;
        let relative = relative.to_string_lossy().replace('\\', "/");
        material.extend_from_slice(relative.as_bytes());
        material.push(0);
        material.extend_from_slice(hex(&digest(&bytes)).as_bytes());
        material.push(b'\n');
    }

    Ok(format!("sha256:{}", hex(&digest(&material))))
}

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, PathBuf)>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_files(root, &path, out)?;
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(root)
                .map(|value| value.to_path_buf())
                .unwrap_or_else(|_| path.clone());
            out.push((relative, path));
        }
    }
    Ok(())
}

/// Drop the `signature = "..."` line from the `[update]` table so the
/// signature does not cover itself.
pub fn strip_signature(manifest_text: &str) -> String {
    let mut in_update = false;
    let mut out = String::new();
    for line in manifest_text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix('[') {
            let name = rest.trim_end_matches(']').trim();
            in_update = name == "update";
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if in_update && trimmed.starts_with("signature") && trimmed.contains('=') {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in a.iter().zip(b) {
        difference |= left ^ right;
    }
    difference == 0
}

impl fmt::Display for SignatureStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignatureStatus::Verified => write!(f, "verified"),
            SignatureStatus::Unverified(reason) => write!(f, "unverified ({reason})"),
            SignatureStatus::Rejected(reason) => write!(f, "rejected ({reason})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tiaki-signature-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("ui")).unwrap();
        fs::write(dir.join("ui/tree.json"), "{\"id\":\"x\"}").unwrap();
        fs::write(dir.join("lib.so"), "binary").unwrap();
        dir
    }

    #[test]
    fn digest_round_trips() {
        let dir = sample_dir();
        let manifest = "[plugin]\nid = \"x.y\"\n\n[update]\nchannel = \"stable\"\nsignature = \"sha256:placeholder\"\n";
        let signature = compute_signature(&dir, manifest).expect("signature");
        let manifest = manifest.replace("sha256:placeholder", &signature);

        let status = DigestVerifier.verify(&dir, &manifest, &signature);
        assert_eq!(status, SignatureStatus::Verified);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tampering_is_rejected() {
        let dir = sample_dir();
        let manifest = "[update]\nsignature = \"sha256:placeholder\"\n";
        let signature = compute_signature(&dir, manifest).expect("signature");
        let manifest = manifest.replace("sha256:placeholder", &signature);
        fs::write(dir.join("lib.so"), "tampered").unwrap();

        let status = DigestVerifier.verify(&dir, &manifest, &signature);
        assert!(matches!(status, SignatureStatus::Rejected(_)));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn signature_line_does_not_cover_itself() {
        let text = "[update]\nchannel = \"stable\"\nsignature = \"sha256:abc\"\n";
        let stripped = strip_signature(text);
        assert!(!stripped.contains("sha256:abc"));
        assert!(stripped.contains("channel"));
    }
}
