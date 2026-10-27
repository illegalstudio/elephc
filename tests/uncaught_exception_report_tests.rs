//! Purpose:
//! End-to-end tests for the fatal diagnostic an UNCAUGHT `throw` prints, and the process exit
//! status that follows it.
//!
//! Before this, every uncaught exception — of any class, with any message — printed one fixed
//! 32-byte string and exited `1`:
//!
//! ```text
//! Fatal error: uncaught exception
//! ```
//!
//! The class and the message were both discarded, so a production crash told you nothing about
//! what had been thrown. `_exc_value` already held the Throwable at that point
//! (`lower_throw_value` publishes it immediately before `__rt_throw_current`); the uncaught arm
//! simply never read it.
//!
//! Reference PHP 8.5.6, measured with `php -d xdebug.mode=off`:
//!
//! ```text
//! Fatal error: Uncaught RuntimeException: boom detail in /path/e.php:2
//! Fatal error: Uncaught MyErr: custom text in /path/m2.php:3
//! Fatal error: Uncaught Exception in /path/m1.php:2        <- EMPTY message: no colon
//! ```
//!
//! elephc now emits the full report PHP does: the class, the message, the ` in <file>:<line>`
//! suffix, the `Stack trace:` block (one `#n file(line): call(args)` line per live frame and the
//! closing `#n {main}`), the `Next ...` chain of previous exceptions, the closing
//! `  thrown in <file> on line <n>` line, and exits `255` like PHP.
//!
//! Two framing details were measured later, from RAW BYTES rather than from the message text, and
//! both had gone unnoticed because reading a merged `2>&1` hides them:
//!
//! - PHP prefixes the report with `\n`, UNCONDITIONALLY — even when the script has written
//!   nothing at all before throwing.
//! - PHP writes it to STDOUT. Captured into separate files, stdout held every byte and stderr was
//!   empty. A program redirecting only stdout used to lose the diagnostic entirely.
//!
//! Called from:
//! - `cargo test --test integration uncaught_exception_report_tests::` through Rust's test harness.
//!
//! Key details:
//! - THE LOCATION IS THE CONSTRUCTION SITE, NOT THE THROW SITE, because that is what PHP reports:
//!   a `new RuntimeException(...)` on line 2 stored in a variable and thrown on line 5 prints
//!   line 2 in both engines. The line is stamped into the Throwable payload when it is allocated,
//!   which is precisely the `new`; `separates_construction_from_throw_site` is the test that would
//!   fail if the line were taken from the `throw` instead, and it is the only one of these tests
//!   where the two differ.
//! - THE STACK TRACE is rendered from the live activation chain at report time. The tests that
//!   exercise it compare the WHOLE output against reference PHP 8.5 text and, when a `php` binary
//!   is on the PATH, against what that binary prints for the very same script. The chain test
//!   keeps both exceptions in one frame on purpose: PHP captures each trace at construction, elephc
//!   walks the frames live at the uncaught throw, so a previous exception built in a callee that
//!   has since returned prints a shorter trace (a documented divergence, not pinned here).
//! - A Throwable with NO user `new` behind it — a `DivisionByZeroError` raised by `intdiv($n, 0)`
//!   — carries line `0`, and the suffix is omitted entirely rather than printed as `:0`.
//!   `synthesized_error_omits_the_location_and_still_exits_255` pins both halves of that: the
//!   omission, and the exit status, which travels a SEPARATE code path
//!   (`codegen::lower_inst::exceptions`) that never reaches `__rt_report_uncaught_exception`.
//! - The EXIT STATUS is asserted separately from the text. It moved from `1` to `255`; a script
//!   that branched on `$?` saw the wrong value before, and that is invisible in stdout/stderr.
//! - The empty-message case is the one that would silently pass with a naive implementation:
//!   writing `": "` unconditionally still looks right in every other test.
//! - A USER SUBCLASS is covered because the class name comes from the runtime
//!   `_class_name_entries` table rather than a compile-time literal, so a name that only exists in
//!   user code is the case that proves the table lookup works.
//! - Host-target only in execution; the emitter change is pinned on both architectures by the unit
//!   tests in `src/codegen_support/runtime/exceptions/uncaught_report.rs`.

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

