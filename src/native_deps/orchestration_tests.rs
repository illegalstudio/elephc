//! Purpose:
//! Unit tests for native command orchestration and materialization seams.
//!
//! Called from:
//! - `cargo test` through `crate::native_deps::orchestration`.
//!
//! Key details:
//! - Injected downloader, recipe, and toolchain fixtures keep tests deterministic and network-free.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::codegen_support::platform::Target;

use super::{run_native_command_with, NativeRunOutput};
use super::super::cache::{ArtifactKey, CacheLayout};
use super::super::catalog::PackageVersion;
use super::super::cli::{NativeCommand, NativeOptions};
use super::super::download::Downloader;
use super::super::error::{NativeError, NativeErrorKind};
use super::super::lockfile::NativeLock;
use super::super::manifest::ManifestDocument;
use super::super::materialize::{
    assert_staging_contents, cleanup_stale_staging, materialize_package,
};
use super::super::receipt::{ArtifactReceipt, ReceiptIdentity};
use super::super::recipe::{RecipeRequest, RecipeRunner};
use super::super::toolchain::{NativeToolchain, ToolchainProvider};

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
    let absent = with_cache(&cache, || {
        run_native_command_with(
            &command,
            &root,
            &downloader,
            &PanicRecipe,
            &FailingToolchains,
        )
        .unwrap_err()
    });
    assert!(absent.to_string().contains("project:"));
    assert!(absent.to_string().contains("recovery: cd --"));
    assert!(absent
        .to_string()
        .contains("elephc native install --target"));
    assert!(!cache.exists(), "absent locked state must fail before cache mutation");
    let stale = String::from_utf8(lock).unwrap().replace("recipe = 1", "recipe = 2");
    fs::write(root.join("elephc.lock"), stale).unwrap();
    let stale = with_cache(&cache, || {
        run_native_command_with(
            &command,
            &root,
            &downloader,
            &PanicRecipe,
            &FailingToolchains,
        )
        .unwrap_err()
    });
    assert!(stale.to_string().contains("project:"));
    assert!(stale.to_string().contains("recovery: cd --"));
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
    let error = with_cache(&cache, || {
        run_native_command_with(
            &command,
            &root,
            &downloader,
            &PanicRecipe,
            &super::super::toolchain::SystemToolchains,
        )
        .unwrap_err()
    });
    assert!(error.to_string().contains("offline mode"));
    assert!(error.to_string().contains("project:"));
    assert!(error.to_string().contains("recovery: cd --"));
    assert!(error
        .to_string()
        .contains("elephc native install --locked --target"));
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
    assert!(materialize_package("fixture", version, target, true, &cache, &downloader, &FailingRecipe, &toolchain, &BTreeMap::new()).is_err());
    let key = ArtifactKey { package: "fixture", version: "1.0", recipe: 1, source_sha256: version.source.sha256, target: target.as_str(), abi: &toolchain.abi, toolchain_fingerprint: &toolchain.fingerprint };
    let final_path = cache.artifact_path(&key).unwrap();
    assert!(!final_path.exists());
    let parent = final_path.parent().unwrap();
    assert!(fs::read_dir(parent).unwrap().next().is_none());
    fs::remove_dir_all(root).unwrap();
}

/// Verifies `materialize_package` forwards the caller-supplied dependency prefixes into the
/// recipe's `RecipeRequest` unchanged, matching the contract `curl`'s recipe relies on to find its
/// already-materialized `openssl`/`zlib` prefixes without ever probing the system.
#[test]
fn materialize_package_forwards_dependency_prefixes_to_the_recipe() {
    struct AssertingRecipe { expected: PathBuf }
    impl RecipeRunner for AssertingRecipe {
        fn build(&self, request: &RecipeRequest<'_>) -> Result<(), NativeError> {
            assert_eq!(request.dependency_prefixes.get("openssl"), Some(&self.expected));
            assert_eq!(request.dependency_prefixes.len(), 1);
            fs::create_dir_all(request.staging_prefix.join("lib")).unwrap();
            fs::create_dir_all(request.staging_prefix.join("include")).unwrap();
            fs::write(request.staging_prefix.join("lib/libfixture.a"), b"archive").unwrap();
            fs::write(request.staging_prefix.join("include/fixture.h"), b"header").unwrap();
            Ok(())
        }
    }

    let (root, cache_path) = fixture("dependency-prefixes");
    let cache = CacheLayout::from_values(&root, Some(cache_path.as_os_str()), None, None).unwrap();
    let version = fixture_version(&cache);
    let target = Target::detect_host();
    let toolchain = fixture_toolchain();
    let downloader = CountingDownloader { calls: Cell::new(0) };
    let openssl_prefix = root.join("already-built-openssl");
    let mut prefixes = BTreeMap::new();
    prefixes.insert("openssl".to_string(), openssl_prefix.clone());
    let recipe = AssertingRecipe { expected: openssl_prefix };
    materialize_package("fixture", version, target, true, &cache, &downloader, &recipe, &toolchain, &prefixes).unwrap();
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
            materialize_package("fixture", version, target, true, &cache, &downloader, &recipe, &toolchain, &BTreeMap::new())
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
