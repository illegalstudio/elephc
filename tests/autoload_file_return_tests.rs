//! Purpose:
//! Guards php's INCLUDE BOUNDARY for a file the autoload pass splices into the program. A
//! top-level `return` in an included file ends THAT FILE; it must not return from the program
//! that included it, and it must not take the program's later declarations with it.
//!
//! Called from:
//! - `cargo test --test autoload_file_return_tests` through Rust's test harness.
//!
//! Key details:
//! - This suite exists because of a measured build failure. Composer's eager `autoload.files`
//!   list names `vendor/symfony/polyfill-mbstring/bootstrap.php`, whose last line is
//!   `return require __DIR__.'/bootstrap72.php';`. `crate::autoload`'s `load_autoloaded_file`
//!   parses that file on its own and splices its NAME-RESOLVED statements straight into the
//!   entry program, so the file's `return` became top-level statement 160 of an 1,848-statement
//!   Symfony `--web` program. `optimize::propagate`'s `propagate_block` stops at the first
//!   statement that does not fall through, so 1,688 top-level statements — 91% of the program,
//!   including the whole `--web` error-handling prelude — were deleted from the AST while the
//!   CHECKER's function table kept them. The build died in the BACKEND with
//!   `unsupported EIR backend feature: call to unknown function error_log`, four passes away
//!   from the cause and naming a function nobody had touched.
//! - `crate::resolver` has restored this boundary for a WRITTEN-OUT `require` since #529
//!   (`discard_statement_include_return`). Only the autoload path was missing it, which is why
//!   no single-file probe and no `require`-based test could see the defect.
//! - EXPECTED OUTPUT WAS CAPTURED FROM php 8.5.10, on the `require` form of the same two files
//!   (`require __DIR__.'/eager.php';` — which is exactly what Composer's generated
//!   `autoload_real.php` does with every `autoload.files` entry, in statement position with the
//!   value discarded). php prints, in order: `eager-top`, `eager-return-expr`, `entry-ran`,
//!   `still-bound`.
//! - The three tests are chosen so each fails on a different half of the fix:
//!   `a_returning_autoloaded_file_does_not_end_the_program` fails if the `return` is left alone;
//!   `a_returning_autoloaded_file_evaluates_its_return_expression` fails if the `return` is
//!   dropped instead of degraded to its expression; and
//!   `declarations_after_an_autoloaded_return_are_still_bound` fails if the tail is truncated
//!   wholesale. Each test's doc comment names the source edit that turns it red.

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

/// Compiles the default project and runs the resulting executable, returning its stdout.
fn compile_and_run(prefix: &str) -> String {
    compile_and_run_project(prefix, &PROJECT)
}

/// Compiles an arbitrary project and runs the resulting executable, returning its stdout.
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

/// A Composer manifest with BOTH an eager `files` entry and the PSR-4 map the entry uses.
///
/// The eager entry is what reproduces the original: `autoload.files` members are loaded by
/// `crate::autoload`'s own prefix loop, never by a `require` the resolver can see.
const MANIFEST: &str = r#"{"autoload":{"psr-4":{"App\\":"src/"},"files":["boot/eager.php"]}}"#;

/// The eager file, in the shape the `symfony/polyfill-*` bootstraps use: a side effect, a
/// top-level `return` OF AN EXPRESSION, and a declaration below it that php still binds.
const EAGER_RETURNING: &str = r#"<?php
function eager_marker(): string { echo "eager-return-expr\n"; return 'x'; }

echo "eager-top\n";

return eager_marker();

function after_the_return(): string { return 'still-bound'; }
"#;

/// An entry that prints after the eager file has run and calls what that file left behind.
const ENTRY: &str = r#"<?php
echo "entry-ran\n";
echo function_exists('after_the_return') ? after_the_return() : 'missing', "\n";
echo (new App\Probe())->id(), "\n";
"#;

/// A PSR-4 class, so the entry keeps a reason to run the autoload pass at all.
const PROBE_CLASS: &str = r#"<?php
namespace App;

class Probe
{
    public function id(): string
    {
        return 'probe';
    }
}
"#;

/// The whole project, so every test compiles byte-identical input.
const PROJECT: [(&str, &str); 4] = [
    ("composer.json", MANIFEST),
    ("boot/eager.php", EAGER_RETURNING),
    ("src/Probe.php", PROBE_CLASS),
    ("entry.php", ENTRY),
];

