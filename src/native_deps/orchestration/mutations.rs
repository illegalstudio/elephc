//! Purpose:
//! Implements native dependency commands that transactionally mutate project manifests and locks.
//!
//! Called from:
//! - `crate::native_deps::orchestration::run_native_command_with()`.
//!
//! Key details:
//! - Artifact materialization succeeds before project files are atomically published.

use std::path::Path;

use crate::native_deps::catalog;
use crate::native_deps::cli::NativeOptions;
use crate::native_deps::download::Downloader;
use crate::native_deps::error::{NativeError, NativeErrorKind};
use crate::native_deps::lockfile::{declare_transitive_dependencies, NativeLock};
use crate::native_deps::manifest::ManifestDocument;
use crate::native_deps::materialize::materialize_manifest;
use crate::native_deps::recipe::RecipeRunner;
use crate::native_deps::toolchain::ToolchainProvider;
use crate::native_deps::util::atomic_write;

use super::support::{project_cache, publish_project, required_project, selected_target, success};
use super::NativeRunOutput;

/// Declares and materializes one exact package before publishing project files.
pub(super) fn add(
    package: &str,
    requested: Option<&str>,
    options: &NativeOptions,
    cwd: &Path,
    downloader: &dyn Downloader,
    recipes: &dyn RecipeRunner,
    toolchains: &dyn ToolchainProvider,
) -> Result<NativeRunOutput, NativeError> {
    let version = catalog::version(package, requested)?;
    let project = required_project(cwd, options.manifest_path.as_deref(), true)?;
    let target = selected_target(options);
    let recovery = format!(
        "elephc native add {package}@{} --target {}",
        version.version,
        target.as_str()
    );
    let cache = project_cache(cwd, &project, &recovery)?;
    let _project_lock = cache.lock(&cache.project_lock_path(&project.manifest), "add")?;
    let mut manifest = if project.manifest.exists() { ManifestDocument::load(&project.manifest)? } else { ManifestDocument::new() };
    if let Some(existing) = manifest.dependencies().get(package) {
        if existing != version.version {
            return Err(NativeError::new(NativeErrorKind::Manifest, format!("native package '{package}' is already declared at {existing}; use elephc native update {package}@{}", version.version)));
        }
    }
    manifest.set_dependency(package, version.version)?;
    // Never require the caller to hand-list a catalog dependency (e.g. `openssl`/`zlib` for
    // `curl`): declare every transitive package directly onto the manifest that gets published.
    declare_transitive_dependencies(&mut manifest).map_err(|error| {
        error
            .with_project(&project.root)
            .with_default_recovery(&recovery)
    })?;
    let lock = NativeLock::from_manifest(&manifest).map_err(|error| {
        error
            .with_project(&project.root)
            .with_default_recovery(&recovery)
    })?;
    materialize_manifest(
        &manifest,
        target,
        options.offline,
        &cache,
        downloader,
        recipes,
        toolchains,
    )
    .map_err(|error| {
        error
            .with_project(&project.root)
            .with_default_recovery(&recovery)
    })?;
    publish_project(&project, &manifest, &lock)?;
    Ok(success(format!(
        "added {package}@{} for {}\nproject: {}\n",
        version.version,
        target.as_str(),
        project.root.display()
    )))
}

/// Reconciles or validates a project lock and materializes every selected package.
pub(super) fn install(
    locked: bool,
    options: &NativeOptions,
    cwd: &Path,
    downloader: &dyn Downloader,
    recipes: &dyn RecipeRunner,
    toolchains: &dyn ToolchainProvider,
) -> Result<NativeRunOutput, NativeError> {
    let project = required_project(cwd, options.manifest_path.as_deref(), false)?;
    let target = selected_target(options);
    let reconcile_recovery =
        format!("elephc native install --target {}", target.as_str());
    let artifact_recovery = format!(
        "elephc native install --locked --target {}",
        target.as_str()
    );
    let cache = project_cache(cwd, &project, &reconcile_recovery)?;
    let _project_lock = if locked { None } else { Some(cache.lock(&cache.project_lock_path(&project.manifest), "install")?) };
    let manifest = ManifestDocument::load(&project.manifest)?;
    let desired = if locked {
        let current = NativeLock::load(&project.lock).map_err(|_| {
            NativeError::new(
                NativeErrorKind::Lock,
                "--locked requires an existing current elephc.lock",
            )
            .with_path(&project.lock)
            .with_project(&project.root)
            .with_recovery(&reconcile_recovery)
        })?;
        current.validate_current(&manifest).map_err(|error| {
            error
                .with_path(&project.lock)
                .with_project(&project.root)
                .with_default_recovery(&reconcile_recovery)
        })?;
        None
    } else {
        Some(NativeLock::from_manifest(&manifest).map_err(|error| {
            error
                .with_project(&project.root)
                .with_default_recovery(&reconcile_recovery)
        })?)
    };
    materialize_manifest(
        &manifest,
        target,
        options.offline,
        &cache,
        downloader,
        recipes,
        toolchains,
    )
    .map_err(|error| {
        error
            .with_project(&project.root)
            .with_default_recovery(if locked {
                &artifact_recovery
            } else {
                &reconcile_recovery
            })
    })?;
    if let Some(desired) = desired {
        atomic_write(&project.lock, desired.render()?.as_bytes())?;
    }
    Ok(success(format!(
        "installed {} native package(s) for {}{}\n",
        manifest.dependencies().len(),
        target.as_str(),
        if options.offline { " (offline)" } else { "" }
    )))
}