/// Compiles `source` and returns the executable path.
fn compile(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let output = Command::new(elephc_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .current_dir(dir)
        .arg(&php)
        .output()
        .expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Compiles and runs `source`, returning `(stdout, exit_code)`.
///
/// STDOUT, not stderr: PHP writes this report to stdout. Measured against 8.5 by capturing the
/// two streams into separate files — stdout held all 465 bytes and stderr was empty. Reading it
/// off `2>&1` cannot tell them apart, which is how the stream went unnoticed until a program that
/// redirected only stdout lost its diagnostic entirely.
///
/// The temp directory is NOT removed before returning, because the location suffix names the
/// script by its canonical path and the assertions have to rebuild that path to compare.
/// The returned text now carries the program's OWN output too, since both share stdout — which is
/// why the assertions below match a FRAGMENT rather than a prefix.
fn run_uncaught(prefix: &str, source: &str) -> (String, Option<i32>, PathBuf) {
    let dir = make_test_dir(prefix);
    let bin = compile(&dir, source, prefix);
    let output = Command::new(&bin).output().expect("failed to run compiled binary");
    let reported = String::from_utf8_lossy(&output.stdout).into_owned();
    let code = output.status.code();
    (reported, code, dir)
}

/// Returns the ` in <file>:<line>` suffix reference PHP prints for `script` in `dir`.
///
/// Built from the CANONICALIZED directory, the same normalization
/// `crate::magic_constants::file_pass` applies when it bakes `__FILE__` and `_script_source_file`.
/// On macOS `std::env::temp_dir()` hands back a `/var/...` symlink to `/private/var/...`, so a
/// naive `dir.join(...)` would produce a path that never matches.
fn location_suffix(dir: &Path, script: &str, line: u32) -> String {
    let canonical = dir
        .canonicalize()
        .expect("failed to canonicalize the test directory");
    format!(" in {}:{}", canonical.join(script).display(), line)
}

/// Returns the canonical absolute path reference PHP prints for `script` in `dir`.
fn canonical_script(dir: &Path, script: &str) -> String {
    dir.canonicalize()
        .expect("failed to canonicalize the test directory")
        .join(script)
        .display()
        .to_string()
}

/// Runs `php -n -d display_errors=1` on the script and returns its stdout, or `None` without PHP.
fn php_reference(dir: &Path, stem: &str) -> Option<String> {
    let output = Command::new("php")
        .args(["-n", "-d", "display_errors=1"])
        .arg(canonical_script(dir, &format!("{stem}.php")))
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Compiles and runs `source`, then requires the report to equal `expected_template` exactly.
///
/// `<F>` in the template stands for the canonical script path. The same bytes must also match
/// what a locally installed `php` prints for the identical script, when one is available.
fn assert_report(prefix: &str, source: &str, expected_template: &str) {
    let (reported, code, dir) = run_uncaught(prefix, source);
    let expected = expected_template.replace("<F>", &canonical_script(&dir, &format!("{prefix}.php")));

    assert_eq!(reported, expected, "report differs from reference PHP 8.5 text");
    assert_eq!(code, Some(255), "PHP exits 255 for an uncaught exception");
    if let Some(php) = php_reference(&dir, prefix) {
        assert_eq!(reported, php, "report differs from the local php binary");
    }
    let _ = fs::remove_dir_all(&dir);
}

/// A built-in exception subclass reports its own class name, its message and its location.
#[test]
fn uncaught_builtin_subclass_reports_class_and_message() {
    assert_report(
        "elephc_uncaught_builtin",
        "<?php\nthrow new RuntimeException(\"boom detail\");\n",
        "\nFatal error: Uncaught RuntimeException: boom detail in <F>:2\nStack trace:\n#0 {main}\n  thrown in <F> on line 2\n",
    );
}

/// A USER-DECLARED subclass is named from the runtime class table, not a compile-time literal.
#[test]
fn uncaught_user_subclass_reports_its_own_name() {
    assert_report(
        "elephc_uncaught_user",
        "<?php\nclass MyErr extends Exception {}\nthrow new MyErr(\"custom text\");\n",
        "\nFatal error: Uncaught MyErr: custom text in <F>:3\nStack trace:\n#0 {main}\n  thrown in <F> on line 3\n",
    );
}

/// An EMPTY message drops the `": "` separator but KEEPS the location, exactly as PHP does.
///
/// This is the case a naive implementation gets wrong while still looking correct everywhere
/// else, because writing the separator unconditionally is invisible whenever a message follows.
/// It is also where the location is easiest to lose: the empty-message branch skips forward, and
/// skipping one label too far would drop the suffix along with the separator.
#[test]
fn uncaught_empty_message_omits_the_separator() {
    assert_report(
        "elephc_uncaught_empty",
        "<?php\nthrow new Exception(\"\");\n",
        "\nFatal error: Uncaught Exception in <F>:2\nStack trace:\n#0 {main}\n  thrown in <F> on line 2\n",
    );
}

/// An uncaught exception thrown inside a function called from the top level lists that call.
#[test]
fn uncaught_inside_a_function_called_from_top_level() {
    assert_report(
        "elephc_uncaught_toplevel_call",
        "<?php\nfunction f() {\n    throw new DomainException(\"from f\");\n}\necho \"before\\n\";\nf();\n",
        "before\n\nFatal error: Uncaught DomainException: from f in <F>:3\nStack trace:\n#0 <F>(6): f()\n#1 {main}\n  thrown in <F> on line 3\n",
    );
}

/// Nested calls print innermost first, with the call-site line and the formatted arguments.
#[test]
fn uncaught_nested_function_trace_formats_arguments() {
    assert_report(
        "elephc_uncaught_nested",
        "<?php\nfunction inner($x) {\n    throw new RuntimeException(\"deep\");\n}\nfunction outer($a, $s) {\n    inner($a + 1);\n}\nouter(1, \"hi\");\n",
        "\nFatal error: Uncaught RuntimeException: deep in <F>:3\nStack trace:\n#0 <F>(6): inner(2)\n#1 <F>(8): outer(1, 'hi')\n#2 {main}\n  thrown in <F> on line 3\n",
    );
}

/// Instance methods print as `Class->method(...)` and static ones as `Class::method(...)`.
#[test]
fn uncaught_method_trace_uses_class_and_arrow() {
    assert_report(
        "elephc_uncaught_method",
        "<?php\nclass K {\n    function m($v) {\n        throw new LogicException(\"in method\");\n    }\n    static function s() {\n        (new K)->m(null);\n    }\n}\nK::s();\n",
        "\nFatal error: Uncaught LogicException: in method in <F>:4\nStack trace:\n#0 <F>(7): K->m(NULL)\n#1 <F>(10): K::s()\n#2 {main}\n  thrown in <F> on line 4\n",
    );
}

/// A previous exception prints FIRST, the thrown one follows a `Next` separator, and a single
/// `thrown in` line names the outermost exception.
#[test]
fn uncaught_chain_prints_previous_first_with_next_separator() {
    assert_report(
        "elephc_uncaught_chain",
        "<?php\nfunction boom() {\n    try {\n        throw new LogicException(\"root\");\n    } catch (Exception $e) {\n        throw new RuntimeException(\"boom\", 3, $e);\n    }\n}\nboom();\n",
        "\nFatal error: Uncaught LogicException: root in <F>:4\nStack trace:\n#0 <F>(9): boom()\n#1 {main}\n\nNext RuntimeException: boom in <F>:6\nStack trace:\n#0 <F>(9): boom()\n#1 {main}\n  thrown in <F> on line 6\n",
    );
}

/// An exception thrown inside a closure names the closure with PHP's `{closure:file:line}` label.
#[test]
fn uncaught_inside_a_closure_names_the_closure() {
    assert_report(
        "elephc_uncaught_closure",
        "<?php\n$f = function (int $n) {\n    throw new Exception(\"in closure\");\n};\n$f(5);\n",
        "\nFatal error: Uncaught Exception: in closure in <F>:3\nStack trace:\n#0 <F>(5): {closure:<F>:2}(5)\n#1 {main}\n  thrown in <F> on line 3\n",
    );
}

/// A closure declared inside a closure is labelled with its enclosing closure's label, as PHP 8.5
/// prints it (`{closure:{closure:file:2}:3}`), never with the compiler's internal symbol.
#[test]
fn uncaught_inside_a_nested_closure_names_both_closures() {
    assert_report(
        "elephc_uncaught_nested_closure",
        "<?php\n$outer = function () {\n    $inner = function () {\n        throw new Exception(\"nested\");\n    };\n    $inner();\n};\n$outer();\n",
        "\nFatal error: Uncaught Exception: nested in <F>:4\nStack trace:\n#0 <F>(6): {closure:{closure:<F>:2}:3}()\n#1 <F>(8): {closure:<F>:2}()\n#2 {main}\n  thrown in <F> on line 4\n",
    );
}

/// A `$previous` cycle closed by re-running the constructor prints each link once, oldest first,
/// and stops at the first repeat as PHP 8.5 does; the renderer used to recurse without bound.
#[test]
fn uncaught_cyclic_previous_chain_prints_each_link_once() {
    assert_report(
        "elephc_uncaught_previous_cycle",
        "<?php\nclass E extends Exception {\n    public function relink(?Throwable $p): void { parent::__construct(\"a\", 0, $p); }\n}\n$a = new E(\"a\");\n$b = new Exception(\"b\", 0, $a);\n$a->relink($b);\nthrow $b;\n",
        "\nFatal error: Uncaught E: a in <F>:5\nStack trace:\n#0 {main}\n\nNext Exception: b in <F>:6\nStack trace:\n#0 {main}\n  thrown in <F> on line 6\n",
    );
}

/// An exception whose previous link is itself prints once.
#[test]
fn uncaught_self_previous_link_prints_once() {
    assert_report(
        "elephc_uncaught_previous_self",
        "<?php\nclass E extends Exception {\n    public function relink(?Throwable $p): void { parent::__construct(\"self\", 0, $p); }\n}\n$c = new E(\"self\");\n$c->relink($c);\nthrow $c;\n",
        "\nFatal error: Uncaught E: self in <F>:5\nStack trace:\n#0 {main}\n  thrown in <F> on line 5\n",
    );
}

/// The reported line is where the exception was CONSTRUCTED, not where it was thrown.
///
/// This is the one test whose expected value would change if the line came from the `throw`
/// terminator instead of the `new`: the two sit on different lines, and in different functions.
/// Reference PHP 8.5.6 prints line 2 here.
#[test]
fn uncaught_reports_the_construction_site_not_the_throw_site() {
    let prefix = "elephc_uncaught_construction_site";
    let (reported, code, dir) = run_uncaught(
        prefix,
        "<?php\nfunction make() { return new LogicException(\"made here\"); }\n$e = make();\necho \"still running\\n\";\nthrow $e;\n",
    );
    let expected = format!(
        "\nFatal error: Uncaught LogicException: made here{}",
        location_suffix(&dir, &format!("{prefix}.php"), 2)
    );

    assert!(
        reported.contains(&expected),
        "the location must be the `new` on line 2, not the `throw` on line 5;\n  attendu (fragment): {expected:?}\n  got:             {reported:?}"
    );
    assert_eq!(code, Some(255));
    let _ = fs::remove_dir_all(&dir);
}

/// A Throwable raised inside an internal call is located at that call, as PHP does, and STILL
/// exits 255.
///
/// `intdiv($n, 0)` raises a `DivisionByZeroError` synthesized by a codegen guard, which stamps the
/// line of the instruction it guards. A module with no `throw` keeps no frame readers (they would
/// cost every program code size), so the guard writes its own short report without a
/// `Stack trace:` block. The uncaught paths must also agree on `$?`: the short codegen path once
/// exited `1` while `throw new ...` exited `255`.
#[test]
fn synthesized_error_is_located_at_the_call_and_still_exits_255() {
    let prefix = "elephc_uncaught_synthesized";
    let (reported, code, dir) = run_uncaught(
        prefix,
        "<?php\n$n = 1;\n$d = 0;\necho intdiv($n, $d);\n",
    );

    let file = canonical_script(&dir, &format!("{prefix}.php"));
    assert_eq!(
        reported,
        format!("\nFatal error: Uncaught DivisionByZeroError: Division by zero in {file}:4\n")
    );
    assert_eq!(
        code,
        Some(255),
        "both uncaught paths must leave the same exit status"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// A synthesized error escaping a user function of a module that throws reports that function's
/// frame, as PHP does: the codegen guard goes through the full report instead of its short form.
/// PHP also lists the internal `intdiv(1, 0)` frame as `#0`; internal calls have no activation
/// here, so that line is missing.
#[test]
fn synthesized_error_reports_the_user_frames_it_escaped() {
    let prefix = "elephc_uncaught_synthesized_frames";
    let (reported, code, dir) = run_uncaught(
        prefix,
        "<?php\nfunction f($a) { return intdiv(1, $a); }\nif ($argc > 5) { throw new Exception(\"x\"); }\nf(0);\n",
    );
    let file = canonical_script(&dir, &format!("{prefix}.php"));
    assert_eq!(
        reported,
        format!("\nFatal error: Uncaught DivisionByZeroError: Division by zero in {file}:2\nStack trace:\n#0 {file}(4): f(0)\n#1 {{main}}\n  thrown in {file} on line 2\n")
    );
    assert_eq!(code, Some(255));
    let _ = fs::remove_dir_all(&dir);
}

/// An exception that IS caught prints nothing and exits cleanly — the report is uncaught-only.
///
/// Verifies buffered output SURVIVES an uncaught exception, and precedes the report.
///
/// This helper used to write its message and exit without draining `ob_*` buffers, so
/// `ob_start(); echo "x"; throw …` printed NOTHING of the program's own output — the bytes were
/// simply dropped on the way out. PHP flushes the buffer first, then reports, so the program's
/// output comes first and the report follows.
///
/// The order is what this asserts, not merely the presence of both: reporting before flushing
/// would still show every byte, in the wrong sequence, and a `contains` check could not tell the
/// difference.
#[test]
fn buffered_output_survives_and_precedes_the_report() {
    let prefix = "elephc_uncaught_buffered";
    let (reported, code, dir) = run_uncaught(
        prefix,
        "<?php\nob_start();\necho \"BUFFERED\\n\";\nthrow new RuntimeException(\"boom\");\n",
    );

    assert_eq!(code, Some(255));
    let fatal = reported
        .find("\nFatal error: Uncaught RuntimeException: boom")
        .unwrap_or_else(|| panic!("no report in output: {reported:?}"));
    let buffered = reported
        .find("BUFFERED\n")
        .unwrap_or_else(|| panic!("buffered output was dropped: {reported:?}"));
    assert!(
        buffered < fatal,
        "buffered output must precede the report, as PHP orders them; got: {reported:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Without this the reporting path could fire on every throw and still pass the tests above.
#[test]
fn caught_exception_prints_no_fatal_report() {
    let dir = make_test_dir("elephc_uncaught_caught");
    let bin = compile(
        &dir,
        "<?php\ntry {\n    throw new RuntimeException(\"handled\");\n} catch (RuntimeException $e) {\n    echo \"caught:\", $e->getMessage(), \"\\n\";\n}\n",
        "elephc_uncaught_caught",
    );
    let output = Command::new(&bin).output().expect("failed to run compiled binary");

    assert!(output.status.success(), "a caught exception must exit cleanly");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "caught:handled\n",
        "the catch body must run normally"
    );
    // BOTH streams: the report moved to stdout, so a stderr-only check could no longer fail and
    // would have kept passing however broken the reporting became.
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("Fatal error")
            && !String::from_utf8_lossy(&output.stdout).contains("Fatal error"),
        "a caught exception must print no fatal report"
    );
    let _ = fs::remove_dir_all(&dir);
}
