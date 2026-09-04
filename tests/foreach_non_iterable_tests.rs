//! Purpose:
//! End-to-end tests that `foreach` over a NON-ITERABLE value behaves like reference PHP:
//! it raises `foreach() argument must be of type array|object, <type> given` on stdout,
//! SKIPS the loop body, runs the statement after the loop, and exits 0.
//!
//! Called from:
//! - `cargo test --test foreach_non_iterable_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   compile a plain executable, run it, and assert stdout AND exit code AND stderr — the same
//!   harness style as `function_exists_tests` / `opcache_ini_tests`. Host-target only
//!   (macOS aarch64 local).
//! - REGRESSION ANCHOR: every one of these programs used to abort with exit code 70 and
//!   `Fatal error: foreach over iterable with unsupported kind` — a compiler-internal string,
//!   not a PHP diagnostic — with the statement after the loop never reached. The exit-code
//!   assertion in `run_binary` is therefore the load-bearing check, not just the stdout compare.
//!   The dynamic (`array|false`) form was the field repro: `foreach (opcache_get_configuration()
//!   ['directives'] as $k => $v)` under `--ini opcache.restrict_api=…`.
//! - Expected warning texts were captured from reference PHP 8.5.6 with
//!   `php -d xdebug.mode=off` (Xdebug overloads warning rendering). elephc does not synthesize
//!   the ` in <file> on line <n>` tail php-src appends, so assertions compare the message BODY.
//! - PHP names the offending VALUE, not its declared type: a bool prints `true`/`false`, never
//!   `bool`. `union_holding_true_names_the_value` and `static_false_and_true_name_the_value`
//!   pin that, since it is the one case a static type cannot answer on its own.
//! - The STATIC forms (`foreach (false as $x)`) also raise a COMPILE warning; php-src only warns
//!   at runtime, but the value is knowable at compile time so elephc reports both. Compile-stderr
//!   assertions filter through `elephc_diagnostics` because the system linker (GNU `ld` on Linux)
//!   emits warnings macOS does not.
//! - `union_holding_array_still_iterates` is the anti-regression for the fix: the runtime
//!   non-iterable branch must not disturb a Mixed/union value that really does hold an array.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// The message body elephc and php-src share, minus the `<type> given` tail.
const WARNING_PREFIX: &str = "Warning: foreach() argument must be of type array|object, ";

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

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than
/// anything elephc emitted: GNU `ld` reports the static-`getaddrinfo`/`gethostbyname` glibc
/// notes and the `.note.GNU-stack` deprecation, while Apple's linker stays silent. Anchoring
/// on elephc's own line starts isolates its diagnostics — and still surfaces an UNEXPECTED
/// elephc warning, which an allow-list of known messages would have hidden.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("Warning: ")
                || line.starts_with("warning:")
                || line.starts_with("warning[")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compiles `source` to a plain executable, returning its path and elephc's own compile
/// diagnostics.
fn compile(dir: &Path, source: &str, stem: &str) -> (PathBuf, String) {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    let raw_stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "elephc compile failed:\n{raw_stderr}"
    );
    (dir.join(stem), elephc_diagnostics(&raw_stderr))
}

