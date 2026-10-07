//! Purpose:
//! Explicitly prunes abandoned staging and obsolete native artifact cache entries.
//!
//! Called from:
//! - `elephc native prune` orchestration.
//!
//! Key details:
//! - Removal is global-cache-only, uses installer-compatible artifact locks, and never changes a
//!   project manifest or lockfile.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::codegen_support::platform::Target;

use super::cache::{remove_exact_node, CacheLayout};
use super::catalog;
use super::error::NativeError;
use super::receipt::ArtifactReceipt;
use super::toolchain::NativeToolchain;

const ABANDONED_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Counts exact nodes removed by one explicit prune command.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PruneReport {
    pub removed_artifacts: usize,
    pub removed_staging: usize,
    pub reclaimed_bytes: u64,
}

/// Removes orphan artifacts, stale selected-toolchain fingerprints, and abandoned staging.
pub fn prune_cache(
    cache: &CacheLayout,
    target: Target,
    toolchain: &NativeToolchain,
) -> Result<PruneReport, NativeError> {
    if !cache.artifacts.exists() {
        return Ok(PruneReport::default());
    }
    let mut staging = Vec::new();
    let mut artifacts = Vec::new();
    collect_candidates(&cache.artifacts, &mut staging, &mut artifacts)?;

    let mut report = PruneReport::default();
    for path in staging {
        let Some(final_path) = final_path_for_staging(&path) else {
            continue;
        };
        let _lock = cache.lock(
            &cache.artifact_lock_path_for_final(&final_path)?,
            "prune-staging",
        )?;
        if !is_abandoned(&path)? {
            continue;
        }
        report.reclaimed_bytes = report
            .reclaimed_bytes
            .saturating_add(approximate_node_size(&path));
        remove_exact_node(&path)?;
        report.removed_staging += 1;
        remove_empty_ancestors(path.parent(), &cache.artifacts);
    }

    for path in artifacts {
        let Ok(receipt) = ArtifactReceipt::load(&path) else {
            continue;
        };
        if !is_orphan(&path, cache, &receipt)
            && !is_stale_toolchain(&receipt, target, toolchain)
        {
            continue;
        }
        let _lock = cache.lock(
            &cache.artifact_lock_path_for_final(&path)?,
            "prune-artifact",
        )?;
        let Ok(current) = ArtifactReceipt::load(&path) else {
            continue;
        };
        if !is_orphan(&path, cache, &current)
            && !is_stale_toolchain(&current, target, toolchain)
        {
            continue;
        }
        report.reclaimed_bytes = report
            .reclaimed_bytes
            .saturating_add(approximate_node_size(&path));
        remove_exact_node(&path)?;
        report.removed_artifacts += 1;
        remove_empty_ancestors(path.parent(), &cache.artifacts);
    }
    Ok(report)
}

/// Collects publication siblings and receipt-bearing artifact roots without following symlinks.
fn collect_candidates(
    directory: &Path,
    staging: &mut Vec<PathBuf>,
    artifacts: &mut Vec<PathBuf>,
) -> Result<(), NativeError> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(NativeError::io(
                "inspect native cache for pruning",
                directory,
                error,
            ));
        }
    };
    for entry in entries {
        let entry = entry
            .map_err(|error| NativeError::io("read native cache entry", directory, error))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|error| {
            NativeError::io("inspect native cache entry type", &path, error)
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.contains(".stage.") || name.contains(".quarantine.") {
            staging.push(path);
            continue;
        }
        if !kind.is_dir() || kind.is_symlink() {
            continue;
        }
        if path.join("receipt.json").is_file() {
            artifacts.push(path);
            continue;
        }
        collect_candidates(&path, staging, artifacts)?;
    }
    Ok(())
}

/// Maps a unique staging/quarantine sibling back to the installer final path it belongs to.
fn final_path_for_staging(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let name = name.strip_prefix('.')?;
    let marker = [".stage.", ".quarantine."]
        .into_iter()
        .filter_map(|marker| name.find(marker).map(|index| (index, marker)))
        .min_by_key(|(index, _)| *index)?;
    let final_name = &name[..marker.0];
    (!final_name.is_empty()).then(|| path.parent().unwrap_or_else(|| Path::new(".")).join(final_name))
}

/// Returns whether a publication sibling is old enough to be abandoned.
fn is_abandoned(path: &Path) -> Result<bool, NativeError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(NativeError::io(
                "inspect abandoned native staging",
                path,
                error,
            ));
        }
    };
    Ok(metadata
        .modified()
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age >= ABANDONED_AGE))
}

/// Detects an artifact whose package/version/recipe/source/path no longer matches the catalog.
fn is_orphan(path: &Path, cache: &CacheLayout, receipt: &ArtifactReceipt) -> bool {
    let Ok(version) = catalog::version(&receipt.package, Some(&receipt.version)) else {
        return true;
    };
    if receipt.recipe != version.recipe_revision
        || receipt.source_sha256 != version.source.sha256
        || !version
            .supported_targets
            .iter()
            .any(|candidate| *candidate == receipt.target)
    {
        return true;
    }
    let key = super::cache::ArtifactKey {
        package: &receipt.package,
        version: &receipt.version,
        recipe: receipt.recipe,
        source_sha256: &receipt.source_sha256,
        target: &receipt.target,
        abi: &receipt.abi,
        toolchain_fingerprint: &receipt.toolchain_fingerprint,
    };
    cache
        .artifact_path(&key)
        .map_or(true, |expected| expected != path)
}