/// Refreshes one or every declaration from the current catalog before transactional publication.
pub(super) fn update(
    package: Option<&str>,
    requested: Option<&str>,
    options: &NativeOptions,
    cwd: &Path,
    downloader: &dyn Downloader,
    recipes: &dyn RecipeRunner,
    toolchains: &dyn ToolchainProvider,
) -> Result<NativeRunOutput, NativeError> {
    let project = required_project(cwd, options.manifest_path.as_deref(), false)?;
    let target = selected_target(options);
    let cache = project_cache(
        cwd,
        &project,
        &format!("elephc native update --target {}", target.as_str()),
    )?;
    let _project_lock = cache.lock(&cache.project_lock_path(&project.manifest), "update")?;
    let mut manifest = ManifestDocument::load(&project.manifest)?;
    if let Some(package) = package {
        if !manifest.dependencies().contains_key(package) {
            return Err(
                NativeError::new(
                    NativeErrorKind::Manifest,
                    format!("native package '{package}' is not declared"),
                )
                .with_path(&project.manifest)
                .with_project(&project.root)
                .with_recovery(format!("elephc native add {package}")),
            );
        }
        let version = catalog::version(package, requested)?;
        manifest.set_dependency(package, version.version)?;
    } else {
        let names = manifest.dependencies().keys().cloned().collect::<Vec<_>>();
        for name in names {
            let version = catalog::version(&name, None)?;
            manifest.set_dependency(&name, version.version)?;
        }
    }
    let selector = package
        .map(|name| {
            let version = manifest
                .dependencies()
                .get(name)
                .expect("updated package remains declared");
            format!("{name}@{version}")
        })
        .unwrap_or_default();
    let recovery = if selector.is_empty() {
        format!("elephc native update --target {}", target.as_str())
    } else {
        format!(
            "elephc native update {selector} --target {}",
            target.as_str()
        )
    };
    // Never require the caller to hand-list a catalog dependency (e.g. `openssl`/`zlib` for
    // `curl`): declare every transitive package directly onto the manifest that gets published.
    declare_transitive_dependencies(&mut manifest).map_err(|error| {
        error
            .with_project(&project.root)
            .with_default_recovery(&recovery)
    })?;
    let lock = NativeLock::from_manifest(&manifest).map_err(|error| {
        error
            .with_project(&project.root)
            .with_default_recovery(&recovery)
    })?;
    materialize_manifest(
        &manifest,
        target,
        options.offline,
        &cache,
        downloader,
        recipes,
        toolchains,
    )
    .map_err(|error| {
        error
            .with_project(&project.root)
            .with_default_recovery(&recovery)
    })?;
    publish_project(&project, &manifest, &lock)?;
    Ok(success(format!(
        "updated {} native package(s) for {}\n",
        if package.is_some() {
            1
        } else {
            manifest.dependencies().len()
        },
        target.as_str()
    )))
}

/// Removes one declaration and lock entry without touching the shared artifact cache.
pub(super) fn remove(package: &str, manifest_path: Option<&Path>, cwd: &Path) -> Result<NativeRunOutput, NativeError> {
    catalog::package(package)?;
    let project = required_project(cwd, manifest_path, false)?;
    let cache = project_cache(cwd, &project, &format!("elephc native remove {package}"))?;
    let _project_lock = cache.lock(&cache.project_lock_path(&project.manifest), "remove")?;
    let mut manifest = ManifestDocument::load(&project.manifest)?;
    if !manifest.remove_dependency(package) {
        return Err(
            NativeError::new(
                NativeErrorKind::Manifest,
                format!("native package '{package}' is not declared"),
            )
            .with_path(&project.manifest)
            .with_project(&project.root)
            .with_recovery(format!("elephc native add {package}")),
        );
    }
    let lock = NativeLock::from_manifest(&manifest)
        .map_err(|error| error.with_project(&project.root))?;
    publish_project(&project, &manifest, &lock)?;
    Ok(success(format!("removed {package}; shared cached artifacts were retained\n")))
}

