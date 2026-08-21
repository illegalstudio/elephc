//! Purpose:
//! Materializes and atomically publishes exact native package artifacts.
//!
//! Called from:
//! - `crate::native_deps::orchestration` after project and lock validation.
//!
//! Key details:
//! - Source download, extraction, recipe execution, receipt verification, and publication stay
//!   behind exact source/artifact locks and never run during ordinary compilation.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::codegen_support::platform::Target;

use super::archive::extract_tar_gz;
use super::cache::{publish_artifact, remove_exact_node, ArtifactKey, CacheLayout};
use super::catalog::{self, PackageVersion};
use super::download::{ensure_source, Downloader};
use super::error::{NativeError, NativeErrorKind};
use super::manifest::ManifestDocument;
use super::receipt::{collect_outputs, ArtifactReceipt, ReceiptIdentity};
use super::recipe::{RecipeRequest, RecipeRunner};
use super::toolchain::{NativeToolchain, ToolchainProvider};
use super::util::unique_sibling;

/// Materializes every declared package for one target after toolchain preflight, building each
/// package's catalog dependencies before the package itself so a recipe like `curl` can link
/// against its already-built `openssl`/`zlib` prefixes.
pub(super) fn materialize_manifest(
    manifest: &ManifestDocument,
    target: Target,
    offline: bool,
    cache: &CacheLayout,
    downloader: &dyn Downloader,
    recipes: &dyn RecipeRunner,
    toolchains: &dyn ToolchainProvider,
) -> Result<(), NativeError> {
    let toolchain = toolchains.resolve(target)?;
    toolchain.verify_compatibility(&cache.root)?;
    let mut prefixes: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut visiting: BTreeSet<String> = BTreeSet::new();
    for name in manifest.dependencies().keys() {
        materialize_with_dependencies(
            name,
            manifest,
            target,
            offline,
            cache,
            downloader,
            recipes,
            &toolchain,
            &mut prefixes,
            &mut visiting,
        )?;
    }
    Ok(())
}

/// Materializes one manifest-declared package after recursively materializing every catalog
/// dependency it lists, threading each dependency's final artifact prefix into the package's own
/// `RecipeRequest`. Reused prefixes are cached across the whole manifest walk.
#[allow(clippy::too_many_arguments)]
fn materialize_with_dependencies(
    name: &str,
    manifest: &ManifestDocument,
    target: Target,
    offline: bool,
    cache: &CacheLayout,
    downloader: &dyn Downloader,
    recipes: &dyn RecipeRunner,
    toolchain: &NativeToolchain,
    prefixes: &mut BTreeMap<String, PathBuf>,
    visiting: &mut BTreeSet<String>,
) -> Result<PathBuf, NativeError> {
    if let Some(existing) = prefixes.get(name) {
        return Ok(existing.clone());
    }
    if !visiting.insert(name.to_string()) {
        return Err(NativeError::new(
            NativeErrorKind::Catalog,
            format!("native package '{name}' has a circular catalog dependency"),
        ));
    }
    let selected = manifest.dependencies().get(name).ok_or_else(|| {
        NativeError::new(
            NativeErrorKind::Manifest,
            format!("native package '{name}' is not declared"),
        )
    })?;
    let version = catalog::version(name, Some(selected))?;
    catalog::ensure_target(version, target)?;
    let mut dependency_prefixes: BTreeMap<String, PathBuf> = BTreeMap::new();
    for dependency in version.dependencies {
        let path = materialize_with_dependencies(
            dependency,
            manifest,
            target,
            offline,
            cache,
            downloader,
            recipes,
            toolchain,
            prefixes,
            visiting,
        )?;
        dependency_prefixes.insert((*dependency).to_string(), path);
    }
    let path = materialize_package(
        name,
        version,
        target,
        offline,
        cache,
        downloader,
        recipes,
        toolchain,
        &dependency_prefixes,
    )?;
    visiting.remove(name);
    prefixes.insert(name.to_string(), path.clone());
    Ok(path)
}

