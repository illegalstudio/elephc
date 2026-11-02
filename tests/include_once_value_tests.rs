//! Purpose:
//! Guards the VALUE a value-position `include`/`require` evaluates to, which php decides from
//! whether the file's body actually ran.
//!
//! Called from:
//! - `cargo test --test include_once_value_tests` through Rust's test harness.
//!
//! Key details:
//! - php has three answers and elephc's compiled expansion used to have two. A file that RAN and
//!   reached a top-level `return E` evaluates to `E`; a file that RAN and reached no top-level
//!   `return` evaluates to `1`; a REPEAT `include_once`/`require_once`, which runs nothing,
//!   evaluates to the literal `true`. `crate::resolver::engine_includes::expand_value_include`
//!   seeded `1` for the last two together, so `$x = require_once F;` on a repeat inclusion came
//!   out as `1` where php says `true`, and `true === $x` flipped from true to false.
//! - EXPECTED OUTPUT WAS CAPTURED FROM php 8.5.10 (the same build the rest of this worktree is
//!   verified against) by running each fixture below through `php entry.php` unchanged. The
//!   probes print `var_dump($v === true)` and `var_dump($v === 1)` rather than the value itself,
//!   because the whole defect is a type distinction that `echo` erases: `echo 1` and
//!   `echo true` both print `1`.
//! - WHY THIS SHAPE MATTERS BEYOND php parity: Symfony Runtime's generated
//!   `vendor/autoload_runtime.php` opens with
//!   `if (true === (require_once __DIR__.'/autoload.php') || ...) { return; }` — it uses exactly
//!   this distinction to decide whether to run the application at all. Answering `1` instead of
//!   `true` there runs the app twice; answering `true` instead of the loader serves nothing.
//! - Each test fails on a different half of the fix, so a partial revert cannot stay green.
//!   `a_repeat_require_once_evaluates_to_true` fails if the `true` seed outside the guard is
//!   dropped; `a_first_require_once_evaluates_to_the_files_return` fails if a captured top-level
//!   `return` stops delivering the file's own value; and
//!   `a_running_file_without_a_return_evaluates_to_one` fails if the `1` seed is not placed
//!   inside the guarded body, which is the half that keeps the "it ran" answer distinct from
//!   the "it was skipped" answer. Each was confirmed red under exactly that edit.
//! - ONE php answer is deliberately NOT asserted here. A repeat inclusion of a file with no
//!   top-level `return` should be `true` and the resolver now emits it, but the compiled program
//!   prints `int(1)`: the `true` and the `1` share one local slot and the checker types that slot
//!   `int`, dropping the bool tag. That collapse has nothing to do with includes --
//!   `$x = true; if (never_taken()) { $x = 1; } var_dump($x);` prints `bool(true)` in php 8.5.10
//!   and `int(1)` in elephc -- so asserting it here would only make this suite red for someone
//!   else's defect. See `SILENT_PROJECT`.

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

/// Compiles `entry.php` from `files` and returns the compiled program's stdout.
fn compile_and_run_project(prefix: &str, files: &[(&str, &str)]) -> String {
    let dir = make_test_dir(prefix);
    write_project(&dir, files);
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(&dir)
        .arg("entry.php")
        .output()
        .expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc entry.php failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let run = Command::new(dir.join("entry"))
        .output()
        .expect("failed to run compiled binary");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        run.status.success(),
        "compiled binary exited non-zero:\nstdout:\n{stdout}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    stdout
}

/// A file whose body prints and whose top-level `return` hands back an object.
const RETURNING: &str = r#"<?php
class Holder { public $tag = 'held'; }
echo "body-ran\n";
return new Holder();
"#;

/// The same file with no top-level `return` at all: php's other "it ran" answer.
const SILENT: &str = r#"<?php
echo "silent-body-ran\n";
"#;

/// `$v = require_once F;` twice over a file that returns an object.
const REPEAT_PROJECT: &[(&str, &str)] = &[
    (
        "entry.php",
        r#"<?php
$first = require_once __DIR__ . '/lib.php';
var_dump($first === true);
$second = require_once __DIR__ . '/lib.php';
var_dump($second === true);
"#,
    ),
    ("lib.php", RETURNING),
];

/// The same two inclusions, asking whether the FIRST one produced the file's own value.
const FIRST_VALUE_PROJECT: &[(&str, &str)] = &[
    (
        "entry.php",
        r#"<?php
$first = require_once __DIR__ . '/lib.php';
var_dump($first instanceof Holder);
"#,
    ),
    ("lib.php", RETURNING),
];

/// A file that runs and returns nothing.
///
/// Only the FIRST inclusion is asserted. php's answer for a REPEAT inclusion of a `return`-less
/// file is `true`, the resolver now emits exactly that, and the IR confirms the placement --
/// `store_local const_bool true` before `include_once_guard`, `store_local const_i64 1` inside
/// its body -- but the compiled program still prints `int(1)`, because the two stores share one
/// local slot and the checker types that slot `int`. The tag, not the include, is what is lost,
/// and it is lost with no include in sight:
///
///     $x = true; if (never_taken()) { $x = 1; } var_dump($x);
///     php 8.5.10: bool(true)          elephc: int(1)
///
/// Asserting the repeat answer here would fail on that separate defect and read as if this
/// suite's own fix had regressed, so it is left to whoever fixes the union collapse.
const SILENT_PROJECT: &[(&str, &str)] = &[
    (
        "entry.php",
        r#"<?php
$first = require_once __DIR__ . '/lib.php';
var_dump($first === 1);
var_dump($first === true);
"#,
    ),
    ("lib.php", SILENT),
];

/// php runs the body once and answers `true` the second time, never `1`.
///
/// Turns red again if `expand_value_include` seeds `1` instead of `true` outside the
/// `IncludeOnceGuard`, which is the exact line this suite was written for.
#[test]
fn a_repeat_require_once_evaluates_to_true() {
    let stdout = compile_and_run_project("include_once_repeat", REPEAT_PROJECT);
    assert_eq!(
        stdout, "body-ran\nbool(false)\nbool(true)\n",
        "php 8.5.10 prints the body once, then false (the object is not `true`) and true"
    );
}

/// The first inclusion still hands back what the file returned, not the repeat answer.
///
/// Turns red if the `true` seed is placed where the file's own `return` cannot overwrite it —
/// for example by seeding it INSIDE the guarded body after the file's statements.
#[test]
fn a_first_require_once_evaluates_to_the_files_return() {
    let stdout = compile_and_run_project("include_once_first", FIRST_VALUE_PROJECT);
    assert_eq!(
        stdout, "body-ran\nbool(true)\n",
        "php 8.5.10 evaluates a first inclusion to the file's own `return` value"
    );
}

/// A file that RAN but returned nothing evaluates to `1`, not to the repeat answer.
///
/// This is the half that needs the `1` seed INSIDE the guard. Remove
/// `seed_guarded_include_body`'s call and only the `true` seed outside the guard is left, so a
/// FIRST inclusion of a `return`-less file reports the answer php reserves for a repeat one, and
/// both assertions below invert.
#[test]
fn a_running_file_without_a_return_evaluates_to_one() {
    let stdout = compile_and_run_project("include_once_silent", SILENT_PROJECT);
    assert_eq!(
        stdout, "silent-body-ran\nbool(true)\nbool(false)\n",
        "php 8.5.10: a first inclusion of a return-less file is int 1, and is not `true`"
    );
}
