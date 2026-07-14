//! Dev-time artifact store (Impl Spec §7.4, §8.4): separate from book
//! storage, integrity-first. Hashes are the identity authority for a
//! deployed artifact; the path on disk is a locator only. Layout:
//! `<dev_artifacts_dir>/workflows/<workflow_deployment_id>/{workflow.json,
//! manifest.json, code/, signatures/}`.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct HashedArtifact {
    pub manifest_hash: String,
    pub code_hash: String,
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn workflow_dir(dev_artifacts_dir: &str, workflow_deployment_id: Uuid) -> PathBuf {
    Path::new(dev_artifacts_dir)
        .join("workflows")
        .join(workflow_deployment_id.to_string())
}

/// Reads `manifest.json` and every file directly under `code/` (v1 artifacts
/// are flat — no nested asset folders), hashing each with SHA-256.
/// `code_hash` covers file names too, so renaming a file inside the bundle
/// changes the hash even if no byte content changed.
pub async fn hash_artifact(dir: &Path) -> Result<HashedArtifact, String> {
    let manifest_bytes = tokio::fs::read(dir.join("manifest.json"))
        .await
        .map_err(|e| format!("cannot read manifest.json: {e}"))?;
    let manifest_hash = to_hex(&Sha256::digest(&manifest_bytes));

    let code_dir = dir.join("code");
    let mut entries = tokio::fs::read_dir(&code_dir)
        .await
        .map_err(|e| format!("cannot read code/ directory: {e}"))?;
    let mut names = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("cannot list code/ directory: {e}"))?
    {
        let is_file = entry
            .file_type()
            .await
            .map(|t| t.is_file())
            .unwrap_or(false);
        if is_file {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    if names.is_empty() {
        return Err("code/ directory is empty".to_string());
    }
    names.sort();

    let mut hasher = Sha256::new();
    for name in &names {
        hasher.update(name.as_bytes());
        hasher.update([0u8]);
        let bytes = tokio::fs::read(code_dir.join(name))
            .await
            .map_err(|e| format!("cannot read code/{name}: {e}"))?;
        hasher.update(&bytes);
    }
    let code_hash = to_hex(&hasher.finalize());

    Ok(HashedArtifact {
        manifest_hash,
        code_hash,
    })
}
