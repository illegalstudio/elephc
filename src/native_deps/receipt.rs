//! Purpose:
//! Records and verifies local native artifact identity and every retained output digest.
//!
//! Called from:
//! - Cache publication, list/doctor, and compilation-time exact archive resolution.
//!
//! Key details:
//! - Unknown schemas and any size/hash mismatch invalidate the whole artifact.

use std::collections::BTreeSet;
use std::fs;
use std::io::Read;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use super::error::{NativeError, NativeErrorKind};
use super::util::{atomic_write, hash_file};

/// Maximum accepted receipt size, far above the deterministic v1 schema output.
const MAX_RECEIPT_BYTES: u64 = 1024 * 1024;

/// Deterministic local receipt for one target/toolchain artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReceipt {
    pub schema: u32,
    pub package: String,
    pub version: String,
    pub recipe: u32,
    pub source_sha256: String,
    pub target: String,
    pub abi: String,
    pub compiler: ToolIdentity,
    pub archiver: ToolIdentity,
    pub ranlib: ToolIdentity,
    pub toolchain_fingerprint: String,
    pub outputs: Vec<ReceiptOutput>,
    pub created_by: String,
}

/// Diagnostic command identity recorded in a receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolIdentity {
    pub command: String,
    pub version: String,
}

/// Hash-bound retained file in an installed artifact.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReceiptOutput {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

impl ArtifactReceipt {
    /// Reads a strict schema-1 receipt from an artifact directory.
    pub fn load(artifact: &Path) -> Result<Self, NativeError> {
        let artifact_metadata = fs::symlink_metadata(artifact).map_err(|error| NativeError::io("inspect native artifact root", artifact, error))?;
        if !artifact_metadata.file_type().is_dir() || artifact_metadata.file_type().is_symlink() {
            return Err(NativeError::new(NativeErrorKind::Integrity, "native artifact root is not a non-symlink directory").with_path(artifact));
        }
        let path = artifact.join("receipt.json");
        let metadata = fs::symlink_metadata(&path).map_err(|error| NativeError::io("inspect native artifact receipt", &path, error))?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(NativeError::new(NativeErrorKind::Integrity, "native artifact receipt is not a regular non-symlink file").with_path(&path));
        }
        if metadata.len() == 0 || metadata.len() > MAX_RECEIPT_BYTES {
            return Err(NativeError::new(NativeErrorKind::Integrity, format!("native artifact receipt size must be between 1 and {MAX_RECEIPT_BYTES} bytes")).with_path(&path));
        }
        let file = fs::File::open(&path).map_err(|error| NativeError::io("open native artifact receipt", &path, error))?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAX_RECEIPT_BYTES + 1).read_to_end(&mut bytes)
            .map_err(|error| NativeError::io("read native artifact receipt", &path, error))?;
        if bytes.len() as u64 > MAX_RECEIPT_BYTES {
            return Err(NativeError::new(NativeErrorKind::Integrity, format!("native artifact receipt exceeds {MAX_RECEIPT_BYTES} byte limit")).with_path(&path));
        }
        let receipt: Self = serde_json::from_slice(&bytes).map_err(|error| NativeError::new(NativeErrorKind::Integrity, format!("invalid native receipt JSON: {error}")).with_path(&path))?;
        if receipt.schema != 1 {
            return Err(NativeError::new(NativeErrorKind::Integrity, format!("unsupported native receipt schema {}", receipt.schema)).with_path(path));
        }
        Ok(receipt)
    }

    /// Writes the receipt as stable pretty JSON after all output verification succeeds.
    pub fn write(&self, artifact: &Path) -> Result<(), NativeError> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| NativeError::new(NativeErrorKind::Integrity, format!("cannot encode native receipt: {error}")))?;
        let mut with_newline = bytes;
        with_newline.push(b'\n');
        atomic_write(&artifact.join("receipt.json"), &with_newline)
    }

    /// Verifies receipt identity and every retained regular-file size and digest.
    pub fn verify(&self, artifact: &Path, expected: &ReceiptIdentity<'_>) -> Result<(), NativeError> {
        if self.schema != 1
            || self.package != expected.package
            || self.version != expected.version
            || self.recipe != expected.recipe
            || self.source_sha256 != expected.source_sha256
            || self.target != expected.target
            || self.abi != expected.abi
            || self.toolchain_fingerprint != expected.toolchain_fingerprint
        {
            return Err(NativeError::new(NativeErrorKind::Integrity, "native artifact receipt identity is stale or incompatible").with_path(artifact));
        }
        let required = expected.required_outputs.iter().map(|path| (*path).to_string()).collect::<BTreeSet<_>>();
        let recorded = self.outputs.iter().map(|output| output.path.clone()).collect::<BTreeSet<_>>();
        if recorded.len() != self.outputs.len() || recorded != required {
            return Err(NativeError::new(
                NativeErrorKind::Integrity,
                format!("native receipt output set is not exact: expected {required:?}, got {recorded:?}"),
            ).with_path(artifact));
        }
        for output in &self.outputs {
            let relative = validate_relative_output(&output.path)?;
            let path = artifact.join(relative);
            let metadata = fs::symlink_metadata(&path).map_err(|error| NativeError::io("inspect native artifact output", &path, error))?;
            if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
                return Err(NativeError::new(NativeErrorKind::Integrity, format!("native output '{}' is not a regular file", output.path)).with_path(path));
            }
            let (size, sha256) = hash_file(&path)?;
            if size != output.size || sha256 != output.sha256 {
                return Err(NativeError::new(NativeErrorKind::Integrity, format!("native output '{}' failed size/SHA-256 verification; run elephc native install --locked --target {}", output.path, expected.target)).with_path(path));
            }
        }
        verify_artifact_tree(artifact, &required)?;
        Ok(())
    }
}

