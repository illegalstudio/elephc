//! Purpose:
//! Coordinates transactional native command state transitions through injected services.
//!
//! Called from:
//! - `crate::native_deps::run_native_command` and deterministic unit tests.
//!
//! Key details:
//! - Compilation never enters this module; only explicit native commands may mutate cache or project state.

use std::path::Path;

use crate::codegen_support::platform::Target;

use super::cache::CacheLayout;
use super::catalog;
use super::cli::{NativeCommand, NativeOptions};
use super::doctor::{self, PackageHealth};
use super::download::Downloader;
use super::error::{recovery_from_project, NativeError, NativeErrorKind};
use super::lockfile::NativeLock;
use super::manifest::ManifestDocument;
use super::materialize::materialize_manifest;
use super::project::{discover_for_native, ProjectPaths};
use super::prune as cache_prune;
use super::recipe::RecipeRunner;
use super::toolchain::ToolchainProvider;
use super::util::atomic_write;

/// Captured stable command output and process status chosen by top-level integration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeRunOutput {
    pub stdout: String,
    pub exit_code: i32,
}

/// Executes a command through injected network, recipe, and toolchain services.
pub(crate) fn run_native_command_with(
    command: &NativeCommand,
    cwd: &Path,
    downloader: &dyn Downloader,
    recipes: &dyn RecipeRunner,
    toolchains: &dyn ToolchainProvider,
) -> Result<NativeRunOutput, NativeError> {
    match command {
        NativeCommand::Add { package, version, options } => add(package, version.as_deref(), options, cwd, downloader, recipes, toolchains),
        NativeCommand::Install { locked, options } => install(*locked, options, cwd, downloader, recipes, toolchains),
        NativeCommand::Update { package, version, options } => update(package.as_deref(), version.as_deref(), options, cwd, downloader, recipes, toolchains),
        NativeCommand::Remove { package, manifest_path } => remove(package, manifest_path.as_deref(), cwd),
        NativeCommand::List { options } => list(options, cwd, toolchains),
        NativeCommand::Doctor { options } => doctor(options, cwd, toolchains),
        NativeCommand::Prune { target } => prune(*target, cwd, toolchains),
    }
}

