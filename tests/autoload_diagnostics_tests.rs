//! Purpose:
//! End-to-end tests for what the CLI does with the Composer AUTOLOAD CLOSURE of a program: the
//! file each diagnostic names, and the standard-library preludes a class file reached only
//! through autoload needs.
//!
//! Called from:
//! - `cargo test --test autoload_diagnostics_tests` through Rust's test harness.
//!
//! Key details:
//! - Autoloaded class files are loaded after name resolution, long after the entry file was
//!   lexed and scanned. Two things used to miss them:
//!   - every checker diagnostic in such a file printed `error[line:col]`, naming no file, so on a
//!     large vendor tree the error could not be located;
//!   - the PDO/curl/... preludes were decided on the entry file alone, so a class file typed
//!     against `PDO` failed with `Unknown type: PDO` unless `--with-pdo` forced it.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) with `--check` as a subprocess in an
//!   isolated temp dir, so nothing is assembled or linked.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Writes `files` into a fresh canonicalized temp dir and returns it.
fn project(prefix: &str, files: &[(&str, &str)]) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "{}_{}_{:?}_{}",
        prefix,
        std::process::id(),
        std::thread::current().id(),
        id
    ));
    fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    for (path, content) in files {
        let full = dir.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }
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

/// Runs `elephc --check` on `<dir>/main.php`.
fn check(dir: &Path) -> Output {
    Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(dir)
        .arg("--check")
        .arg(dir.join("main.php"))
        .output()
        .expect("failed to spawn elephc")
}

const COMPOSER: &str = r#"{"autoload":{"psr-4":{"App\\":"src/"}}}"#;

/// Autoload registration warnings are emitted before include resolution and still name the entry.
#[test]
fn early_autoload_warning_names_the_entry_file() {
    let dir = project(
        "elephc_early_autoload_warning",
        &[("main.php", "<?php\n$base = __DIR__;\nspl_autoload_register(function ($name) use ($base) {});\necho 'ok';\n")],
    );
    let output = check(&dir);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let expected = format!(
        "warning[{}:3:1]: spl_autoload_register: closure rejected", dir.join("main.php").display(),
    );
    fs::remove_dir_all(&dir).unwrap();
    assert!(output.status.success(), "autoload warning must remain non-fatal:\n{stderr}");
    assert!(stderr.contains(&expected),
        "early warning should name the entry file ({expected}), got:\n{stderr}");
}

/// A checker error inside an autoloaded class file names that file, not just a line.
#[test]
fn checker_error_in_an_autoloaded_file_names_the_file() {
    let dir = project(
        "elephc_autoload_diag_path",
        &[
            ("composer.json", COMPOSER),
            (
                "src/Broken.php",
                "<?php\nnamespace App;\nclass Broken {\n    public function run(): int { return new Missing(); }\n}\n",
            ),
            ("main.php", "<?php\n$b = new App\\Broken();\necho $b->run();\n"),
        ],
    );
    let output = check(&dir);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "expected a checker error, got success");
    let expected = format!("error[{}:4:", dir.join("src/Broken.php").display());
    assert!(
        stderr.contains(&expected),
        "diagnostic should name the autoloaded file ({expected}), got:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A checker error in the entry file names the entry file too.
#[test]
fn checker_error_in_the_entry_file_names_the_file() {
    let dir = project(
        "elephc_entry_diag_path",
        &[("main.php", "<?php\n$x = 1;\necho new Missing();\n")],
    );
    let output = check(&dir);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "expected a checker error, got success");
    let expected = format!("{}:3:", dir.join("main.php").display());
    assert!(
        stderr.contains(&expected),
        "diagnostic should name the entry file ({expected}), got:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// An include-resolution error in the entry file names the entry file as well: the path is
/// registered before the resolver runs, so `require 'missing.php'` reports
/// `error[<dir>/main.php:2:1]` instead of the file-less `error[2:1]`.
#[test]
fn missing_require_in_the_entry_file_names_the_file() {
    let dir = project(
        "elephc_entry_missing_require",
        &[("main.php", "<?php\nrequire 'missing.php';\necho 'x';\n")],
    );
    let output = check(&dir);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "expected an include error, got success");
    let expected = format!("error[{}:2:1]", dir.join("main.php").display());
    assert!(
        stderr.contains(&expected),
        "include error should name the entry file ({expected}), got:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A class file reached only through autoload that types against `PDO` and `CurlHandle` gets
/// the PDO and curl preludes, without `--with-pdo` / `--with-curl`.
#[test]
fn autoloaded_class_typed_against_pdo_and_curl_gets_their_preludes() {
    let dir = project(
        "elephc_autoload_prelude",
        &[
            ("composer.json", COMPOSER),
            (
                "src/Store.php",
                "<?php\nnamespace App;\nuse PDO;\nclass Store {\n    public ?PDO $pdo = null;\n    public ?\\CurlHandle $curl = null;\n    public function connected(): bool { return $this->pdo !== null || $this->curl !== null; }\n}\n",
            ),
            (
                "main.php",
                "<?php\n$s = new App\\Store();\necho $s->connected() ? \"yes\" : \"no\";\n",
            ),
        ],
    );
    let output = check(&dir);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "an autoloaded PDO/curl user should check without --with-*, got:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Two autoloaded class files whose retyping assignments sit at the same line and column are
/// told apart: each file has its own source identity, so the per-position binding decisions no
/// longer collide ("Cannot re-bind $x here", issue #778) and both methods run.
#[test]
fn autoloaded_files_with_retypes_at_the_same_position_compile() {
    let class = |name: &str, first: &str, second: &str| {
        format!(
            "<?php\nnamespace App;\nclass {name} {{\n    public function f() {{ $x = {first}; echo $x; $x = \"{second}\"; echo $x, \"\\n\"; }}\n}}\n"
        )
    };
    let a = class("A", "0", "s");
    let b = class("B", "1", "t");
    let dir = project(
        "elephc_autoload_same_position",
        &[
            ("composer.json", COMPOSER),
            ("src/A.php", &a),
            ("src/B.php", &b),
            ("main.php", "<?php\n(new App\\A())->f();\n(new App\\B())->f();\n"),
        ],
    );
    let output = check(&dir);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "same-position retypes in two autoloaded files should check, got:\n{stderr}"
    );
    assert!(!stderr.contains("Cannot re-bind"), "unexpected re-bind error:\n{stderr}");
    let _ = fs::remove_dir_all(&dir);
}
