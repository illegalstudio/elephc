//! Purpose:
//! Supplies exact managed Oniguruma archives to codegen fixtures and isolated CLI projects.
//!
//! Called from:
//! - Native test link planning and explicit mbregex CLI integration tests.
//!
//! Key details:
//! - The production native command owns download, verification, target selection, and immutable caching.
//! - No system-library fallback or fabricated package receipt is used for mbregex.
//! - Archived shards install offline from the bundled verified source when their toolchain identity differs.

use super::*;
use std::path::PathBuf;

/// Retains one target's verified package and the manifest/lock that selected it.
struct Fixture { target: Target, project: PathBuf, cache: PathBuf, archives: Vec<PathBuf> }

/// Installs or reuses the curated package once per test process and resolves its exact ordered archives.
fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    let fixture = FIXTURE.get_or_init(|| {
        let target = target();
        if prebuilt_bridge_staticlibs_are_trusted() { return archived_fixture(target); }
        let project = std::env::temp_dir().join(format!("elephc_oniguruma_test_{}_{}", std::process::id(), target.as_str()));
        fs::create_dir_all(&project).expect("create isolated Oniguruma project");
        let manifest = project.join("elephc.toml");
        let output = Command::new(elephc_cli_bin()).args(["native", "add", "oniguruma", "--target", target.as_str(), "--manifest-path"])
            .arg(&manifest).output().expect("run managed Oniguruma test installation");
        assert!(output.status.success(), "managed Oniguruma test installation failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let packages = elephc::native_deps::resolve_for_compilation(&project.join("main.php"), target,
            &[elephc::native_deps::NativeRequirement::package("oniguruma")]).expect("resolve verified Oniguruma archives");
        assert_eq!(packages.len(), 1);
        let package = packages.into_iter().next().unwrap();
        assert!(package.system_libraries.is_empty() && package.frameworks.is_empty());
        let cache = package.artifact_root.ancestors().find(|path| path.file_name().is_some_and(|name| name == "artifacts"))
            .and_then(Path::parent).expect("managed artifact cache layout").to_path_buf();
        Fixture { target, project, cache, archives: package.archives }
    });
    assert_eq!(fixture.target, target(), "Oniguruma fixtures cannot mix targets within one test process");
    fixture
}

/// Revalidates a packaged native cache and rebuilds offline for the shard's actual toolchain if needed.
fn archived_fixture(target: Target) -> Fixture {
    let compiler = PathBuf::from(elephc_cli_bin());
    let bundle = compiler.parent().expect("compiler directory").join("elephc-oniguruma");
    let project = bundle.join("project");
    let cache = bundle.join("cache");
    let manifest = project.join("elephc.toml");
    assert!(manifest.is_file() && project.join("elephc.lock").is_file() && cache.join("sources").is_dir(),
        "archived mbregex tests require the Oniguruma bundle at {}; run scripts/ci/prepare_oniguruma_tests.py before archiving", bundle.display());
    let output = Command::new(&compiler).args(["native", "install", "--locked", "--offline", "--target", target.as_str(), "--manifest-path"])
        .arg(&manifest).env("ELEPHC_NATIVE_CACHE", &cache).output().expect("install bundled Oniguruma offline");
    assert!(output.status.success(), "offline managed Oniguruma installation failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    let packages = elephc::native_deps::resolve_for_compilation_in_cache(&project.join("main.php"), target,
        &[elephc::native_deps::NativeRequirement::package("oniguruma")], &cache).expect("verify bundled Oniguruma archives");
    assert_eq!(packages.len(), 1);
    let package = packages.into_iter().next().unwrap();
    assert!(package.system_libraries.is_empty() && package.frameworks.is_empty());
    Fixture { target, project, cache, archives: package.archives }
}

/// Borrows the provider shim followed by libonig in the production catalog's link order.
pub(crate) fn test_oniguruma_archives() -> &'static [PathBuf] { &fixture().archives }

/// Seeds a new CLI project with the reviewed Oniguruma lock while keeping its runtime cache isolated.
pub(crate) fn elephc_cli_command_with_oniguruma(dir: &Path) -> Command {
    let fixture = fixture();
    for file in ["elephc.toml", "elephc.lock"] {
        let destination = dir.join(file);
        assert!(!destination.exists(), "Oniguruma CLI fixture requires a new owned project");
        fs::copy(fixture.project.join(file), destination).expect("copy verified Oniguruma project metadata");
    }
    let mut command = elephc_cli_command(dir);
    command.env("ELEPHC_NATIVE_CACHE", &fixture.cache);
    command
}