/// Detects an older fingerprint for the selected target and still-compatible ABI.
fn is_stale_toolchain(
    receipt: &ArtifactReceipt,
    target: Target,
    toolchain: &NativeToolchain,
) -> bool {
    receipt.target == target.as_str()
        && receipt.abi == toolchain.abi
        && receipt.toolchain_fingerprint != toolchain.fingerprint
}

/// Returns approximate regular-file bytes below one exact cache node.
fn approximate_node_size(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.file_type().is_symlink() {
        return 0;
    }
    if metadata.file_type().is_file() {
        return metadata.len();
    }
    if !metadata.file_type().is_dir() {
        return 0;
    }
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| approximate_node_size(&entry.path()))
        .fold(0_u64, u64::saturating_add)
}

/// Removes empty cache-key parents without crossing the configured artifacts root.
fn remove_empty_ancestors(mut directory: Option<&Path>, stop: &Path) {
    while let Some(path) = directory {
        if path == stop || !path.starts_with(stop) {
            break;
        }
        match fs::remove_dir(path) {
            Ok(()) => directory = path.parent(),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::NotFound
                ) =>
            {
                break;
            }
            Err(_) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::super::receipt::{collect_outputs, ToolIdentity};

    /// Creates one unique cache and matching current toolchain identity.
    fn fixture() -> (PathBuf, CacheLayout, NativeToolchain) {
        let root = std::env::temp_dir().join(format!(
            "elephc-prune-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cache =
            CacheLayout::from_values(&root, Some(root.join("cache").as_os_str()), None, None)
                .unwrap();
        let identity = ToolIdentity {
            command: "tool".into(),
            version: "current".into(),
        };
        let toolchain = NativeToolchain {
            cc: "cc".into(),
            ar: "ar".into(),
            ranlib: "ranlib".into(),
            mingw_sysroot: None,
            target_tuple: "fixture".into(),
            abi: "fixture-abi".into(),
            fingerprint: "current".into(),
            compiler: identity.clone(),
            archiver: identity.clone(),
            ranlib_identity: identity,
        };
        (root, cache, toolchain)
    }

    /// Writes one catalog-current artifact under a chosen toolchain fingerprint.
    fn write_artifact(
        cache: &CacheLayout,
        target: Target,
        fingerprint: &str,
    ) -> PathBuf {
        let version = catalog::version("zlib", None).unwrap();
        let key = super::super::cache::ArtifactKey {
            package: "zlib",
            version: version.version,
            recipe: version.recipe_revision,
            source_sha256: version.source.sha256,
            target: target.as_str(),
            abi: "fixture-abi",
            toolchain_fingerprint: fingerprint,
        };
        let artifact = cache.artifact_path(&key).unwrap();
        fs::create_dir_all(artifact.join("include")).unwrap();
        fs::create_dir_all(artifact.join("lib")).unwrap();
        fs::write(artifact.join("include/zlib.h"), b"zlib").unwrap();
        fs::write(artifact.join("include/zconf.h"), b"zconf").unwrap();
        fs::write(artifact.join("lib/libz.a"), b"archive").unwrap();
        let required = ["include/zlib.h", "include/zconf.h", "lib/libz.a"];
        let identity = ToolIdentity {
            command: "tool".into(),
            version: "fixture".into(),
        };
        ArtifactReceipt {
            schema: 1,
            package: "zlib".into(),
            version: version.version.into(),
            recipe: version.recipe_revision,
            source_sha256: version.source.sha256.into(),
            target: target.as_str().into(),
            abi: "fixture-abi".into(),
            compiler: identity.clone(),
            archiver: identity.clone(),
            ranlib: identity,
            toolchain_fingerprint: fingerprint.into(),
            outputs: collect_outputs(&artifact, &required).unwrap(),
            created_by: "test".into(),
        }
        .write(&artifact)
        .unwrap();
        artifact
    }

    /// Verifies prune removes old fingerprints and abandoned staging while retaining current data.
    #[test]
    fn prune_removes_only_explicit_stale_cache_state() {
        let (root, cache, toolchain) = fixture();
        let target = Target::detect_host();
        let current = write_artifact(&cache, target, "current");
        let stale = write_artifact(&cache, target, "old");
        let abandoned = stale
            .parent()
            .unwrap()
            .join(".old.stage.abandoned");
        fs::create_dir_all(&abandoned).unwrap();
        fs::write(abandoned.join("partial"), b"partial").unwrap();
        let times = fs::FileTimes::new()
            .set_modified(SystemTime::now() - Duration::from_secs(25 * 60 * 60));
        fs::File::open(&abandoned)
            .unwrap()
            .set_times(times)
            .unwrap();

        let report = prune_cache(&cache, target, &toolchain).unwrap();
        assert_eq!(report.removed_artifacts, 1);
        assert_eq!(report.removed_staging, 1);
        assert!(report.reclaimed_bytes > 0);
        assert!(current.is_dir());
        assert!(!stale.exists());
        assert!(!abandoned.exists());

        fs::remove_dir_all(root).unwrap();
    }
}
