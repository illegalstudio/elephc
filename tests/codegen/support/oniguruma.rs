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

/// Seeds the complete forced-mbstring project without broadening Oniguruma-only fixtures.
pub(crate) fn elephc_cli_command_with_mbstring_native(dir: &Path) -> Command {
    let fixture = fixture();
    static PCRE2_READY: OnceLock<()> = OnceLock::new();
    PCRE2_READY.get_or_init(|| {
        crate::managed_pcre2_support::populate_managed_pcre2_cache(&fixture.cache, target());
    });
    for (name, contents) in [
        ("elephc.toml", include_bytes!("../../../examples/date-json-regex/elephc.toml").as_slice()),
        ("elephc.lock", include_bytes!("../../../examples/date-json-regex/elephc.lock").as_slice()),
    ] {
        let destination = dir.join(name);
        assert!(!destination.exists(), "mbstring CLI fixture requires a new owned project");
        fs::write(destination, contents).expect("write complete mbstring native project metadata");
    }
    let mut command = elephc_cli_command(dir);
    command.env("ELEPHC_NATIVE_CACHE", &fixture.cache);
    command
}

/// Builds a native project for eval-catalog fixtures that also declares the managed GMP package.
///
/// The eval builtin-catalog probe covers every registry builtin, including `ext/gmp`, so the
/// compiler requires the project to declare the pinned native package even though the probe
/// only asks `function_exists()`. Reuse the mbstring/regex fixture project, append the GMP
/// dependency and lock stanza, and copy the globally materialized artifact into the fixture
/// cache so the compiler can resolve it without a network or install step.
pub(crate) fn elephc_cli_command_with_eval_catalog_native(dir: &Path) -> Command {
    let command = elephc_cli_command_with_mbstring_native(dir);
    let manifest = dir.join("elephc.toml");
    let mut manifest_text = std::fs::read_to_string(&manifest).expect("read eval catalog manifest");
    manifest_text.push_str("gmp = \"6.3.0\"\n");
    std::fs::write(&manifest, manifest_text).expect("write eval catalog manifest with gmp");

    let lock = dir.join("elephc.lock");
    let lock_text = std::fs::read_to_string(&lock).expect("read eval catalog lock");
    let gmp_lock = include_str!("../../../examples/gmp/elephc.lock");
    let merged_lock = merge_lock_stanzas(&lock_text, gmp_lock);
    std::fs::write(&lock, merged_lock).expect("write eval catalog lock with gmp");

    let fixture_cache = command
        .get_envs()
        .find_map(|(name, value)| {
            (name == "ELEPHC_NATIVE_CACHE").then(|| value.map(PathBuf::from))?
        })
        .expect("mbstring fixture must set ELEPHC_NATIVE_CACHE");
    let global_root = std::env::var_os("ELEPHC_NATIVE_CACHE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".cache/elephc/native"))
        });
    if let Some(global_root) = global_root {
        let source = global_root.join("artifacts/gmp");
        if source.is_dir() {
            let destination = fixture_cache.join("artifacts/gmp");
            if source != destination {
                copy_tree(&source, &destination)
                    .expect("copy managed gmp artifact into fixture cache");
            }
        }
    }
    command
}

/// Merges two generated native locks and restores the resolver's package-name order.
///
/// Appending a package stanza is not enough: the lock validation compares the package graph
/// in canonical order, so the combined lock has to list `gmp` before `oniguruma` and `pcre2`.
fn merge_lock_stanzas(base: &str, extra: &str) -> String {
    let header_end = base.find("[[package]]").expect("base lock package stanza");
    let header = &base[..header_end];
    let mut stanzas: Vec<String> = base[header_end..]
        .split("[[package]]")
        .filter(|stanza| !stanza.trim().is_empty())
        .map(|stanza| format!("[[package]]{}", stanza.trim_end()))
        .collect();
    let extra_start = extra.find("[[package]]").expect("extra lock package stanza");
    stanzas.extend(
        extra[extra_start..]
            .split("[[package]]")
            .filter(|stanza| !stanza.trim().is_empty())
            .map(|stanza| format!("[[package]]{}", stanza.trim_end())),
    );
    stanzas.sort_by_key(|stanza| {
        stanza
            .lines()
            .find_map(|line| line.strip_prefix("name = ").map(str::to_string))
            .unwrap_or_default()
    });
    let mut merged = header.to_string();
    for stanza in stanzas {
        merged.push_str(&stanza);
        merged.push_str("\n\n");
    }
    merged
}

/// Copies a directory tree recursively for the test-only managed-native cache handoff.
fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if entry.path() != target {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}