/// Reuses or transactionally builds one exact artifact under its advisory lock.
#[allow(clippy::too_many_arguments)]
pub(super) fn materialize_package(
    package: &str,
    version: &'static PackageVersion,
    target: Target,
    offline: bool,
    cache: &CacheLayout,
    downloader: &dyn Downloader,
    recipes: &dyn RecipeRunner,
    toolchain: &NativeToolchain,
    dependency_prefixes: &BTreeMap<String, PathBuf>,
) -> Result<PathBuf, NativeError> {
    let key = ArtifactKey {
        package,
        version: version.version,
        recipe: version.recipe_revision,
        source_sha256: version.source.sha256,
        target: target.as_str(),
        abi: &toolchain.abi,
        toolchain_fingerprint: &toolchain.fingerprint,
    };
    let final_path = cache.artifact_path(&key)?;
    let _artifact_lock = cache.lock(&cache.artifact_lock_path(&key)?, "install-artifact")?;
    cleanup_stale_staging(&final_path)?;
    let retained = version
        .retained_headers
        .iter()
        .chain(version.ordered_link_outputs.iter())
        .copied()
        .collect::<Vec<_>>();
    let identity = ReceiptIdentity {
        package,
        version: version.version,
        recipe: version.recipe_revision,
        source_sha256: version.source.sha256,
        target: target.as_str(),
        abi: &toolchain.abi,
        toolchain_fingerprint: &toolchain.fingerprint,
        required_outputs: &retained,
    };
    let existing_valid = ArtifactReceipt::load(&final_path)
        .and_then(|receipt| receipt.verify(&final_path, &identity))
        .is_ok();
    if existing_valid {
        return Ok(final_path);
    }

    let source_path = cache.source_path(version.source.sha256);
    {
        let _source_lock =
            cache.lock(&cache.source_lock_path(version.source.sha256), "download-source")?;
        ensure_source(&source_path, &version.source, offline, downloader)?;
    }
    let parent = final_path
        .parent()
        .ok_or_else(|| NativeError::new(NativeErrorKind::Cache, "artifact path has no parent"))?;
    fs::create_dir_all(parent)
        .map_err(|error| NativeError::io("create artifact parent", parent, error))?;
    let staging = unique_sibling(&final_path, "stage");
    fs::create_dir(&staging)
        .map_err(|error| NativeError::io("create artifact staging", &staging, error))?;
    let result = (|| {
        let extracted = staging.join(".source");
        extract_tar_gz(&source_path, &extracted)?;
        recipes.build(&RecipeRequest {
            package,
            version,
            target,
            source: &extracted,
            staging_prefix: &staging,
            toolchain,
            dependency_prefixes,
        })?;
        fs::remove_dir_all(&extracted).map_err(|error| {
            NativeError::io("remove extracted native source tree", &extracted, error)
        })?;
        assert_staging_contents(&staging, &retained)?;
        let receipt = ArtifactReceipt {
            schema: 1,
            package: package.to_string(),
            version: version.version.to_string(),
            recipe: version.recipe_revision,
            source_sha256: version.source.sha256.to_string(),
            target: target.as_str().to_string(),
            abi: toolchain.abi.clone(),
            compiler: toolchain.compiler.clone(),
            archiver: toolchain.archiver.clone(),
            ranlib: toolchain.ranlib_identity.clone(),
            toolchain_fingerprint: toolchain.fingerprint.clone(),
            outputs: collect_outputs(&staging, &retained)?,
            created_by: env!("CARGO_PKG_VERSION").to_string(),
        };
        receipt.write(&staging)?;
        receipt.verify(&staging, &identity)?;
        publish_artifact(&staging, &final_path, false)?;
        Ok(final_path.clone())
    })();
    if result.is_err() && staging.exists() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

/// Ensures the final staging tree contains only catalog-retained regular files and directories.
pub(super) fn assert_staging_contents(
    staging: &Path,
    retained: &[&str],
) -> Result<(), NativeError> {
    let expected = retained
        .iter()
        .map(|path| (*path).to_string())
        .collect::<BTreeSet<_>>();
    let expected_directories = expected_directories(&expected);
    let mut actual = BTreeSet::new();
    let mut actual_directories = BTreeSet::new();
    collect_staging_files(
        staging,
        staging,
        &mut actual,
        &mut actual_directories,
    )?;
    if actual != expected || actual_directories != expected_directories {
        return Err(NativeError::new(
            NativeErrorKind::Integrity,
            format!(
                "trusted recipe staging is not exact: expected files {expected:?} and directories {expected_directories:?}, got files {actual:?} and directories {actual_directories:?}"
            ),
        )
        .with_path(staging));
    }
    Ok(())
}

/// Recursively collects only non-symlink regular files beneath an exact staging root.
fn collect_staging_files(
    root: &Path,
    directory: &Path,
    output: &mut BTreeSet<String>,
    directories: &mut BTreeSet<String>,
) -> Result<(), NativeError> {
    for entry in fs::read_dir(directory)
        .map_err(|error| NativeError::io("inspect recipe staging", directory, error))?
    {
        let entry = entry
            .map_err(|error| NativeError::io("read recipe staging entry", directory, error))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|error| {
            NativeError::io("inspect recipe staging entry type", &path, error)
        })?;
        if kind.is_symlink() {
            return Err(
                NativeError::new(
                    NativeErrorKind::Integrity,
                    "trusted recipe staging contains a symlink",
                )
                .with_path(path),
            );
        }
        if kind.is_dir() {
            let relative = path
                .strip_prefix(root)
                .expect("staging directory below root");
            directories.insert(
                relative
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/"),
            );
            collect_staging_files(root, &path, output, directories)?;
        } else if kind.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("staging child rooted below staging");
            output.insert(
                relative
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/"),
            );
        } else {
            return Err(
                NativeError::new(
                    NativeErrorKind::Integrity,
                    "trusted recipe staging contains a special file",
                )
                .with_path(path),
            );
        }
    }
    Ok(())
}

/// Derives the only allowed directory set from parent components of retained outputs.
fn expected_directories(files: &BTreeSet<String>) -> BTreeSet<String> {
    let mut directories = BTreeSet::new();
    for file in files {
        let mut parent = Path::new(file).parent();
        while let Some(path) = parent.filter(|path| !path.as_os_str().is_empty()) {
            directories.insert(
                path.to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/"),
            );
            parent = path.parent();
        }
    }
    directories
}

/// Removes only exact-key staging siblings older than 24 hours while holding the artifact lock.
pub(super) fn cleanup_stale_staging(final_path: &Path) -> Result<(), NativeError> {
    let Some(parent) = final_path.parent() else {
        return Ok(());
    };
    let Some(name) = final_path.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    let prefix = format!(".{name}.stage.");
    let Ok(entries) = fs::read_dir(parent) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let entry_name = entry.file_name().to_string_lossy().into_owned();
        if !entry_name.starts_with(&prefix) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
            NativeError::io("inspect stale artifact staging", &entry.path(), error)
        })?;
        let old = metadata
            .modified()
            .ok()
            .and_then(|modified| SystemTime::now().duration_since(modified).ok())
            .is_some_and(|age| age >= Duration::from_secs(24 * 60 * 60));
        if old {
            remove_exact_node(&entry.path())?;
        }
    }
    Ok(())
}