/// Runs a compiled executable and returns `(stdout, stderr)`.
///
/// Asserts a clean exit FIRST: the defect this file anchors aborted the process with exit
/// code 70 (`EX_SOFTWARE`) from `__rt_iterable_unsupported_kind`, so the status assertion is
/// the load-bearing check. A 70 here means the fatal is back.
fn run_binary(bin: &Path) -> (String, String) {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    assert_eq!(
        output.status.code(),
        Some(0),
        "compiled binary exited non-zero (70 means the foreach fatal is back):\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Builds a probe that iterates `expr` and prints a marker before and after the loop.
///
/// The trailing `echo` is the point of the whole file: under the old fatal it never ran.
fn probe(expr: &str) -> String {
    format!(
        "<?php\necho \"before\\n\";\nforeach ({expr} as $k => $v) {{ echo \"body\\n\"; }}\necho \"after\\n\";\n"
    )
}

/// A probe whose source is a genuine `array|bool` union, so the kind is only known at RUNTIME.
///
/// The return hint is spelled out so the checker keeps the union instead of narrowing to the
/// branch actually taken; `$pick` is read from `count()` of a literal array so no constant
/// folding can collapse the call. `pick_len` selects the branch: 1 → array, 2 → `true`,
/// anything else → `false`.
fn union_probe(pick_len: usize) -> String {
    let items = vec!["0"; pick_len].join(", ");
    format!(
        "<?php\n\
         function source(int $n): array|bool {{ if ($n === 1) {{ return [10, 20]; }} if ($n === 2) {{ return true; }} return false; }}\n\
         $pick = count([{items}]);\n\
         $value = source($pick);\n\
         echo \"before\\n\";\n\
         foreach ($value as $k => $v) {{ echo \"body=\", $v, \"\\n\"; }}\n\
         echo \"after\\n\";\n"
    )
}

/// Splits php's diagnostics back out of a program's stdout, as `(program_output, messages)`.
///
/// php writes a diagnostic to STDOUT, so the separation these tests need is textual rather than a
/// file descriptor: a line whose kind prefix names a php diagnostic is one, and the blank line php
/// opens it with belongs to it. The trailing ` in <file> on line <n>` is dropped, because elephc
/// publishes it at some warning sites and not others — `resource given` carries it, `null given`
/// does not — and which stream a line belongs to must not depend on that.
fn split_diagnostics(stdout: &str) -> (String, String) {
    const KINDS: &[&str] = &[
        "Warning: ",
        "Notice: ",
        "Deprecated: ",
        "Fatal error: ",
        "Parse error: ",
    ];
    let mut program = String::with_capacity(stdout.len());
    let mut messages = String::new();
    for line in stdout.split_inclusive('\n') {
        let body = line.strip_suffix('\n').unwrap_or(line);
        if !KINDS.iter().any(|kind| body.starts_with(kind)) {
            program.push_str(line);
            continue;
        }
        // Exactly one newline, and without asking what precedes it: the program's own output and
        // php's opening blank line share a line whenever the program did not end its last write
        // with a newline.
        if program.ends_with('\n') {
            program.truncate(program.len() - 1);
        }
        let message = match body.find(" in ") {
            Some(cut) if body[cut..].contains(" on line ") => &body[..cut],
            _ => body,
        };
        messages.push_str(message);
        messages.push('\n');
    }
    (program, messages)
}

/// Asserts the loop body was skipped, the following statement ran, and the warning names `type`.
fn assert_skipped_with_warning(bin: &Path, type_name: &str) {
    let (stdout, stderr) = run_binary(bin);
    let (program, messages) = split_diagnostics(&stdout);
    assert_eq!(program, "before\nafter\n", "loop body must not run");
    assert_eq!(
        messages,
        format!("{WARNING_PREFIX}{type_name} given\n"),
        "runtime warning must match reference PHP's message body"
    );
    assert_eq!(stderr, "", "php writes this warning to stdout, not stderr");
}

/// Reference (`php -d xdebug.mode=off`, PHP 8.5.6):
/// ```text
/// before
/// Warning: foreach() argument must be of type array|object, false given in …:3
/// after
/// ```
/// exit 0. elephc aborted with exit 70 and `Fatal error: foreach over iterable with
/// unsupported kind` before the fix.
#[test]
fn static_false_warns_and_continues() {
    let dir = make_test_dir("foreach_non_iterable_false");
    let (bin, diagnostics) = compile(&dir, &probe("false"), "false_probe");
    assert!(
        diagnostics.contains("foreach() argument must be of type array|object, false given"),
        "the statically known value should also be reported at compile time: {diagnostics}"
    );
    assert_skipped_with_warning(&bin, "false");
}

/// PHP names the VALUE for booleans, so `true` and `false` produce DIFFERENT messages even
/// though both are `bool`. Reference: `…, true given` / `…, false given`.
#[test]
fn static_false_and_true_name_the_value() {
    let dir = make_test_dir("foreach_non_iterable_bools");
    let (false_bin, _) = compile(&dir, &probe("false"), "bool_false_probe");
    assert_skipped_with_warning(&false_bin, "false");
    let (true_bin, _) = compile(&dir, &probe("true"), "bool_true_probe");
    assert_skipped_with_warning(&true_bin, "true");
}

/// Reference: `Warning: foreach() argument must be of type array|object, null given`, exit 0.
#[test]
fn static_null_warns_and_continues() {
    let dir = make_test_dir("foreach_non_iterable_null");
    let (bin, diagnostics) = compile(&dir, &probe("null"), "null_probe");
    assert!(
        diagnostics.contains("foreach() argument must be of type array|object, null given"),
        "compile diagnostics: {diagnostics}"
    );
    assert_skipped_with_warning(&bin, "null");
}

/// Reference: `Warning: foreach() argument must be of type array|object, int given`, exit 0.
#[test]
fn static_int_warns_and_continues() {
    let dir = make_test_dir("foreach_non_iterable_int");
    let (bin, diagnostics) = compile(&dir, &probe("42"), "int_probe");
    assert!(
        diagnostics.contains("foreach() argument must be of type array|object, int given"),
        "compile diagnostics: {diagnostics}"
    );
    assert_skipped_with_warning(&bin, "int");
}

/// Reference: `Warning: foreach() argument must be of type array|object, string given`, exit 0.
///
/// PHP reports `string` for a non-empty string as well as `""` — the type, never the contents.
#[test]
fn static_string_warns_and_continues() {
    let dir = make_test_dir("foreach_non_iterable_string");
    let (bin, diagnostics) = compile(&dir, &probe("\"str\""), "string_probe");
    assert!(
        diagnostics.contains("foreach() argument must be of type array|object, string given"),
        "compile diagnostics: {diagnostics}"
    );
    assert_skipped_with_warning(&bin, "string");
}

/// Reference: `Warning: foreach() argument must be of type array|object, float given`, exit 0.
#[test]
fn static_float_warns_and_continues() {
    let dir = make_test_dir("foreach_non_iterable_float");
    let (bin, _) = compile(&dir, &probe("1.5"), "float_probe");
    assert_skipped_with_warning(&bin, "float");
}

/// Reference: `Warning: foreach() argument must be of type array|object, resource given`, exit 0.
#[test]
fn resource_warns_and_continues() {
    let dir = make_test_dir("foreach_non_iterable_resource");
    let source = "<?php\n\
                  $handle = fopen(\"php://memory\", \"r+\");\n\
                  echo \"before\\n\";\n\
                  foreach ($handle as $k => $v) { echo \"body\\n\"; }\n\
                  echo \"after\\n\";\n";
    let (bin, _) = compile(&dir, source, "resource_probe");
    assert_skipped_with_warning(&bin, "resource");
}

/// ANTI-REGRESSION for the fix: a `array|false` value that really holds an array must still
/// iterate normally. The runtime non-iterable branch sits on the same `__rt_mixed_unbox`
/// dispatch as the array path, so a mistake there would silently turn every dynamic foreach
/// into a zero-iteration loop.
///
/// Reference: `before` / `body=10` / `body=20` / `after` on stdout, empty stderr, exit 0.
#[test]
fn union_holding_array_still_iterates() {
    let dir = make_test_dir("foreach_non_iterable_union_array");
    let (bin, _) = compile(&dir, &union_probe(1), "union_array_probe");
    let (stdout, stderr) = run_binary(&bin);
    let (program, messages) = split_diagnostics(&stdout);
    assert_eq!(program, "before\nbody=10\nbody=20\nafter\n");
    assert_eq!(messages, "", "a real array must not warn");
    assert_eq!(stderr, "");
}

/// The field repro's shape: a `array|false` value holding `false`, where the kind is known
/// only at runtime. Reference: warning + `after`, exit 0.
#[test]
fn union_holding_false_warns_and_continues() {
    let dir = make_test_dir("foreach_non_iterable_union_false");
    let (bin, diagnostics) = compile(&dir, &union_probe(0), "union_false_probe");
    assert_eq!(
        diagnostics, "",
        "a union source is only diagnosable at runtime, so the compile must stay quiet"
    );
    assert_skipped_with_warning(&bin, "false");
}

/// The runtime path must read the PAYLOAD, not just the tag: the same `bool` tag has to print
/// `true` here and `false` in `union_holding_false_warns_and_continues`.
#[test]
fn union_holding_true_names_the_value() {
    let dir = make_test_dir("foreach_non_iterable_union_true");
    let (bin, _) = compile(&dir, &union_probe(2), "union_true_probe");
    assert_skipped_with_warning(&bin, "true");
}

/// The whole point of warning instead of aborting: everything AFTER the loop still runs, and
/// the process exits 0. Two non-iterable loops in a row prove the state is not left poisoned.
#[test]
fn statements_after_the_loop_still_run() {
    let dir = make_test_dir("foreach_non_iterable_sequence");
    let source = "<?php\n\
                  function source(int $n): array|false { if ($n === 1) { return [7]; } return false; }\n\
                  $missing = source(count([]));\n\
                  foreach ($missing as $v) { echo \"never\\n\"; }\n\
                  echo \"one\\n\";\n\
                  foreach (42 as $v) { echo \"never\\n\"; }\n\
                  echo \"two\\n\";\n\
                  foreach (source(count([0])) as $v) { echo \"three=\", $v, \"\\n\"; }\n\
                  echo \"four\\n\";\n";
    let (bin, _) = compile(&dir, source, "sequence_probe");
    let (stdout, stderr) = run_binary(&bin);
    let (program, messages) = split_diagnostics(&stdout);
    assert_eq!(program, "one\ntwo\nthree=7\nfour\n");
    assert_eq!(
        messages,
        format!("{WARNING_PREFIX}false given\n{WARNING_PREFIX}int given\n")
    );
    assert_eq!(stderr, "", "php writes these warnings to stdout, not stderr");
}

/// The original field repro, reduced: `opcache_get_configuration()` returns `false` under
/// `restrict_api`, indexing `false` yields null, and the `foreach` over that null used to
/// abort with exit 70 so `2 ok` never printed.
///
/// Reference (`php -d xdebug.mode=off -d opcache.restrict_api=/nowhere`) prints the
/// restrict-api warning, `Trying to access array offset on false`, then
/// `foreach() argument must be of type array|object, null given`, then `2 ok`, exit 0.
/// elephc does not yet emit the array-offset-on-false warning, so only the foreach warning
/// and the exit code are asserted here.
#[test]
fn opcache_restricted_configuration_repro_exits_zero() {
    let dir = make_test_dir("foreach_non_iterable_opcache");
    let source = "<?php\n\
                  $c = opcache_get_configuration();\n\
                  foreach ($c['directives'] as $k => $v) { echo \"never\\n\"; }\n\
                  echo \"2 ok\\n\";\n";
    let php = dir.join("opcache_probe.php");
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(&dir);
    cmd.arg("--ini").arg("opcache.restrict_api=/nowhere").arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let (stdout, stderr) = run_binary(&dir.join("opcache_probe"));
    let (program, messages) = split_diagnostics(&stdout);
    assert_eq!(program, "2 ok\n", "the statement after the loop must run");
    assert!(
        messages.contains(&format!("{WARNING_PREFIX}null given")),
        "expected the foreach warning, got: {messages}"
    );
    assert_eq!(stderr, "", "php writes this warning to stdout, not stderr");
}
