//! Purpose:
//! Guards the shape every PHP polyfill ships: an eager `autoload.files` entry that declares global
//! functions behind `if (!function_exists('f'))`. One hop works; two hops do not, and the second
//! test pins exactly where the boundary is.
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
/// the include-stripping path that drops them. This is the green anchor for the ignored test
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
