//! Purpose:
//! Produces deterministic read-only native project and artifact health reports.
//!
//! Called from:
//! - `native list` and `native doctor` orchestration.
//!
//! Key details:
//! - Inspection never creates cache directories, locks, staging paths, or project files.

use std::path::Path;

use crate::codegen_support::platform::Target;

use super::cache::{ArtifactKey, CacheLayout};
use super::catalog;
use super::error::NativeError;
use super::lockfile::NativeLock;
use super::manifest::ManifestDocument;
use super::project::ProjectPaths;
use super::receipt::{ArtifactReceipt, ReceiptIdentity};
use super::toolchain::ToolchainProvider;

/// Deterministic package health state displayed by list and doctor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageHealth {
    Installed,
    Missing,
    Corrupt,
    Stale,
    ToolchainError,
}

impl PackageHealth {
    /// Returns the frozen lowercase CLI label for this state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Missing => "missing",
            Self::Corrupt => "corrupt",
            Self::Stale => "stale",
            Self::ToolchainError => "toolchain-error",
        }
    }
}

/// Inspects all declared packages without mutating project or cache state.
pub fn inspect(
    project: &ProjectPaths,
    target: Target,
    cache: &CacheLayout,
    toolchains: &dyn ToolchainProvider,
) -> Result<Vec<(String, String, Option<String>, String, PackageHealth)>, NativeError> {
    let manifest = ManifestDocument::load(&project.manifest)?;
    let lock = NativeLock::load(&project.lock).ok();
    let lock_current = lock.as_ref().is_some_and(|lock| lock.validate_current(&manifest).is_ok());
    let toolchain = toolchains.resolve(target);
    let mut rows = Vec::new();
    for (name, version_name) in manifest.dependencies() {
        let locked_version = lock.as_ref().and_then(|lock| lock.package(name)).map(|package| package.version.clone());
        let (abi, health) = match &toolchain {
            Err(_) => ("unresolved".to_string(), PackageHealth::ToolchainError),
            Ok(toolchain) if !lock_current => (toolchain.abi.clone(), PackageHealth::Stale),
            Ok(toolchain) => {
                let version = catalog::version(name, Some(version_name))?;
                let key = ArtifactKey { package: name, version: version.version, recipe: version.recipe_revision, source_sha256: version.source.sha256, target: target.as_str(), abi: &toolchain.abi, toolchain_fingerprint: &toolchain.fingerprint };
                let root = cache.artifact_path(&key)?;
                if !root.exists() {
                    (toolchain.abi.clone(), PackageHealth::Missing)
                } else {
                    let retained = version.retained_headers.iter().chain(version.ordered_link_outputs.iter()).copied().collect::<Vec<_>>();
                    let identity = ReceiptIdentity { package: name, version: version.version, recipe: version.recipe_revision, source_sha256: version.source.sha256, target: target.as_str(), abi: &toolchain.abi, toolchain_fingerprint: &toolchain.fingerprint, required_outputs: &retained };
                    let valid = ArtifactReceipt::load(&root).and_then(|receipt| receipt.verify(&root, &identity)).is_ok();
                    (toolchain.abi.clone(), if valid { PackageHealth::Installed } else { PackageHealth::Corrupt })
                }
            }
        };
        rows.push((name.clone(), version_name.clone(), locked_version, abi, health));
    }
    Ok(rows)
}

/// Reports stale staging siblings without deleting them.
pub fn stale_staging_paths(cache: &CacheLayout) -> Vec<String> {
    let mut paths = Vec::new();
    collect_staging(&cache.artifacts, &mut paths);
    paths.sort();
    paths
}

/// Recursively collects staging/quarantine diagnostics without following symlinks.
fn collect_staging(root: &Path, output: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(root) else { return; };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.contains(".stage.") || name.contains(".quarantine.") {
            output.push(path.display().to_string());
        }
        if entry.file_type().is_ok_and(|kind| kind.is_dir() && !kind.is_symlink()) {
            collect_staging(&path, output);
        }
    }
}
