//! Purpose:
//! Guards the shape every PHP polyfill ships: an eager `autoload.files` entry that declares global
//! functions behind `if (!function_exists('f'))`. Both shapes work: the declarations in the eager
//! file itself (one hop) and the ones in a file it includes (two hops).
//!
//! Called from:
//! - `cargo test --test eager_file_polyfill_tests` through Rust's test harness.
//!
//! Key details:
//! - Conditional declarations in an INCLUDED file are bound
//!   (`resolver::declarations::conditional_include_declarations_are_bound`); they used to be
//!   dropped behind a gate, which is what the two-hop test pins. An eager file's OWN conditional
//!   declarations were never on that path.
//! - `function_exists` cannot be used to observe any of this: it answers false for
//!   interpreter-declared functions even when calling them works. These tests CALL the function.
//! - Symfony's `/` route dies on exactly this shape --
//!   `polyfill-mbstring/bootstrap.php` -> `return require __DIR__.'/bootstrap80.php'` -> guarded
//!   `mb_*` declarations -- traced as `phase=compiler_included_skip` on the first file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut exe = std::env::current_exe().expect("failed to resolve current test binary");
        exe.pop();
        if exe.ends_with("deps") {
            exe.pop();
        }
        exe.join("elephc").to_string_lossy().into_owned()
    })
}

/// Writes a multi-file project into `dir`, creating parent directories for nested paths.
fn write_project(dir: &Path, files: &[(&str, &str)]) {
    for (relative, contents) in files {
        let target = dir.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&target, contents).unwrap();
    }
}