/// Declares and materializes one exact package before publishing project files.
fn add(
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
fn install(
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
fn update(
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
fn remove(package: &str, manifest_path: Option<&Path>, cwd: &Path) -> Result<NativeRunOutput, NativeError> {
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

/// Lists deterministic manifest/lock/artifact state without mutating any path.
fn list(options: &NativeOptions, cwd: &Path, toolchains: &dyn ToolchainProvider) -> Result<NativeRunOutput, NativeError> {
    let Some(project) = discover_for_native(cwd, options.manifest_path.as_deref(), false)? else {
        return Ok(success("no native dependencies (no elephc.toml discovered)\n".to_string()));
    };
    let cache = project_cache(
        cwd,
        &project,
        &format!(
            "elephc native install --locked --target {}",
            selected_target(options).as_str()
        ),
    )?;
    let rows = doctor::inspect(&project, selected_target(options), &cache, toolchains)?;
    if rows.is_empty() {
        return Ok(success("no native dependencies declared\n".to_string()));
    }
    let mut output = String::new();
    let mut healthy = true;
    let mut lock_repair = false;
    for (name, manifest_version, locked_version, abi, health) in rows {
        healthy &= health == PackageHealth::Installed;
        lock_repair |= health == PackageHealth::Stale;
        output.push_str(&format!("{name}\t{manifest_version}\t{}\t{}\t{abi}\t{}\n", locked_version.unwrap_or_else(|| "unlocked".to_string()), selected_target(options).as_str(), health.as_str()));
    }
    if !healthy {
        let command = if lock_repair {
            format!(
                "elephc native install --target {}",
                selected_target(options).as_str()
            )
        } else {
            format!(
                "elephc native install --locked --target {}",
                selected_target(options).as_str()
            )
        };
        output.push_str(&format!("project: {}\n", project.root.display()));
        output.push_str(&format!(
            "recovery: {}\n",
            recovery_from_project(&project.root, &command)
        ));
    }
    Ok(NativeRunOutput { stdout: output, exit_code: if healthy { 0 } else { 1 } })
}

/// Reports project, cache, toolchain, package, and stale-staging health without cleanup.
fn doctor(options: &NativeOptions, cwd: &Path, toolchains: &dyn ToolchainProvider) -> Result<NativeRunOutput, NativeError> {
    let target = selected_target(options);
    let discovered = discover_for_native(cwd, options.manifest_path.as_deref(), false)?;
    let search_root = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let cache = CacheLayout::from_environment(cwd).map_err(|error| match &discovered {
        Some(project) => error
            .with_project(&project.root)
            .with_recovery(format!(
                "elephc native install --locked --target {}",
                target.as_str()
            )),
        None => error
            .with_missing_project(&search_root)
            .with_recovery("elephc native add pcre2"),
    })?;
    let stats = doctor::cache_stats(&cache);
    let Some(project) = discovered else {
        let selected_toolchain = toolchains.resolve(target);
        let cache_available = cache.root.is_dir();
        let stale = doctor::stale_staging_paths(&cache);
        let (tuple, abi) = selected_toolchain.as_ref().map(|toolchain| (toolchain.target_tuple.as_str(), toolchain.abi.as_str())).unwrap_or(("unresolved", "unresolved"));
        let mut output = format!(
            "project: missing (searched from {})\ncache: {} ({})\ncache size: {}\nstale staging summary: {} ({})\ntarget: {}\ntoolchain: {}\nabi: {}\n",
            search_root.display(),
            cache.root.display(),
            if cache_available { "available" } else { "missing" },
            doctor::approximate_size(stats.approximate_bytes),
            stats.stale_staging_count,
            doctor::approximate_size(stats.stale_staging_bytes),
            target.as_str(),
            tuple,
            abi,
        );
        for path in stale {
            output.push_str(&format!("stale staging: {path}\n"));
        }
        output.push_str(&format!(
            "recovery: {}\n",
            recovery_from_project(&search_root, "elephc native add pcre2")
        ));
        output.push_str("summary: unhealthy\n");
        return Ok(NativeRunOutput { stdout: output, exit_code: 1 });
    };
    let manifest = ManifestDocument::load(&project.manifest)?;
    let lock_consistent = NativeLock::load(&project.lock).and_then(|lock| lock.validate_current(&manifest)).is_ok();
    let selected_toolchain = toolchains.resolve(selected_target(options));
    let cache_available = cache.root.is_dir();
    let rows = doctor::inspect(&project, selected_target(options), &cache, toolchains)?;
    let stale = doctor::stale_staging_paths(&cache);
    let mut healthy = stale.is_empty() && lock_consistent && cache_available && selected_toolchain.is_ok();
    let mut artifact_repair = false;
    let (tuple, abi) = selected_toolchain.as_ref().map(|toolchain| (toolchain.target_tuple.as_str(), toolchain.abi.as_str())).unwrap_or(("unresolved", "unresolved"));
    let mut output = format!("project: {}\nmanifest: {}\nlock: {} ({})\ncache: {} ({})\ncache size: {}\nstale staging summary: {} ({})\ntarget: {}\ntoolchain: {}\nabi: {}\n", project.root.display(), project.manifest.display(), project.lock.display(), if lock_consistent { "current" } else { "missing-or-stale" }, cache.root.display(), if cache_available { "available" } else { "missing" }, doctor::approximate_size(stats.approximate_bytes), stats.stale_staging_count, doctor::approximate_size(stats.stale_staging_bytes), selected_target(options).as_str(), tuple, abi);
    for (name, manifest_version, locked_version, abi, health) in rows {
        healthy &= health == PackageHealth::Installed;
        artifact_repair |= matches!(
            health,
            PackageHealth::Missing | PackageHealth::Corrupt | PackageHealth::ToolchainError
        );
        output.push_str(&format!("package {name}: manifest={manifest_version} lock={} abi={abi} {}\n", locked_version.unwrap_or_else(|| "missing".to_string()), health.as_str()));
    }
    for path in stale {
        output.push_str(&format!("stale staging: {path}\n"));
    }
    if !healthy {
        let command = if !lock_consistent {
            format!(
                "elephc native install --target {}",
                selected_target(options).as_str()
            )
        } else if artifact_repair {
            format!(
                "elephc native install --locked --target {}",
                selected_target(options).as_str()
            )
        } else {
            "elephc native prune".to_string()
        };
        output.push_str(&format!(
            "recovery: {}\n",
            recovery_from_project(&project.root, &command)
        ));
    }
    output.push_str(if healthy { "summary: healthy\n" } else { "summary: unhealthy\n" });
    Ok(NativeRunOutput { stdout: output, exit_code: if healthy { 0 } else { 1 } })
}

/// Explicitly prunes selected-target stale fingerprints and abandoned publication siblings.
fn prune(
    requested_target: Option<Target>,
    cwd: &Path,
    toolchains: &dyn ToolchainProvider,
) -> Result<NativeRunOutput, NativeError> {
    let cache = CacheLayout::from_environment(cwd)?;
    let target = requested_target.unwrap_or_else(Target::detect_host);
    if !cache.artifacts.exists() {
        return Ok(success(format!(
            "cache: {}\nremoved stale artifacts: 0\nremoved abandoned staging: 0\nreclaimed: ~0 B\n",
            cache.root.display()
        )));
    }
    let toolchain = toolchains.resolve(target).map_err(|error| {
        error.with_default_recovery(format!(
            "elephc native prune --target {}",
            target.as_str()
        ))
    })?;
    let report = cache_prune::prune_cache(&cache, target, &toolchain)?;
    Ok(success(format!(
        "cache: {}\nremoved stale artifacts: {}\nremoved abandoned staging: {}\nreclaimed: {}\n",
        cache.root.display(),
        report.removed_artifacts,
        report.removed_staging,
        doctor::approximate_size(report.reclaimed_bytes)
    )))
}

/// Returns the explicitly selected target or the supported host target.
fn selected_target(options: &NativeOptions) -> Target {
    options.target.unwrap_or_else(Target::detect_host)
}

/// Resolves cache configuration while retaining the already discovered project in diagnostics.
fn project_cache(
    cwd: &Path,
    project: &ProjectPaths,
    recovery: &str,
) -> Result<CacheLayout, NativeError> {
    CacheLayout::from_environment(cwd).map_err(|error| {
        error
            .with_project(&project.root)
            .with_default_recovery(recovery)
    })
}

/// Discovers a project and converts an absent manifest into a command-specific hard error.
fn required_project(cwd: &Path, explicit: Option<&Path>, create: bool) -> Result<ProjectPaths, NativeError> {
    discover_for_native(cwd, explicit, create)?.ok_or_else(|| {
        let search_root = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
        NativeError::new(
            NativeErrorKind::Project,
            "no elephc.toml discovered; pass --manifest-path or initialize this directory",
        )
        .with_missing_project(search_root)
        .with_recovery("elephc native add pcre2")
    })
}

/// Atomically publishes manifest then deterministic lock after successful installation.
fn publish_project(project: &ProjectPaths, manifest: &ManifestDocument, lock: &NativeLock) -> Result<(), NativeError> {
    atomic_write(&project.manifest, manifest.render().as_bytes())?;
    atomic_write(&project.lock, lock.render()?.as_bytes())
}

/// Constructs successful captured output.
fn success(stdout: String) -> NativeRunOutput {
    NativeRunOutput { stdout, exit_code: 0 }
}

#[cfg(test)]
#[path = "orchestration_tests.rs"]
mod tests;
