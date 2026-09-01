//! Purpose:
//! End-to-end tests that a project's declared PHP profile actually reaches the compiled
//! binary, so `--php-version` is not something the user has to remember.
//!
//! Called from:
//! - `cargo test --test php_profile_resolve_tests` through Rust's test harness.
//!
//! Key details:
//! - `php_profile::resolve`'s own unit tests cover every source, their precedence, the
//!   upward walk and the clamping rules. What they CANNOT cover is the wiring: that
//!   `cli::compile_config` consults the resolver at all, and that the profile it produces
//!   survives into the baked version surface. These tests assert on the RUN PROGRAM's output,
//!   which is the only place that is observable.
//! - `bare_file_needs_no_manifest` is the guarantee that matters most: adding this feature
//!   must not make a lone `.php` file require project files.

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
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// A program that prints the version surface it was compiled with.
const PROBE: &str = "<?php echo PHP_VERSION, \"|\", PHP_VERSION_ID;\n";

/// Compiles `PROBE` at `sub` inside a freshly built project and returns its stdout.
///
/// `extra_args` lets a test add `--php-version` to check that the flag still wins.
fn build_and_run(dir: &Path, sub: &str, extra_args: &[&str]) -> String {
    let source_dir = dir.join(sub);
    fs::create_dir_all(&source_dir).unwrap();
    fs::write(source_dir.join("prog.php"), PROBE).unwrap();

    let rel = if sub.is_empty() {
        "prog.php".to_string()
    } else {
        format!("{sub}/prog.php")
    };
    let mut args: Vec<&str> = extra_args.to_vec();
    args.push(&rel);

    let compile = Command::new(elephc_bin())
        .args(&args)
        .current_dir(dir)
        .output()
        .expect("failed to spawn elephc");
    assert!(
        compile.status.success(),
        "compile failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(source_dir.join("prog"))
        .output()
        .expect("failed to run compiled program");
    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// A lone `.php` file with no project files still compiles, at the default profile.
///
/// This is the guarantee the whole feature is built around: detection must never turn into a
/// requirement.
#[test]
fn bare_file_needs_no_manifest() {
    let dir = make_test_dir("elephc_resolve_bare");
    assert_eq!(build_and_run(&dir, "", &[]), "8.5.10-dev|80510");
    let _ = fs::remove_dir_all(&dir);
}

/// A malformed `composer.json` never fails the build, and says why the pin was not read.
///
/// This is the branch where elephc knows a project TRIED to declare something and could not
/// honor it. Failing would make elephc the arbiter of a file it does not own; staying silent
/// would leave a pin that looks applied and is not. The note is the only honest answer, and
/// the profile falls back to the default rather than to a guess.
#[test]
fn a_malformed_manifest_compiles_and_explains_itself() {
    let dir = make_test_dir("elephc_resolve_malformed");
    fs::write(dir.join("composer.json"), r#"{"config": {"platform": "#).unwrap();
    fs::write(dir.join("prog.php"), PROBE).unwrap();

    let compile = Command::new(elephc_bin())
        .arg("prog.php")
        .current_dir(&dir)
        .output()
        .expect("failed to spawn elephc");
    assert!(
        compile.status.success(),
        "a manifest elephc does not own must never fail the build:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let stderr = String::from_utf8_lossy(&compile.stderr);
    assert!(
        stderr.contains("composer.json could not be parsed"),
        "expected a note explaining the unread pin, got:\n{stderr}"
    );

    let run = Command::new(dir.join("prog"))
        .output()
        .expect("failed to run compiled program");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "8.5.10-dev|80510");
    let _ = fs::remove_dir_all(&dir);
}

/// A `config.platform.php` pin reaches the baked version surface, with no flag passed.
#[test]
fn composer_platform_pin_reaches_the_binary() {
    let dir = make_test_dir("elephc_resolve_platform");
    fs::write(
        dir.join("composer.json"),
        r#"{"name":"acme/app","config":{"platform":{"php":"8.3.11"}}}"#,
    )
    .unwrap();
    assert_eq!(build_and_run(&dir, "", &[]), "8.3.0|80300");
    let _ = fs::remove_dir_all(&dir);
}

/// The pin is found by walking UP from the entry file, not only beside it.
#[test]
fn pin_is_found_from_a_nested_source_dir() {
    let dir = make_test_dir("elephc_resolve_nested");
    fs::write(
        dir.join("composer.json"),
        r#"{"config":{"platform":{"php":"8.4"}}}"#,
    )
    .unwrap();
    assert_eq!(build_and_run(&dir, "src/app", &[]), "8.4.0|80400");
    let _ = fs::remove_dir_all(&dir);
}

/// `composer.lock` outranks `composer.json`: it records what was actually installed against.
#[test]
fn lock_outranks_manifest_in_the_binary() {
    let dir = make_test_dir("elephc_resolve_lock");
    fs::write(
        dir.join("composer.json"),
        r#"{"config":{"platform":{"php":"8.2"}}}"#,
    )
    .unwrap();
    fs::write(
        dir.join("composer.lock"),
        r#"{"platform-overrides":{"php":"8.4"}}"#,
    )
    .unwrap();
    assert_eq!(build_and_run(&dir, "", &[]), "8.4.0|80400");
    let _ = fs::remove_dir_all(&dir);
}

/// An explicit `--php-version` still wins over everything the project declares.
#[test]
fn explicit_flag_still_wins() {
    let dir = make_test_dir("elephc_resolve_flag");
    fs::write(
        dir.join("composer.json"),
        r#"{"config":{"platform":{"php":"8.3"}}}"#,
    )
    .unwrap();
    assert_eq!(
        build_and_run(&dir, "", &["--php-version", "8.5"]),
        "8.5.10-dev|80510"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A `require.php` constraint is not a pin, so it leaves the default in place.
#[test]
fn require_constraint_leaves_the_default() {
    let dir = make_test_dir("elephc_resolve_require");
    fs::write(dir.join("composer.json"), r#"{"require":{"php":"^8.2"}}"#).unwrap();
    assert_eq!(build_and_run(&dir, "", &[]), "8.5.10-dev|80510");
    let _ = fs::remove_dir_all(&dir);
}