/// Compiles a multi-file project and runs the resulting executable, returning its stdout.
fn compile_and_run_project(prefix: &str, files: &[(&str, &str)], entry: &str) -> String {
    let dir = make_test_dir(prefix);
    write_project(&dir, files);
    let compiled = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg(entry)
        .output()
        .expect("failed to spawn elephc");
    assert!(
        compiled.status.success(),
        "elephc {entry} failed:\n{}",
        String::from_utf8_lossy(&compiled.stderr)
    );

    let stem = entry.strip_suffix(".php").expect("entry must end in .php");
    let output = Command::new(dir.join(stem))
        .output()
        .expect("failed to run compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "compiled binary exited non-zero:\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

/// A manifest whose only autoload rule is one eager `files` entry.
const MANIFEST: &str = r#"{"autoload":{"files":["boot/bootstrap.php"]}}"#;

/// Removes each regression fixture even when compilation or an assertion fails.
struct EagerVariantFixture(PathBuf);

impl Drop for EagerVariantFixture {
    /// Cleans only the temporary directory created for this fixture.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs the same physical eager/include project under PHP and the real compiler CLI.
fn assert_eager_variant_reuse(bootstrap: &str, entry: &str, preload: bool) {
    let fixture = EagerVariantFixture(make_test_dir("eager_variant_reuse"));
    let dir = &fixture.0;
    write_project(dir, &[
        ("composer.json", MANIFEST),
        ("boot/bootstrap.php", bootstrap),
        ("main.php", entry),
        ("preload.php", "<?php require_once __DIR__.'/boot/bootstrap.php';"),
    ]);
    let php = Command::new("php").arg("-n").arg("main.php").current_dir(dir)
        .output().expect("run PHP reference");
    assert!(php.status.success(), "{}", String::from_utf8_lossy(&php.stderr));
    let mut command = Command::new(elephc_bin());
    command.current_dir(dir);
    if preload {
        command.args(["--ini", "opcache.enable_cli=1", "--ini"])
            .arg(format!("opcache.preload={}", dir.join("preload.php").display()));
    }
    let compiled = command.arg("main.php").output().expect("compile eager/include fixture");
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let native = Command::new(dir.join("main")).current_dir(dir)
        .output().expect("run eager/include fixture");
    assert!(native.status.success(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(native.stdout, php.stdout);
}

/// One physical function is reused and eager effects run once before the entry's once include.
#[test]
fn eager_file_reuses_include_variant_before_file_effects() {
    assert_eager_variant_reuse(
        "<?php echo eager_reuse_helper(), ':boot|'; function eager_reuse_helper(): string { return 'ok'; }",
        "<?php require_once __DIR__.'/boot/bootstrap.php'; echo eager_reuse_helper(), ':entry|';",
        false,
    );
}

/// A file with no declarations still executes its eager effects only once.
#[test]
fn eager_file_reuses_include_variant_guard_for_effects_only_file() {
    assert_eager_variant_reuse(
        "<?php echo 'boot|';",
        "<?php require_once __DIR__.'/boot/bootstrap.php'; echo 'entry|';",
        false,
    );
}

/// Reuse follows PHP's canonical names for namespaced functions too.
#[test]
fn eager_file_reuses_include_variant_in_namespace() {
    assert_eager_variant_reuse(
        "<?php namespace EagerReuse; function helper(): string { return 'namespace'; } echo helper(), ':boot|';",
        "<?php require_once __DIR__.'/boot/bootstrap.php'; echo \\EagerReuse\\helper(), ':entry|';",
        false,
    );
}

/// A preload reaching the eager file shares its variant and does not duplicate its effects.
#[test]
fn eager_file_reuses_include_variant_from_preload() {
    assert_eager_variant_reuse(
        "<?php function eager_reuse_preload(): string { return 'preload'; } echo eager_reuse_preload(), ':boot|';",
        "<?php require_once __DIR__.'/preload.php'; echo eager_reuse_preload(), ':entry|';",
        true,
    );
}

/// Matching names in distinct physical files must still be rejected as duplicate declarations.
#[test]
fn eager_file_reuses_include_variant_preserves_distinct_file_error() {
    let fixture = EagerVariantFixture(make_test_dir("eager_variant_collision"));
    write_project(&fixture.0, &[
        ("composer.json", MANIFEST),
        ("boot/bootstrap.php", "<?php function eager_reuse_collision(): int { return 1; }"),
        ("other.php", "<?php function eager_reuse_collision(): int { return 2; }"),
        ("main.php", "<?php require_once __DIR__.'/other.php'; echo eager_reuse_collision();"),
    ]);
    let compiled = Command::new(elephc_bin()).current_dir(&fixture.0)
        .args(["--check", "main.php"]).output().expect("check distinct declarations");
    assert!(!compiled.status.success());
    assert!(String::from_utf8_lossy(&compiled.stderr).contains("Duplicate function declaration"));
}

/// The declarations themselves, each behind the guard every polyfill uses.
const GUARDED_DECLARATIONS: &str = r#"<?php

if (!function_exists('probe_upper')) {
    function probe_upper(?string $string): string { return strtoupper((string) $string); }
}
if (!function_exists('probe_width')) {
    function probe_width(?string $string): int { return strlen((string) $string); }
}
"#;

/// The polyfill entry point: a version guard delegating to the file that holds the declarations.
/// `symfony/polyfill-mbstring/bootstrap.php` is this file, line for line.
const DELEGATING_BOOTSTRAP: &str = r#"<?php

if (\PHP_VERSION_ID >= 80000) {
    return require __DIR__ . '/bootstrap80.php';
}

return require __DIR__ . '/bootstrap72.php';
"#;

/// Tests that an eager file's OWN guarded global functions are declared.
///
/// One hop, and it works: the file is spliced whole and its conditional declarations are not on
/// the include-stripping path. This is the green anchor for the two-hop test
/// below — without it, a failure there could just as well mean eager files do nothing at all.
#[test]
fn an_eager_file_declares_its_own_guarded_global_functions() {
    let out = compile_and_run_project(
        "eager_polyfill_1hop",
        &[
            ("module.json", MANIFEST),
            ("boot/bootstrap.php", GUARDED_DECLARATIONS),
            (
                "main.php",
                r#"<?php
echo probe_upper('ab'), ':', probe_width('abc'), "\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "AB:3\n");
}

/// Tests that a guarded global function reached through an eager file's INCLUDE is declared.
///
/// The second hop puts the declarations on the include-stripping path, where a conditional
/// declaration used to be dropped while the file was still reported to the runtime as one the
/// compiler included — so nobody declared the function (`Call to undefined function
/// probe_upper()`). This is Symfony's `polyfill-mbstring` shape.
#[test]
fn an_eager_file_include_declares_its_guarded_global_functions() {
    let out = compile_and_run_project(
        "eager_polyfill_2hop",
        &[
            ("module.json", MANIFEST),
            ("boot/bootstrap.php", DELEGATING_BOOTSTRAP),
            ("boot/bootstrap80.php", GUARDED_DECLARATIONS),
            ("boot/bootstrap72.php", GUARDED_DECLARATIONS),
            (
                "main.php",
                r#"<?php
echo probe_upper('ab'), ':', probe_width('abc'), "\n";
"#,
            ),
        ],
        "main.php",
    );
    assert_eq!(out, "AB:3\n");
}
