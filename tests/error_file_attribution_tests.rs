//! Purpose:
//! End-to-end tests that a type-check diagnostic names the physical file its declaration was
//! written in, not just a line and column.
//!
//! Called from:
//! - `cargo test --test error_file_attribution_tests` through Rust's test harness.
//!
//! Key details:
//! - A `Span` carries line and column only, deliberately: it is 16 bytes in every token and AST
//!   node. That is enough while one file is being parsed and useless afterwards, because include
//!   and autoload expansion splice hundreds of files into one program whose line numbers collide.
//!   The file therefore has to be recovered from the enclosing DECLARATION, which the checker
//!   records and `pipeline` maps back to a path.
//! - Both halves of that map are exercised: a class-like (tagged by the method pass) and a plain
//!   function (tagged by signature resolution, and renamed to an include-variant symbol on the way,
//!   so the map has to know that symbol too).
//! - These assert on the CLI's real stderr, which is the only place the wiring is observable.

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

/// Writes `files` into a fresh project, runs `--check` on `main.php`, and returns stderr.
///
/// The check is expected to FAIL: every caller here plants exactly one diagnostic.
fn check_project(files: &[(&str, &str)]) -> String {
    let dir = make_test_dir("elephc_error_attribution");
    for (path, contents) in files {
        let full = dir.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, contents).unwrap();
    }

    let output = Command::new(elephc_bin())
        .args(["--check", "main.php"])
        .current_dir(&dir)
        .output()
        .expect("failed to spawn elephc");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "expected --check to fail, stderr was: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
    stderr
}

/// Asserts `stderr` names `file` and, on the same diagnostic line, `line`.
fn assert_reports_at(stderr: &str, file: &str, line: u32, message: &str) {
    let found = stderr.lines().find(|entry| entry.contains(message));
    let entry = found.unwrap_or_else(|| {
        panic!("no diagnostic mentioning '{message}' in:\n{stderr}");
    });
    assert!(
        entry.contains(file),
        "diagnostic does not name '{file}': {entry}"
    );
    assert!(
        entry.contains(&format!("{file}:{line}:")),
        "diagnostic does not point at {file}:{line}: {entry}"
    );
}

const MAIN: &str = r#"<?php
require __DIR__ . '/lib/Widget.php';
require __DIR__ . '/lib/helpers.php';

$w = new Widget();
echo $w->label(), describe();
"#;

/// Verifies a diagnostic raised inside an included class's METHOD names that class's file.
///
/// The method pass is what knows the declaring class here; without it the error reads
/// `error[7:21]`, a coordinate in the spliced program that matches a line in every other
/// included file too.
#[test]
fn method_body_error_names_the_class_file() {
    let widget = r#"<?php

class Widget
{
    public function label(): string
    {
        return $this->missingHelper();
    }
}
"#;
    let helpers = "<?php\n\nfunction describe(): string\n{\n    return \"d\";\n}\n";
    let stderr = check_project(&[
        ("main.php", MAIN),
        ("lib/Widget.php", widget),
        ("lib/helpers.php", helpers),
    ]);
    assert_reports_at(
        &stderr,
        "lib/Widget.php",
        7,
        "Undefined method: Widget::missingHelper",
    );
}

/// Verifies a diagnostic raised inside an included FUNCTION names that function's file.
///
/// An include-loaded function is renamed to `__elephc_include_variant_<hash>_<name>` before the
/// checker ever sees it, so the declaration-to-path map has to carry the renamed symbol as well
/// as the public one — that renaming is why this case needs its own test and does not follow
/// from the method one.
#[test]
fn function_body_error_names_the_function_file() {
    let widget = r#"<?php

class Widget
{
    public function label(): string
    {
        return "w";
    }
}
"#;
    let helpers = r#"<?php

function describe(): string
{
    $w = new Widget();
    $w->nope();
    return "d";
}
"#;
    let stderr = check_project(&[
        ("main.php", MAIN),
        ("lib/Widget.php", widget),
        ("lib/helpers.php", helpers),
    ]);
    assert_reports_at(&stderr, "lib/helpers.php", 6, "Undefined method: Widget::nope");
}

/// Verifies the file a diagnostic names is the one that DECLARES the code, not the entry file
/// that happens to include it.
///
/// Two included files declare a method at the same line number; only the declaring file's path
/// tells them apart, which is the whole point of the attribution.
#[test]
fn colliding_line_numbers_are_told_apart_by_file() {
    let first = r#"<?php

class First
{
    public function run(): string
    {
        return "ok";
    }
}
"#;
    let second = r#"<?php

class Second
{
    public function run(): string
    {
        return $this->absent();
    }
}
"#;
    let main = r#"<?php
require __DIR__ . '/lib/First.php';
require __DIR__ . '/lib/Second.php';

echo (new First())->run(), (new Second())->run();
"#;
    let stderr = check_project(&[
        ("main.php", main),
        ("lib/First.php", first),
        ("lib/Second.php", second),
    ]);
    assert_reports_at(&stderr, "lib/Second.php", 7, "Undefined method: Second::absent");
    assert!(
        !stderr.contains("lib/First.php"),
        "the clean file must not be named: {stderr}"
    );
}

/// Verifies a plain single-file program is unaffected: nothing was spliced, so there is no
/// declaration map to consult and the diagnostic keeps its original shape.
#[test]
fn single_file_program_still_reports_its_own_error() {
    let dir = make_test_dir("elephc_error_attribution_single");
    let path = dir.join("solo.php");
    fs::write(
        &path,
        "<?php\nclass Solo { public function go(): string { return $this->absent(); } }\necho (new Solo())->go();\n",
    )
    .unwrap();

    let output = Command::new(elephc_bin())
        .args(["--check", "solo.php"])
        .current_dir(&dir)
        .output()
        .expect("failed to spawn elephc");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(!output.status.success(), "expected --check to fail: {stderr}");
    assert!(
        stderr.contains("Undefined method: Solo::absent"),
        "unexpected stderr: {stderr}"
    );
    let _ = fs::remove_dir_all(Path::new(&dir));
}
