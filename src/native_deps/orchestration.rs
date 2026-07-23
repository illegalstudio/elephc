//! Purpose:
//! Coordinates transactional native command state transitions through injected services.
//!
//! Called from:
//! - `crate::native_deps::run_native_command` and deterministic unit tests.
//!
//! Key details:
//! - Compilation never enters this module; only explicit native commands may mutate cache or project state.

use std::fs;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::codegen_support::platform::Target;

use super::archive::extract_tar_gz;
use super::cache::{publish_artifact, remove_exact_node, ArtifactKey, CacheLayout};
use super::catalog::{self, PackageVersion};
use super::cli::{NativeCommand, NativeOptions};
use super::doctor::{self, PackageHealth};
use super::download::{ensure_source, Downloader};
use super::error::{NativeError, NativeErrorKind};
use super::lockfile::NativeLock;
use super::manifest::ManifestDocument;
use super::project::{discover_for_native, ProjectPaths};
use super::receipt::{collect_outputs, ArtifactReceipt, ReceiptIdentity};
use super::recipe::{RecipeRequest, RecipeRunner};
use super::toolchain::{NativeToolchain, ToolchainProvider};
use super::util::{atomic_write, unique_sibling};

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
    let cache = CacheLayout::from_environment(cwd)?;
    let project = required_project(cwd, options.manifest_path.as_deref(), true)?;
    let _project_lock = cache.lock(&cache.project_lock_path(&project.manifest), "add")?;
    let mut manifest = if project.manifest.exists() { ManifestDocument::load(&project.manifest)? } else { ManifestDocument::new() };
    if let Some(existing) = manifest.dependencies().get(package) {
        if existing != version.version {
            return Err(NativeError::new(NativeErrorKind::Manifest, format!("native package '{package}' is already declared at {existing}; use elephc native update {package}@{}", version.version)));
        }
    }
    manifest.set_dependency(package, version.version)?;
    let lock = NativeLock::from_manifest(&manifest)?;
    materialize_manifest(&manifest, selected_target(options), options.offline, &cache, downloader, recipes, toolchains)?;
    publish_project(&project, &manifest, &lock)?;
    Ok(success(format!("added {package}@{} for {}\nproject: {}\n", version.version, selected_target(options).as_str(), project.root.display())))
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
    let cache = CacheLayout::from_environment(cwd)?;
    let project = required_project(cwd, options.manifest_path.as_deref(), false)?;
    let _project_lock = if locked { None } else { Some(cache.lock(&cache.project_lock_path(&project.manifest), "install")?) };
    let manifest = ManifestDocument::load(&project.manifest)?;
    let desired = NativeLock::from_manifest(&manifest)?;
    if locked {
        let current = NativeLock::load(&project.lock).map_err(|_| NativeError::new(NativeErrorKind::Lock, "--locked requires an existing current elephc.lock").with_path(&project.lock))?;
        current.validate_current(&manifest)?;
    }
    materialize_manifest(&manifest, selected_target(options), options.offline, &cache, downloader, recipes, toolchains)?;
    if !locked {
        atomic_write(&project.lock, desired.render()?.as_bytes())?;
    }
    Ok(success(format!("installed {} native package(s) for {}{}\n", manifest.dependencies().len(), selected_target(options).as_str(), if options.offline { " (offline)" } else { "" })))
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
    let cache = CacheLayout::from_environment(cwd)?;
    let project = required_project(cwd, options.manifest_path.as_deref(), false)?;
    let _project_lock = cache.lock(&cache.project_lock_path(&project.manifest), "update")?;
    let mut manifest = ManifestDocument::load(&project.manifest)?;
    if let Some(package) = package {
        if !manifest.dependencies().contains_key(package) {
            return Err(NativeError::new(NativeErrorKind::Manifest, format!("native package '{package}' is not declared; use elephc native add {package}")));
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
    let lock = NativeLock::from_manifest(&manifest)?;
    materialize_manifest(&manifest, selected_target(options), options.offline, &cache, downloader, recipes, toolchains)?;
    publish_project(&project, &manifest, &lock)?;
    Ok(success(format!("updated {} native package(s) for {}\n", if package.is_some() { 1 } else { manifest.dependencies().len() }, selected_target(options).as_str())))
}

/// Removes one declaration and lock entry without touching the shared artifact cache.
fn remove(package: &str, manifest_path: Option<&Path>, cwd: &Path) -> Result<NativeRunOutput, NativeError> {
    catalog::package(package)?;
    let cache = CacheLayout::from_environment(cwd)?;
    let project = required_project(cwd, manifest_path, false)?;
    let _project_lock = cache.lock(&cache.project_lock_path(&project.manifest), "remove")?;
    let mut manifest = ManifestDocument::load(&project.manifest)?;
    if !manifest.remove_dependency(package) {
        return Err(NativeError::new(NativeErrorKind::Manifest, format!("native package '{package}' is not declared")));
    }
    let lock = NativeLock::from_manifest(&manifest)?;
    publish_project(&project, &manifest, &lock)?;
    Ok(success(format!("removed {package}; shared cached artifacts were retained\n")))
}

/// Lists deterministic manifest/lock/artifact state without mutating any path.
fn list(options: &NativeOptions, cwd: &Path, toolchains: &dyn ToolchainProvider) -> Result<NativeRunOutput, NativeError> {
    let Some(project) = discover_for_native(cwd, options.manifest_path.as_deref(), false)? else {
        return Ok(success("no native dependencies (no elephc.toml discovered)\n".to_string()));
    };
    let cache = CacheLayout::from_environment(cwd)?;
    let rows = doctor::inspect(&project, selected_target(options), &cache, toolchains)?;
    if rows.is_empty() {
        return Ok(success("no native dependencies declared\n".to_string()));
    }
    let mut output = String::new();
    let mut healthy = true;
    for (name, manifest_version, locked_version, abi, health) in rows {
        healthy &= health == PackageHealth::Installed;
        output.push_str(&format!("{name}\t{manifest_version}\t{}\t{}\t{abi}\t{}\n", locked_version.unwrap_or_else(|| "unlocked".to_string()), selected_target(options).as_str(), health.as_str()));
    }
    Ok(NativeRunOutput { stdout: output, exit_code: if healthy { 0 } else { 1 } })
}

/// Reports project, cache, toolchain, package, and stale-staging health without cleanup.
fn doctor(options: &NativeOptions, cwd: &Path, toolchains: &dyn ToolchainProvider) -> Result<NativeRunOutput, NativeError> {
    let cache = CacheLayout::from_environment(cwd)?;
    let target = selected_target(options);
    let Some(project) = discover_for_native(cwd, options.manifest_path.as_deref(), false)? else {
        let selected_toolchain = toolchains.resolve(target);
        let cache_available = cache.root.is_dir();
        let stale = doctor::stale_staging_paths(&cache);
        let (tuple, abi) = selected_toolchain.as_ref().map(|toolchain| (toolchain.target_tuple.as_str(), toolchain.abi.as_str())).unwrap_or(("unresolved", "unresolved"));
        let mut output = format!(
            "project: missing\ncache: {} ({})\ntarget: {}\ntoolchain: {}\nabi: {}\n",
            cache.root.display(),
            if cache_available { "available" } else { "missing" },
            target.as_str(),
            tuple,
            abi,
        );
        for path in stale {
            output.push_str(&format!("stale staging: {path}\n"));
        }
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
    let (tuple, abi) = selected_toolchain.as_ref().map(|toolchain| (toolchain.target_tuple.as_str(), toolchain.abi.as_str())).unwrap_or(("unresolved", "unresolved"));
    let mut output = format!("project: {}\nmanifest: {}\nlock: {} ({})\ncache: {} ({})\ntarget: {}\ntoolchain: {}\nabi: {}\n", project.root.display(), project.manifest.display(), project.lock.display(), if lock_consistent { "current" } else { "missing-or-stale" }, cache.root.display(), if cache_available { "available" } else { "missing" }, selected_target(options).as_str(), tuple, abi);
    for (name, manifest_version, locked_version, abi, health) in rows {
        healthy &= health == PackageHealth::Installed;
        output.push_str(&format!("package {name}: manifest={manifest_version} lock={} abi={abi} {}\n", locked_version.unwrap_or_else(|| "missing".to_string()), health.as_str()));
    }
    for path in stale {
        output.push_str(&format!("stale staging: {path}\n"));
    }
    output.push_str(if healthy { "summary: healthy\n" } else { "summary: unhealthy\n" });
    Ok(NativeRunOutput { stdout: output, exit_code: if healthy { 0 } else { 1 } })
}

/// Returns the explicitly selected target or the supported host target.
fn selected_target(options: &NativeOptions) -> Target {
    options.target.unwrap_or_else(Target::detect_host)
}

/// Discovers a project and converts an absent manifest into a command-specific hard error.
fn required_project(cwd: &Path, explicit: Option<&Path>, create: bool) -> Result<ProjectPaths, NativeError> {
    discover_for_native(cwd, explicit, create)?.ok_or_else(|| NativeError::new(NativeErrorKind::Project, "no elephc.toml discovered; run elephc native add pcre2 or pass --manifest-path"))
}

/// Materializes every declared package for one target after toolchain preflight.
fn materialize_manifest(
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
    for (name, selected) in manifest.dependencies() {
        let version = catalog::version(name, Some(selected))?;
        catalog::ensure_target(version, target)?;
        materialize_package(name, version, target, offline, cache, downloader, recipes, &toolchain)?;
    }
    Ok(())
}

/// Reuses or transactionally builds one exact artifact under its advisory lock.
fn materialize_package(
    package: &str,
    version: &'static PackageVersion,
    target: Target,
    offline: bool,
    cache: &CacheLayout,
    downloader: &dyn Downloader,
    recipes: &dyn RecipeRunner,
    toolchain: &NativeToolchain,
) -> Result<PathBuf, NativeError> {
    let key = ArtifactKey { package, version: version.version, recipe: version.recipe_revision, source_sha256: version.source.sha256, target: target.as_str(), abi: &toolchain.abi, toolchain_fingerprint: &toolchain.fingerprint };
    let final_path = cache.artifact_path(&key)?;
    let _artifact_lock = cache.lock(&cache.artifact_lock_path(&key)?, "install-artifact")?;
    cleanup_stale_staging(&final_path)?;
    let retained = version.retained_headers.iter().chain(version.ordered_link_outputs.iter()).copied().collect::<Vec<_>>();
    let identity = ReceiptIdentity { package, version: version.version, recipe: version.recipe_revision, source_sha256: version.source.sha256, target: target.as_str(), abi: &toolchain.abi, toolchain_fingerprint: &toolchain.fingerprint, required_outputs: &retained };
    let existing_valid = ArtifactReceipt::load(&final_path).and_then(|receipt| receipt.verify(&final_path, &identity)).is_ok();
    if existing_valid { return Ok(final_path); }

    let source_path = cache.source_path(version.source.sha256);
    {
        let _source_lock = cache.lock(&cache.source_lock_path(version.source.sha256), "download-source")?;
        ensure_source(&source_path, &version.source, offline, downloader)?;
    }
    let parent = final_path.parent().ok_or_else(|| NativeError::new(NativeErrorKind::Cache, "artifact path has no parent"))?;
    fs::create_dir_all(parent).map_err(|error| NativeError::io("create artifact parent", parent, error))?;
    let staging = unique_sibling(&final_path, "stage");
    fs::create_dir(&staging).map_err(|error| NativeError::io("create artifact staging", &staging, error))?;
    let result = (|| {
        let extracted = staging.join(".source");
        extract_tar_gz(&source_path, &extracted)?;
        recipes.build(&RecipeRequest { package, version, target, source: &extracted, staging_prefix: &staging, toolchain })?;
        fs::remove_dir_all(&extracted)
            .map_err(|error| NativeError::io("remove extracted native source tree", &extracted, error))?;
        assert_staging_contents(&staging, &retained)?;
        let receipt = ArtifactReceipt {
            schema: 1, package: package.to_string(), version: version.version.to_string(), recipe: version.recipe_revision,
            source_sha256: version.source.sha256.to_string(), target: target.as_str().to_string(), abi: toolchain.abi.clone(),
            compiler: toolchain.compiler.clone(), archiver: toolchain.archiver.clone(), ranlib: toolchain.ranlib_identity.clone(),
            toolchain_fingerprint: toolchain.fingerprint.clone(), outputs: collect_outputs(&staging, &retained)?, created_by: env!("CARGO_PKG_VERSION").to_string(),
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

/// Ensures the final staging tree contains only catalog-retained regular files and their directories.
fn assert_staging_contents(staging: &Path, retained: &[&str]) -> Result<(), NativeError> {
    let expected = retained.iter().map(|path| (*path).to_string()).collect::<BTreeSet<_>>();
    let expected_directories = expected_directories(&expected);
    let mut actual = BTreeSet::new();
    let mut actual_directories = BTreeSet::new();
    collect_staging_files(staging, staging, &mut actual, &mut actual_directories)?;
    if actual != expected || actual_directories != expected_directories {
        return Err(NativeError::new(
            NativeErrorKind::Integrity,
            format!("trusted recipe staging is not exact: expected files {expected:?} and directories {expected_directories:?}, got files {actual:?} and directories {actual_directories:?}"),
        ).with_path(staging));
    }
    Ok(())
}

/// Recursively collects only non-symlink regular files beneath an exact staging root.
fn collect_staging_files(root: &Path, directory: &Path, output: &mut BTreeSet<String>, directories: &mut BTreeSet<String>) -> Result<(), NativeError> {
    for entry in fs::read_dir(directory).map_err(|error| NativeError::io("inspect recipe staging", directory, error))? {
        let entry = entry.map_err(|error| NativeError::io("read recipe staging entry", directory, error))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|error| NativeError::io("inspect recipe staging entry type", &path, error))?;
        if kind.is_symlink() {
            return Err(NativeError::new(NativeErrorKind::Integrity, "trusted recipe staging contains a symlink").with_path(path));
        }
        if kind.is_dir() {
            let relative = path.strip_prefix(root).expect("staging directory below root");
            directories.insert(relative.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"));
            collect_staging_files(root, &path, output, directories)?;
        } else if kind.is_file() {
            let relative = path.strip_prefix(root).expect("staging child rooted below staging");
            output.insert(relative.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"));
        } else {
            return Err(NativeError::new(NativeErrorKind::Integrity, "trusted recipe staging contains a special file").with_path(path));
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
            directories.insert(path.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/"));
            parent = path.parent();
        }
    }
    directories
}

/// Removes only exact-key staging siblings older than 24 hours while holding the artifact lock.
fn cleanup_stale_staging(final_path: &Path) -> Result<(), NativeError> {
    let Some(parent) = final_path.parent() else { return Ok(()); };
    let Some(name) = final_path.file_name().and_then(|name| name.to_str()) else { return Ok(()); };
    let prefix = format!(".{name}.stage.");
    let Ok(entries) = fs::read_dir(parent) else { return Ok(()); };
    for entry in entries.flatten() {
        let entry_name = entry.file_name().to_string_lossy().into_owned();
        if !entry_name.starts_with(&prefix) { continue; }
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| NativeError::io("inspect stale artifact staging", &entry.path(), error))?;
        let old = metadata.modified().ok().and_then(|modified| SystemTime::now().duration_since(modified).ok()).is_some_and(|age| age >= Duration::from_secs(24 * 60 * 60));
        if old {
            remove_exact_node(&entry.path())?;
        }
    }
    Ok(())
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
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::Mutex;
    use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
    use std::time::{SystemTime, UNIX_EPOCH};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Downloader fake that records calls and fails if actual transport is requested.
    struct CountingDownloader { calls: Cell<usize> }

    impl Downloader for CountingDownloader {
        /// Records the forbidden call and returns a deterministic test error.
        fn download_to(&self, _source: &super::super::catalog::SourceArchive, _destination: &Path) -> Result<(), NativeError> {
            self.calls.set(self.calls.get() + 1);
            Err(NativeError::new(NativeErrorKind::Network, "test downloader called"))
        }
    }

    /// Recipe fake that must never run in preflight/offline failure tests.
    struct PanicRecipe;

    impl RecipeRunner for PanicRecipe {
        /// Fails immediately if orchestration reaches recipe execution unexpectedly.
        fn build(&self, _request: &RecipeRequest<'_>) -> Result<(), NativeError> { panic!("recipe should not run") }
    }

    /// Recipe fake that returns a controlled build failure after extraction.
    struct FailingRecipe;

    impl RecipeRunner for FailingRecipe {
        /// Simulates a trusted recipe process failure without producing outputs.
        fn build(&self, _request: &RecipeRequest<'_>) -> Result<(), NativeError> {
            Err(NativeError::new(NativeErrorKind::Build, "injected recipe failure"))
        }
    }

    /// Recipe fake that writes exactly the fixture catalog outputs and counts builds.
    #[derive(Clone)]
    struct WritingRecipe { calls: Arc<AtomicUsize> }

    impl RecipeRunner for WritingRecipe {
        /// Produces the fixture header/archive output set without invoking external tools.
        fn build(&self, request: &RecipeRequest<'_>) -> Result<(), NativeError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            fs::create_dir_all(request.staging_prefix.join("lib")).map_err(|error| NativeError::io("create fake recipe lib", request.staging_prefix, error))?;
            fs::create_dir_all(request.staging_prefix.join("include")).map_err(|error| NativeError::io("create fake recipe include", request.staging_prefix, error))?;
            fs::write(request.staging_prefix.join("lib/libfixture.a"), b"archive").map_err(|error| NativeError::io("write fake recipe archive", request.staging_prefix, error))?;
            fs::write(request.staging_prefix.join("include/fixture.h"), b"header").map_err(|error| NativeError::io("write fake recipe header", request.staging_prefix, error))
        }
    }

    /// Toolchain provider fake used to force failure before download or publication.
    struct FailingToolchains;

    impl ToolchainProvider for FailingToolchains {
        /// Returns a deterministic preflight failure.
        fn resolve(&self, _target: Target) -> Result<NativeToolchain, NativeError> {
            Err(NativeError::new(NativeErrorKind::Toolchain, "injected toolchain failure"))
        }
    }

    /// Creates an isolated native project with a durable cache sibling.
    fn fixture(label: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("elephc-orchestration-{label}-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let cache = root.join("cache");
        fs::create_dir_all(&root).unwrap();
        (root, cache)
    }

    /// Runs a closure with one serialized `ELEPHC_NATIVE_CACHE` value and restores prior state.
    fn with_cache<T>(cache: &Path, action: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous = std::env::var_os("ELEPHC_NATIVE_CACHE");
        std::env::set_var("ELEPHC_NATIVE_CACHE", cache);
        let result = action();
        if let Some(previous) = previous { std::env::set_var("ELEPHC_NATIVE_CACHE", previous); } else { std::env::remove_var("ELEPHC_NATIVE_CACHE"); }
        result
    }

    /// Writes a canonical project manifest and returns its generated lock bytes.
    fn write_project(root: &Path) -> Vec<u8> {
        let manifest = ManifestDocument::parse("# keep\n[native]\nschema = 1\n[native.dependencies]\npcre2 = \"10.47\"\n").unwrap();
        fs::write(root.join("elephc.toml"), manifest.render()).unwrap();
        let lock = NativeLock::from_manifest(&manifest).unwrap().render().unwrap().into_bytes();
        fs::write(root.join("elephc.lock"), &lock).unwrap();
        lock
    }

    /// Creates a tiny safe tar.gz and leaked immutable catalog version for cache state-machine tests.
    fn fixture_version(cache: &CacheLayout) -> &'static PackageVersion {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use sha2::{Digest, Sha256};
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let bytes = b"source";
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, "fixture/source.txt", &bytes[..]).unwrap();
        let compressed = builder.into_inner().unwrap().finish().unwrap();
        let sha256 = format!("{:x}", Sha256::digest(&compressed));
        let sha256: &'static str = Box::leak(sha256.into_boxed_str());
        fs::create_dir_all(&cache.sources).unwrap();
        fs::write(cache.source_path(sha256), &compressed).unwrap();
        let target: &'static str = Target::detect_host().as_str();
        Box::leak(Box::new(PackageVersion {
            version: "1.0",
            source: super::super::catalog::SourceArchive { https_url: "https://example.invalid/fixture.tar.gz", sha256, exact_size: compressed.len() as u64, body_limit: 1024 * 1024 },
            recipe_revision: 1,
            dependencies: &[], supported_targets: Box::leak(vec![target].into_boxed_slice()),
            ordered_link_outputs: &["lib/libfixture.a"], retained_headers: &["include/fixture.h"], provides: &["fixture"],
        }))
    }

    /// Creates a deterministic toolchain identity for fake recipe materialization.
    fn fixture_toolchain() -> NativeToolchain {
        use super::super::receipt::ToolIdentity;
        NativeToolchain {
            cc: "cc".into(), ar: "ar".into(), ranlib: "ranlib".into(), target_tuple: "fixture-tuple".into(), abi: "fixture-abi".into(), fingerprint: "fixture-fingerprint".into(),
            compiler: ToolIdentity { command: "cc".into(), version: "fixture".into() }, archiver: ToolIdentity { command: "ar".into(), version: "fixture".into() }, ranlib_identity: ToolIdentity { command: "ranlib".into(), version: "fixture".into() },
        }
    }

    /// Verifies output status can represent read-only unhealthy diagnostics without process exit.
    #[test]
    fn run_output_carries_exit_status_without_exiting() {
        let output = NativeRunOutput { stdout: "diagnostic\n".into(), exit_code: 1 };
        assert_eq!(output.exit_code, 1);
        assert_eq!(output.stdout, "diagnostic\n");
    }

    /// Verifies build intermediates cannot be published beside catalog outputs.
    #[test]
    fn final_staging_rejects_unexpected_files() {
        let root = std::env::temp_dir().join(format!("elephc-staging-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::write(root.join("lib/a.a"), b"a").unwrap();
        assert_staging_contents(&root, &["lib/a.a"]).unwrap();
        fs::create_dir(root.join("empty-build")).unwrap();
        assert!(assert_staging_contents(&root, &["lib/a.a"]).is_err());
        fs::remove_dir(root.join("empty-build")).unwrap();
        fs::write(root.join("build.log"), b"unexpected").unwrap();
        assert!(assert_staging_contents(&root, &["lib/a.a"]).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies a fresh broken-symlink staging sibling cannot block installation cleanup.
    #[test]
    #[cfg(unix)]
    fn staging_cleanup_inspects_broken_symlinks_without_following() {
        let root = fixture("staging-symlink").0;
        let final_path = root.join("artifact");
        let staging = root.join(".artifact.stage.broken");
        std::os::unix::fs::symlink(root.join("missing"), &staging).unwrap();
        cleanup_stale_staging(&final_path).unwrap();
        assert!(fs::symlink_metadata(&staging).unwrap().file_type().is_symlink());
        fs::remove_file(staging).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies failed add and update leave both project files byte-identical.
    #[test]
    fn failed_mutations_do_not_publish_project_state() {
        let (root, cache) = fixture("transaction");
        let original_lock = write_project(&root);
        let original_manifest = fs::read(root.join("elephc.toml")).unwrap();
        let downloader = CountingDownloader { calls: Cell::new(0) };
        with_cache(&cache, || {
            let options = NativeOptions { target: Some(Target::detect_host()), manifest_path: Some(root.join("elephc.toml")), offline: false };
            let add = NativeCommand::Add { package: "pcre2".into(), version: Some("10.47".into()), options: options.clone() };
            assert!(run_native_command_with(&add, &root, &downloader, &PanicRecipe, &FailingToolchains).is_err());
            let update = NativeCommand::Update { package: Some("pcre2".into()), version: Some("10.47".into()), options };
            assert!(run_native_command_with(&update, &root, &downloader, &PanicRecipe, &FailingToolchains).is_err());
        });
        assert_eq!(fs::read(root.join("elephc.toml")).unwrap(), original_manifest);
        assert_eq!(fs::read(root.join("elephc.lock")).unwrap(), original_lock);
        assert_eq!(downloader.calls.get(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies remove creates only its mandatory project-lock path and no source or artifact cache.
    #[test]
    fn remove_does_not_initialize_native_artifact_cache() {
        let (root, cache) = fixture("remove-cache");
        write_project(&root);
        let command = NativeCommand::Remove {
            package: "pcre2".into(),
            manifest_path: Some(root.join("elephc.toml")),
        };
        let downloader = CountingDownloader { calls: Cell::new(0) };

        let output = with_cache(&cache, || {
            run_native_command_with(
                &command,
                &root,
                &downloader,
                &PanicRecipe,
                &FailingToolchains,
            )
            .unwrap()
        });

        assert_eq!(output.exit_code, 0);
        assert!(!cache.join("sources").exists());
        assert!(!cache.join("artifacts").exists());
        assert!(cache.join("locks/project").is_dir());
        assert_eq!(downloader.calls.get(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies locked install rejects absent and stale locks before tool or network access.
    #[test]
    fn locked_install_rejects_absent_and_stale_lock() {
        let (root, cache) = fixture("locked");
        let lock = write_project(&root);
        fs::remove_file(root.join("elephc.lock")).unwrap();
        let downloader = CountingDownloader { calls: Cell::new(0) };
        let command = NativeCommand::Install { locked: true, options: NativeOptions { target: Some(Target::detect_host()), manifest_path: Some(root.join("elephc.toml")), offline: true } };
        with_cache(&cache, || assert!(run_native_command_with(&command, &root, &downloader, &PanicRecipe, &FailingToolchains).is_err()));
        assert!(!cache.exists(), "absent locked state must fail before cache mutation");
        let stale = String::from_utf8(lock).unwrap().replace("recipe = 1", "recipe = 2");
        fs::write(root.join("elephc.lock"), stale).unwrap();
        with_cache(&cache, || assert!(run_native_command_with(&command, &root, &downloader, &PanicRecipe, &FailingToolchains).is_err()));
        assert!(!cache.exists(), "stale locked state must fail before cache mutation");
        assert_eq!(downloader.calls.get(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies offline install misses fail without ever invoking injected transport.
    #[test]
    fn offline_install_never_invokes_downloader() {
        let (root, cache) = fixture("offline");
        write_project(&root);
        let downloader = CountingDownloader { calls: Cell::new(0) };
        let command = NativeCommand::Install { locked: true, options: NativeOptions { target: Some(Target::detect_host()), manifest_path: Some(root.join("elephc.toml")), offline: true } };
        with_cache(&cache, || assert!(run_native_command_with(&command, &root, &downloader, &PanicRecipe, &super::super::toolchain::SystemToolchains).is_err()));
        assert_eq!(downloader.calls.get(), 0);
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies doctor reports global lock/toolchain health even when no package rows exist.
    #[test]
    fn doctor_empty_manifest_with_missing_lock_is_unhealthy() {
        let (root, cache) = fixture("doctor-empty");
        fs::write(root.join("elephc.toml"), "[native]\nschema = 1\n[native.dependencies]\n").unwrap();
        let command = NativeCommand::Doctor { options: NativeOptions { target: Some(Target::detect_host()), manifest_path: Some(root.join("elephc.toml")), offline: false } };
        let downloader = CountingDownloader { calls: Cell::new(0) };
        let output = with_cache(&cache, || run_native_command_with(&command, &root, &downloader, &PanicRecipe, &FailingToolchains).unwrap());
        assert_eq!(output.exit_code, 1);
        assert!(output.stdout.contains("lock:") && output.stdout.contains("missing-or-stale"));
        assert!(output.stdout.contains("toolchain: unresolved"));
        assert!(output.stdout.contains("abi: unresolved"));
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies doctor without a project still inspects global cache, target, toolchain, and staging state read-only.
    #[test]
    fn doctor_without_project_reports_global_health_read_only() {
        let (root, cache) = fixture("doctor-missing-project");
        let stale = cache.join("artifacts/.fixture.stage.123");
        fs::create_dir_all(&stale).unwrap();
        let command = NativeCommand::Doctor { options: NativeOptions { target: Some(Target::detect_host()), manifest_path: None, offline: false } };
        let downloader = CountingDownloader { calls: Cell::new(0) };
        let output = with_cache(&cache, || run_native_command_with(&command, &root, &downloader, &PanicRecipe, &FailingToolchains).unwrap());
        assert_eq!(output.exit_code, 1);
        assert!(output.stdout.contains("project: missing"));
        assert!(output.stdout.contains("cache:") && output.stdout.contains("available"));
        assert!(output.stdout.contains(Target::detect_host().as_str()));
        assert!(output.stdout.contains("toolchain: unresolved"));
        assert!(output.stdout.contains("abi: unresolved"));
        assert!(output.stdout.contains("stale staging:"));
        assert!(stale.is_dir());
        assert!(!cache.join("locks").exists());
        assert!(!cache.join("sources").exists());
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies a failed recipe leaves no final artifact and removes its unique staging sibling.
    #[test]
    fn failed_recipe_leaves_no_resolvable_artifact_or_staging() {
        let (root, cache_path) = fixture("failed-recipe");
        let cache = CacheLayout::from_values(&root, Some(cache_path.as_os_str()), None, None).unwrap();
        let version = fixture_version(&cache);
        let target = Target::detect_host();
        let toolchain = fixture_toolchain();
        let downloader = CountingDownloader { calls: Cell::new(0) };
        assert!(materialize_package("fixture", version, target, true, &cache, &downloader, &FailingRecipe, &toolchain).is_err());
        let key = ArtifactKey { package: "fixture", version: "1.0", recipe: 1, source_sha256: version.source.sha256, target: target.as_str(), abi: &toolchain.abi, toolchain_fingerprint: &toolchain.fingerprint };
        let final_path = cache.artifact_path(&key).unwrap();
        assert!(!final_path.exists());
        let parent = final_path.parent().unwrap();
        assert!(fs::read_dir(parent).unwrap().next().is_none());
        fs::remove_dir_all(root).unwrap();
    }

    /// Verifies concurrent exact-key installers serialize so one recipe publishes and the other reuses.
    #[test]
    fn concurrent_materialization_builds_once_and_reuses_verified_winner() {
        let (root, cache_path) = fixture("concurrent");
        let cache = CacheLayout::from_values(&root, Some(cache_path.as_os_str()), None, None).unwrap();
        let version = fixture_version(&cache);
        let target = Target::detect_host();
        let toolchain = fixture_toolchain();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let cache = cache.clone();
            let toolchain = toolchain.clone();
            let recipe = WritingRecipe { calls: calls.clone() };
            handles.push(std::thread::spawn(move || {
                let downloader = CountingDownloader { calls: Cell::new(0) };
                materialize_package("fixture", version, target, true, &cache, &downloader, &recipe, &toolchain)
            }));
        }
        let first = handles.remove(0).join().unwrap().unwrap();
        let second = handles.remove(0).join().unwrap().unwrap();
        assert_eq!(first, second);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let receipt = ArtifactReceipt::load(&first).unwrap();
        let required = ["include/fixture.h", "lib/libfixture.a"];
        let identity = ReceiptIdentity { package: "fixture", version: "1.0", recipe: 1, source_sha256: version.source.sha256, target: target.as_str(), abi: &toolchain.abi, toolchain_fingerprint: &toolchain.fingerprint, required_outputs: &required };
        receipt.verify(&first, &identity).unwrap();
        fs::remove_dir_all(root).unwrap();
    }
}