/// THE REGRESSION. An eagerly autoloaded file that ends in `return` must not end the PROGRAM.
///
/// Before the fix the compiled binary printed only the eager file's own two lines and then
/// stopped: everything after the spliced `return` — the entry included — was deleted by
/// `optimize::propagate`.
///
/// TEETH, by mutation: removing the `discard_autoloaded_file_return` call from
/// `crate::autoload::load_autoloaded_file` makes this assertion fail, with a stdout that ends at
/// `eager-return-expr`.
#[test]
fn a_returning_autoloaded_file_does_not_end_the_program() {
    let stdout = compile_and_run("elephc_autoload_return");
    assert!(
        stdout.contains("entry-ran"),
        "the entry never ran; the eager file's return ended the program:\n{stdout}"
    );
    assert!(
        stdout.contains("probe"),
        "the autoloaded class was lost with the truncated tail:\n{stdout}"
    );
}

/// php EVALUATES the returned expression before ending the file, and so must the splice.
///
/// `return eager_marker();` is a call with an observable effect, which is the one thing a fix
/// that simply DELETES the `return` statement would lose while leaving the other two tests
/// green.
///
/// TEETH, by mutation: replacing the `ExprStmt(value)` rewrite in
/// `crate::autoload::discard_autoloaded_file_return` with the empty
/// `StmtKind::Synthetic(Vec::new())` placeholder it uses for a bare `return;` drops
/// `eager-return-expr` from stdout and fails only this test.
#[test]
fn a_returning_autoloaded_file_evaluates_its_return_expression() {
    let stdout = compile_and_run("elephc_autoload_return_value");
    assert!(
        stdout.contains("eager-top"),
        "the eager file's statements above its return did not run:\n{stdout}"
    );
    assert!(
        stdout.contains("eager-return-expr"),
        "the returned expression was dropped instead of evaluated:\n{stdout}"
    );
}

/// php binds an unconditional top-level declaration when the file is COMPILED, so one written
/// BELOW a `return` is still callable afterwards.
///
/// Verified directly on php 8.5.10: with `<?php return 1; function after_return_fn() {}` and a
/// `require` of it, `function_exists('after_return_fn')` answers `true` in the includer.
///
/// TEETH, by mutation: dropping the `is_file_scope_hoisted_declaration` filter from
/// `discard_autoloaded_file_return` — truncating the tail outright, the shape
/// `crate::resolver`'s own `discard_statement_include_return` may use because declarations have
/// already been hoisted out by then — prints `missing` here while the other two tests stay green.
#[test]
fn declarations_after_an_autoloaded_return_are_still_bound() {
    let stdout = compile_and_run("elephc_autoload_return_decl");
    assert!(
        stdout.contains("still-bound"),
        "a declaration below the eager file's return was not bound:\n{stdout}"
    );
}

// ===== A FILE-SCOPE `return` UNDER A GUARD =====
//
// The three tests above cover the `return` written at an eager file's TOP LEVEL. A library
// writes it under a guard just as often, and php's rule is the same one: the `return` ends THAT
// FILE. Two shapes carry the whole Symfony `--web` eager set between them:
//
//   if (extension_loaded('mbstring')) { return; }           -- runtime condition, end of file
//   if (\PHP_VERSION_ID >= 80000) { return require …; }     -- profile condition, pick a file
//
// MEASURED: with Composer's eager `autoload.files` in the closed world, the first shape — from
// `symfony/polyfill-mbstring/bootstrap80.php`, and `extension_loaded('mbstring')` is TRUE in a
// compiled binary — returned from the REQUEST PROGRAM. Every Symfony route answered `200` with a
// zero-byte body and no diagnostic anywhere. The second shape contributed BOTH files'
// declarations instead of the one php contributes, and the assembler refused the program with
// `symbol '_fn_grapheme_u_levenshtein' is already defined`.