/// Expected receipt identity derived only from catalog and selected toolchain.
pub struct ReceiptIdentity<'a> {
    pub package: &'a str,
    pub version: &'a str,
    pub recipe: u32,
    pub source_sha256: &'a str,
    pub target: &'a str,
    pub abi: &'a str,
    pub toolchain_fingerprint: &'a str,
    pub required_outputs: &'a [&'a str],
}

/// Verifies that a published artifact contains only receipt.json and exact catalog outputs.
fn verify_artifact_tree(artifact: &Path, required: &BTreeSet<String>) -> Result<(), NativeError> {
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    collect_artifact_files(artifact, artifact, &mut files, &mut directories)?;
    let mut expected = required.clone();
    expected.insert("receipt.json".to_string());
    let expected_directories = expected_directories(&expected);
    if files != expected || directories != expected_directories {
        return Err(NativeError::new(
            NativeErrorKind::Integrity,
            format!("native artifact tree is not exact: expected files {expected:?} and directories {expected_directories:?}, got files {files:?} and directories {directories:?}"),
        ).with_path(artifact));
    }
    Ok(())
}

/// Recursively collects regular files and rejects symlinks or special nodes in an artifact.
fn collect_artifact_files(root: &Path, directory: &Path, output: &mut BTreeSet<String>, directories: &mut BTreeSet<String>) -> Result<(), NativeError> {
    for entry in fs::read_dir(directory).map_err(|error| NativeError::io("inspect native artifact tree", directory, error))? {
        let entry = entry.map_err(|error| NativeError::io("read native artifact tree entry", directory, error))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|error| NativeError::io("inspect native artifact node", &path, error))?;
        if kind.is_symlink() {
            return Err(NativeError::new(NativeErrorKind::Integrity, "native artifact contains a symlink").with_path(path));
        }
        if kind.is_dir() {
            let relative = path.strip_prefix(root).expect("artifact directory below root");
            directories.insert(relative.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"));
            collect_artifact_files(root, &path, output, directories)?;
        } else if kind.is_file() {
            let relative = path.strip_prefix(root).expect("artifact child below root");
            output.insert(relative.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"));
        } else {
            return Err(NativeError::new(NativeErrorKind::Integrity, "native artifact contains a special file").with_path(path));
        }
    }
    Ok(())
}

/// Derives the complete allowed artifact directory set from retained file parents.
fn expected_directories(files: &BTreeSet<String>) -> BTreeSet<String> {
    let mut directories = BTreeSet::new();
    for file in files {
        let mut parent = Path::new(file).parent();
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            directories.insert(path.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"));
            parent = path.parent();
        }
    }
    directories
}

/// Creates sorted receipt output records for catalog-retained paths.
pub fn collect_outputs(artifact: &Path, paths: &[&str]) -> Result<Vec<ReceiptOutput>, NativeError> {
    let mut outputs = Vec::new();
    for path in paths {
        let relative = validate_relative_output(path)?;
        let absolute = artifact.join(relative);
        let metadata = fs::symlink_metadata(&absolute).map_err(|error| NativeError::io("inspect recipe output", &absolute, error))?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() || metadata.len() == 0 {
            return Err(NativeError::new(NativeErrorKind::Integrity, format!("recipe output '{path}' must be a non-empty regular file")).with_path(absolute));
        }
        let (size, sha256) = hash_file(&absolute)?;
        outputs.push(ReceiptOutput { path: (*path).to_string(), size, sha256 });
    }
    outputs.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(outputs)
}

/// Validates that a receipt output path cannot escape its artifact directory.
fn validate_relative_output(path: &str) -> Result<&Path, NativeError> {
    let path = Path::new(path);
    if path.is_absolute() || path.components().any(|component| !matches!(component, Component::Normal(_))) {
        return Err(NativeError::new(NativeErrorKind::Integrity, "receipt output path is not a safe relative path").with_path(path));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Creates a unique receipt fixture directory.
    fn fixture() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("elephc-receipt-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::write(root.join("lib/a.a"), b"archive").unwrap();
        root
    }

    /// Verifies output corruption is detected before an archive can be resolved.
    #[test]
    fn receipt_detects_output_corruption() {
        let root = fixture();
        let receipt = ArtifactReceipt {
            schema: 1, package: "pcre2".into(), version: "10.47".into(), recipe: 1,
            source_sha256: "source".into(), target: "macos-aarch64".into(), abi: "arm64-apple-darwin".into(),
            compiler: ToolIdentity { command: "cc".into(), version: "v".into() },
            archiver: ToolIdentity { command: "ar".into(), version: "v".into() },
            ranlib: ToolIdentity { command: "ranlib".into(), version: "v".into() }, toolchain_fingerprint: "fp".into(),
            outputs: collect_outputs(&root, &["lib/a.a"]).unwrap(), created_by: "test".into(),
        };
        receipt.write(&root).unwrap();
        let identity = ReceiptIdentity { package: "pcre2", version: "10.47", recipe: 1, source_sha256: "source", target: "macos-aarch64", abi: "arm64-apple-darwin", toolchain_fingerprint: "fp", required_outputs: &["lib/a.a"] };
        receipt.verify(&root, &identity).unwrap();
        let mut duplicate = receipt.clone();
        duplicate.outputs.push(duplicate.outputs[0].clone());
        assert!(duplicate.verify(&root, &identity).is_err());
        fs::write(root.join("unexpected"), b"extra").unwrap();
        assert!(receipt.verify(&root, &identity).is_err());
        fs::remove_file(root.join("unexpected")).unwrap();
        fs::create_dir(root.join("empty-build")).unwrap();
        assert!(receipt.verify(&root, &identity).is_err());
        fs::remove_dir(root.join("empty-build")).unwrap();
        fs::write(root.join("lib/a.a"), b"corrupt").unwrap();
        assert!(receipt.verify(&root, &identity).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies a symlinked artifact root can never be considered installed.
    #[test]
    #[cfg(unix)]
    fn receipt_rejects_symlinked_artifact_root() {
        let root = fixture();
        let link = root.with_extension("link");
        std::os::unix::fs::symlink(&root, &link).unwrap();
        assert!(ArtifactReceipt::load(&link).is_err());
        fs::remove_file(link).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies loading rejects a symlinked receipt before following its target.
    #[test]
    #[cfg(unix)]
    fn receipt_rejects_symlinked_receipt_file() {
        let root = fixture();
        let target = root.with_extension("external-receipt");
        fs::write(&target, b"{}").unwrap();
        std::os::unix::fs::symlink(&target, root.join("receipt.json")).unwrap();
        let error = ArtifactReceipt::load(&root).unwrap_err();
        assert_eq!(error.kind, NativeErrorKind::Integrity);
        assert!(error.message.contains("non-symlink"));
        fs::remove_dir_all(root).unwrap();
        fs::remove_file(target).unwrap();
    }

    /// Verifies loading never allocates or parses a receipt above the fixed bound.
    #[test]
    fn receipt_rejects_oversized_file() {
        let root = fixture();
        let receipt = root.join("receipt.json");
        let file = fs::File::create(&receipt).unwrap();
        file.set_len(MAX_RECEIPT_BYTES + 1).unwrap();
        let error = ArtifactReceipt::load(&root).unwrap_err();
        assert_eq!(error.kind, NativeErrorKind::Integrity);
        assert!(error.message.contains("receipt size"));
        fs::remove_dir_all(root).unwrap();
    }
}