/// An eager file whose guard is TRUE ends THAT FILE, and the program keeps going.
///
/// `function_exists('strlen')` is true in php and in a compiled binary alike, so php runs the
/// `return`, skips the rest of the eager file, and still runs everything that included it.
///
/// TEETH, by mutation: removing the `is_bare_if_ending_in_return` arm from
/// `crate::autoload::discard_autoloaded_file_return` makes stdout stop at `guard-top` — no
/// `entry-ran`, no diagnostic, exit status 0.
#[test]
fn a_taken_guard_ends_the_eager_file_and_not_the_program() {
    let stdout = compile_and_run_project(
        "elephc_autoload_guarded_return_taken",
        &[
            ("composer.json", r#"{"autoload":{"files":["boot/guard.php"]}}"#),
            ("boot/guard.php", GUARD_TAKEN),
            ("entry.php", GUARD_ENTRY),
        ],
    );
    assert!(
        stdout.contains("guard-top"),
        "the eager file's statements above its guard did not run:\n{stdout}"
    );
    assert!(
        !stdout.contains("guard-tail"),
        "the eager file kept running after a taken file-scope return:\n{stdout}"
    );
    assert!(
        stdout.contains("entry-ran"),
        "the eager file's guarded return ended the PROGRAM:\n{stdout}"
    );
}

/// An eager file whose guard is FALSE runs the rest of itself.
///
/// This is the half a fix that simply DELETED the statements after the guard would lose, and the
/// reason the rewrite has to be an `else` rather than a truncation.
///
/// TEETH, by mutation: replacing `else_body: (!executable.is_empty()).then_some(executable)` with
/// `else_body: None` in `crate::autoload::end_file_under_guard` drops `guard-tail` here and
/// leaves the other two guard tests green.
#[test]
fn an_untaken_guard_leaves_the_rest_of_the_eager_file_running() {
    let stdout = compile_and_run_project(
        "elephc_autoload_guarded_return_untaken",
        &[
            ("composer.json", r#"{"autoload":{"files":["boot/guard.php"]}}"#),
            ("boot/guard.php", GUARD_UNTAKEN),
            ("entry.php", GUARD_ENTRY),
        ],
    );
    assert!(
        stdout.contains("guard-tail"),
        "the rest of the eager file was dropped although its guard was false:\n{stdout}"
    );
    assert!(
        stdout.contains("entry-ran"),
        "the entry did not run:\n{stdout}"
    );
}

/// A `\PHP_VERSION_ID` guard picks the file php picks, and the OTHER branch contributes nothing.
///
/// This is `symfony/polyfill-*`'s bootstrap pair reduced to what makes it fail: the condition is
/// decided by the compile-time profile, and php resolves it before either file's declarations
/// exist. Contributing both is what put two definitions of one function in the assembly.
///
/// TEETH — AND THE HONEST LIMIT OF THEM. No SINGLE edit turns this red, and that was measured,
/// not assumed: with `fold_file_scope_profile_conditions` removed the guarded-`if` rewrite in
/// `discard_autoloaded_file_return` still stops the `return` and still skips `legacy-ran`, and
/// with that rewrite removed the fold does the same on its own. Only removing BOTH prints
/// `modern-ran` and then nothing. It is kept because it is the only test that states what php
/// actually does with the pair — run one file, skip the other, keep going — and because the two
/// edits it covers are each pinned by a test of their own:
/// `two_eager_files_guarded_by_the_profile_do_not_both_declare` for the fold and
/// `a_taken_guard_ends_the_eager_file_and_not_the_program` for the rewrite.
#[test]
fn a_profile_guard_contributes_only_the_branch_php_takes() {
    let stdout = compile_and_run_project(
        "elephc_autoload_profile_guard",
        &[
            ("composer.json", r#"{"autoload":{"files":["boot/bootstrap.php"]}}"#),
            ("boot/bootstrap.php", PROFILE_GUARD_BOOTSTRAP),
            ("boot/modern.php", "<?php\necho \"modern-ran\\n\";\n"),
            ("entry.php", GUARD_ENTRY),
        ],
    );
    assert!(
        stdout.contains("bootstrap-top") && stdout.contains("modern-ran"),
        "the branch php takes did not run:\n{stdout}"
    );
    assert!(
        !stdout.contains("legacy-ran"),
        "the branch php never takes ran as well:\n{stdout}"
    );
    assert!(
        stdout.contains("entry-ran"),
        "the guarded `return` ended the PROGRAM instead of the file:\n{stdout}"
    );
}

/// TWO eager files that end themselves under the same profile guard must not both contribute.
///
/// THIS IS THE ONE THAT FAILS THE ASSEMBLER. The runtime tests above are satisfied by any rewrite
/// that stops the `return` escaping — an `else` around the rest of the file does that, and leaves
/// the rest of the file EMITTED. elephc emits a declaration nested in an `if` whether or not the
/// branch runs, so two files that both carry `if (!function_exists('x')) { function x() {} }`
/// below a guard php never passes put two definitions of `x` in one object file:
///
/// ```text
/// error: symbol '_fn_shared_u_poly' is already defined
/// ```
///
/// Only DROPPING the branch php never takes fixes that, which is what makes
/// `fold_file_scope_profile_conditions` load-bearing rather than a tidier spelling of the `else`.
/// `symfony/polyfill-php85` and `symfony/polyfill-intl-grapheme` are this fixture, in a 1.4 GB
/// assembly: both declare `grapheme_levenshtein` under a guard, and php declares neither.
///
/// TEETH, by mutation: removing the `fold_file_scope_profile_conditions` call from
/// `crate::autoload::load_autoloaded_file` makes THIS test fail — the compile dies in the
/// assembler with `symbol '_fn_shared_u_poly' is already defined` — while every other test in this
/// file stays green.
#[test]
fn two_eager_files_guarded_by_the_profile_do_not_both_declare() {
    let first = duplicate_polyfill("a");
    let second = duplicate_polyfill("b");
    let stdout = compile_and_run_project(
        "elephc_autoload_profile_guard_duplicate",
        &[
            (
                "composer.json",
                r#"{"autoload":{"files":["boot/a.php","boot/b.php"]}}"#,
            ),
            ("boot/a.php", first.as_str()),
            ("boot/b.php", second.as_str()),
            (
                "entry.php",
                "<?php\necho function_exists('shared_poly') ? \"declared\\n\" : \"absent\\n\";\n",
            ),
        ],
    );
    assert!(
        !stdout.is_empty(),
        "the entry produced nothing, so an eager file ended the program"
    );
}

/// One half of the duplicate fixture: a polyfill below a profile guard php never passes.
fn duplicate_polyfill(answer: &str) -> String {
    format!(
        "<?php\n\nif (\\PHP_VERSION_ID >= 80000) {{\n    return;\n}}\n\nif (!function_exists('shared_poly')) {{\n    function shared_poly(): string {{ return '{answer}'; }}\n}}\n"
    )
}

/// A guarded `return` in a `require`d file ends THAT file, and the file that required it goes on.
///
/// The include boundary has two sides and this is the other one: `crate::autoload` restores it
/// for the file the autoload pass loads, `crate::resolver` for every file that one `require`s.
/// Without the resolver's half the includer's own tail is lost — the statements after the
/// `require` are treated as belonging to the file that ended, because by the time the autoload
/// pass sees them `name_resolver` has flattened the two files into one list.
///
/// TEETH, by mutation: removing the `is_guarded_end_of_file` arm from
/// `crate::resolver::engine_includes::discard_statement_include_return` drops `bootstrap-tail`
/// here and leaves every other test in this file green.
#[test]
fn a_guarded_return_in_a_required_file_does_not_end_the_file_that_required_it() {
    let stdout = compile_and_run_project(
        "elephc_autoload_required_guarded_return",
        &[
            ("composer.json", r#"{"autoload":{"files":["boot/bootstrap.php"]}}"#),
            (
                "boot/bootstrap.php",
                "<?php\n\nrequire __DIR__.'/inner.php';\n\necho \"bootstrap-tail\\n\";\n",
            ),
            ("boot/inner.php", GUARD_TAKEN),
            ("entry.php", GUARD_ENTRY),
        ],
    );
    assert!(
        stdout.contains("guard-top"),
        "the required file did not run:\n{stdout}"
    );
    assert!(
        !stdout.contains("guard-tail"),
        "the required file kept running after its own file-scope return:\n{stdout}"
    );
    assert!(
        stdout.contains("bootstrap-tail"),
        "the required file's return ended the file that required it:\n{stdout}"
    );
    assert!(
        stdout.contains("entry-ran"),
        "the required file's return ended the PROGRAM:\n{stdout}"
    );
}

/// An eager file that ends itself under a condition TRUE in a compiled binary.
const GUARD_TAKEN: &str = r#"<?php
echo "guard-top\n";

if (function_exists('strlen')) {
    return;
}

echo "guard-tail\n";
"#;

/// The same file with a condition nothing satisfies, so php runs all of it.
const GUARD_UNTAKEN: &str = r#"<?php
echo "guard-top\n";

if (function_exists('elephc_no_such_function_zz')) {
    return;
}

echo "guard-tail\n";
"#;

/// `symfony/polyfill-*`'s bootstrap shape: a profile guard that hands the file over to another.
const PROFILE_GUARD_BOOTSTRAP: &str = r#"<?php
echo "bootstrap-top\n";

if (\PHP_VERSION_ID >= 80000) {
    return require __DIR__.'/modern.php';
}

echo "legacy-ran\n";
"#;

/// The entry every guard test compiles: it prints only if the eager file did not end the program.
const GUARD_ENTRY: &str = "<?php\necho \"entry-ran\\n\";\n";
