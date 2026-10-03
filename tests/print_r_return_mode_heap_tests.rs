//! Purpose:
//! End-to-end heap-ownership tests for `print_r`'s two capture-buffer modes, both of which
//! leaked exactly one heap block per call.
//!
//! `print_r` renders into the static `_print_r_buf` capture buffer and `__rt_pr_finish` copies
//! those bytes out through `__rt_str_persist`, so EVERY capture mode hands back a freshly
//! allocated heap block. Two independent defects dropped that block on the floor:
//!
//! - BUG A — `print_r($v, true)` (literal-flag return mode). `RuntimeFnId::result_ownership`
//!   left `PrintR` in the default `MayAliasArguments` bucket, so
//!   `codegen::lower_inst::ownership::value_is_scratch_string` classified the returned string as
//!   transient CONCAT SCRATCH and skipped its `release` entirely. The EIR was already correct —
//!   it emits `acquire`/`release` around the call — the backend just refused to lower the
//!   release. `PrintR` is now declared `Fresh`, which is what it always was: the rendered text
//!   is a copy and can never alias an argument. The fix REMOVES a suppression rather than adding
//!   a free, so it cannot introduce the double free that a hand-added release would risk.
//! - BUG B — `print_r($v, $runtimeFlag)`. The return branch boxes the capture string with
//!   `__rt_mixed_from_value`, whose string arm PERSISTS (copies) the payload instead of adopting
//!   it. The call site's EIR `release` targets the resulting Mixed cell, so nothing anywhere
//!   freed the intermediate capture string. `lower_print_r_runtime_flag` now frees it once the
//!   box owns its copy.
//!
//! Called from:
//! - `cargo test --test print_r_return_mode_heap_tests` through Rust's test harness.
//!
//! Key details:
//! - Harness style mirrors `tests/null_coalesce_merge_tests.rs`: the elephc CLI
//!   (`CARGO_BIN_EXE_elephc`) is invoked as a subprocess in an isolated temp dir, compiled to a
//!   plain executable, run, and its output asserted. Host-target only.
//! - `--heap-debug` is the authoritative instrument; `--gc-stats` under-reports and is never
//!   used here. Its summary goes to STDERR, the program's own output to stdout.
//! - Every test asserts the RENDERED TEXT as well as the leak summary. A leak fix that freed the
//!   wrong block would still report `clean` while corrupting the returned string, so the content
//!   assertion is what distinguishes "freed the intermediate" from "freed the live result".
//!   All expected text and lengths come from reference PHP 8.5.6 (`php -d xdebug.mode=off`).
//! - NEGATIVE CONTROLS: `print_r_echo_mode_loop_was_already_clean` and
//!   `var_export_return_mode_loop_was_already_clean` pin shapes that were ALREADY clean before
//!   the fix, so the suite distinguishes "fixed" from "never broken". `var_export($v, true)` is
//!   clean because the name resolver reroutes the literal-flag form to the prelude PHP helper
//!   `__elephc_var_export_str`, which returns through the ordinary user-function path.
//! - Compile-failure assertions filter stderr through `elephc_diagnostics` because the system
//!   linker (GNU `ld` on Linux) emits warnings macOS does not.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// How many times each probe calls the builtin under test.
///
/// The leak was one block per call, so a multi-iteration loop separates a genuine per-call leak
/// from a single live value the program still owns at exit.
const ITERATIONS: usize = 8;

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
/// anything elephc emitted: GNU `ld` on Linux reports the static-`getaddrinfo` glibc notes and
/// the `.note.GNU-stack` deprecation, while Apple's linker stays silent. elephc's own lines
/// start with `error`/`warning`, so anchoring on those prefixes isolates its diagnostics.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("error")
                || line.starts_with("Error")
                || line.starts_with("warning")
                || line.starts_with("Warning: ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compiles `source` to a heap-instrumented executable, asserting elephc reported no diagnostic.
fn compile_with_heap_debug(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.arg("build");
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&php);
    cmd.arg("--heap-debug");
    cmd.arg("-q");
    let output = cmd.output().expect("failed to spawn elephc");
    let diagnostics = elephc_diagnostics(&String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "elephc compile failed:\n{diagnostics}"
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected elephc diagnostic:\n{diagnostics}"
    );
    dir.join(stem)
}

/// Runs a heap-instrumented binary and returns its stdout plus the `--heap-debug` leak summary.
///
/// The summary is the last `HEAP DEBUG: leak summary:` line on stderr; the runtime prints
/// `clean` when nothing is live and `live_blocks=N live_bytes=M` otherwise.
fn run_with_heap_debug(bin: &Path) -> (String, String) {
    let output = Command::new(bin)
        .output()
        .expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let summary = stderr
        .lines()
        .find(|line| line.contains("leak summary:"))
        .unwrap_or_else(|| {
            panic!("no `HEAP DEBUG: leak summary:` line on stderr; got:\n{stderr}")
        })
        .trim()
        .to_owned();
    (String::from_utf8_lossy(&output.stdout).into_owned(), summary)
}

/// Compiles and runs `source`, asserting its stdout and that the heap ends with nothing live.
///
/// Asserting the stdout alongside the leak summary is deliberate: freeing the WRONG block would
/// also report `clean`, so only the rendered text proves the surviving string is intact.
fn assert_output_and_no_leak(prefix: &str, source: &str, expected_stdout: &str) {
    let dir = make_test_dir(prefix);
    let bin = compile_with_heap_debug(&dir, source, prefix);
    let (stdout, summary) = run_with_heap_debug(&bin);
    assert_eq!(stdout, expected_stdout, "rendered output changed");
    assert!(
        summary.ends_with("clean"),
        "expected a clean heap after {ITERATIONS} calls, got: {summary}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// BUG A regression: `print_r($v, true)` in a loop must free every rendered string.
///
/// Before the fix this reported `live_blocks=8` — one leaked `__rt_pr_finish` block per call,
/// each sized `next_pow2(strlen) + 16`. Reference PHP 8.5.6 renders 49 bytes for this array.
#[test]
fn print_r_return_mode_loop_frees_every_rendered_string() {
    let source = format!(
        r#"<?php
$a = [1, 2, 3];
$total = 0;
for ($i = 0; $i < {ITERATIONS}; $i++) {{
    $s = print_r($a, true);
    $total += strlen($s);
}}
echo $total, "\n";
echo $s;
"#
    );
    let expected = format!(
        "{}\nArray\n(\n    [0] => 1\n    [1] => 2\n    [2] => 3\n)\n",
        49 * ITERATIONS
    );
    assert_output_and_no_leak("print_r_return_loop", &source, &expected);
}

/// BUG A regression: a DISCARDED `print_r($v, true)` still owns and must free its string.
///
/// A result nobody stores takes a different ownership path from an assigned one — there is no
/// destination slot whose reassignment could mask the leak — so it is pinned separately.
#[test]
fn print_r_return_mode_discarded_result_frees_its_string() {
    let source = format!(
        r#"<?php
$a = ["x" => 1, "y" => 2];
for ($i = 0; $i < {ITERATIONS}; $i++) {{
    print_r($a, true);
}}
echo "done\n";
"#
    );
    assert_output_and_no_leak("print_r_return_discard", &source, "done\n");
}

/// BUG A regression: a nested array, whose larger rendering makes the leak size obvious.
///
/// Reference PHP 8.5.6 renders 116 bytes for `[1, [2, 3], 4]`, so the leaked block was 144 bytes
/// (`next_pow2(116) + 16`) rather than the 80 the flat array produced — evidence the leaked block
/// was the RESULT string and not a fixed-size intermediate.
#[test]
fn print_r_return_mode_nested_array_frees_every_rendered_string() {
    let source = format!(
        r#"<?php
$a = [1, [2, 3], 4];
$total = 0;
for ($i = 0; $i < {ITERATIONS}; $i++) {{
    $total += strlen(print_r($a, true));
}}
echo $total, "\n";
"#
    );
    let expected = format!("{}\n", 116 * ITERATIONS);
    assert_output_and_no_leak("print_r_return_nested", &source, &expected);
}

/// BUG B regression: a RUNTIME `$return` flag must free the intermediate capture string.
///
/// This mode boxes the capture string into a Mixed cell with `__rt_mixed_from_value`, which
/// COPIES the payload. The EIR release frees the box, never the intermediate, so before the fix
/// this leaked one `next_pow2(strlen) + 16` block per call exactly like BUG A. The flag is read
/// from a variable so the literal-flag lowering cannot be selected.
#[test]
fn print_r_runtime_flag_mode_frees_the_intermediate_capture_string() {
    let source = format!(
        r#"<?php
$a = [1, 2, 3];
$flag = true;
for ($i = 0; $i < {ITERATIONS}; $i++) {{
    $s = print_r($a, $flag);
}}
echo "done\n";
"#
    );
    assert_output_and_no_leak("print_r_runtime_flag", &source, "done\n");
}

/// NEGATIVE CONTROL: `print_r($v)` echo mode was ALREADY clean before the fix.
///
/// Echo mode returns PHP's `true` and never calls `__rt_pr_finish`, so it allocates no capture
/// string. Pinning it keeps the suite honest about which shapes the fix actually changed.
#[test]
fn print_r_echo_mode_loop_was_already_clean() {
    let source = format!(
        r#"<?php
$a = [1, 2, 3];
for ($i = 0; $i < {ITERATIONS}; $i++) {{
    print_r($a);
}}
"#
    );
    let one = "Array\n(\n    [0] => 1\n    [1] => 2\n    [2] => 3\n)\n";
    let expected = one.repeat(ITERATIONS);
    assert_output_and_no_leak("print_r_echo_loop", &source, &expected);
}

/// NEGATIVE CONTROL: `var_export($v, true)` was ALREADY clean before the fix.
///
/// The name resolver rewrites the literal-flag form to the prelude helper
/// `__elephc_var_export_str` (`src/name_resolver/expressions.rs`), so it returns through the
/// ordinary user-function path and never touches the `print_r` capture buffer. Reference PHP
/// 8.5.6 renders 39 bytes for this array.
#[test]
fn var_export_return_mode_loop_was_already_clean() {
    let source = format!(
        r#"<?php
$a = [1, 2, 3];
$total = 0;
for ($i = 0; $i < {ITERATIONS}; $i++) {{
    $total += strlen(var_export($a, true));
}}
echo $total, "\n";
"#
    );
    let expected = format!("{}\n", 39 * ITERATIONS);
    assert_output_and_no_leak("var_export_return_loop", &source, &expected);
}

/// OBJECT COVERAGE for BUG A's fix: `print_r($object, true)` in a loop.
///
/// `print_r` of an object only became possible at all with the C5 fix — it used to be the hard
/// compile error `unsupported EIR backend feature: print_r for PHP type Object("…")` — so this
/// is the first shape that exercises `__rt_print_r_object` through the CAPTURE buffer rather
/// than stdout. Nesting an object and an array inside the object makes the walk recurse through
/// `__rt_print_r_value` in both directions, and the loop separates a per-call capture-string
/// leak from the one live result the program still owns at exit.
///
/// Reference PHP 8.4.20 renders 224 bytes for this object.
#[test]
fn print_r_object_return_mode_loop_frees_every_rendered_string() {
    let source = format!(
        r#"<?php
class Foo {{ public $a = 1; public $b = 'xyz'; public $arr = [1, 2]; }}
class Bar {{ public ?Foo $o = null; function __construct() {{ $this->o = new Foo; }} }}
$total = 0;
for ($i = 0; $i < {ITERATIONS}; $i++) {{
    $s = print_r(new Bar, true);
    $total += strlen($s);
}}
echo $total, "\n";
echo $s;
"#
    );
    let expected = format!(
        concat!(
            "{}\n",
            "Bar Object\n",
            "(\n",
            "    [o] => Foo Object\n",
            "        (\n",
            "            [a] => 1\n",
            "            [b] => xyz\n",
            "            [arr] => Array\n",
            "                (\n",
            "                    [0] => 1\n",
            "                    [1] => 2\n",
            "                )\n",
            "\n",
            "        )\n",
            "\n",
            ")\n",
        ),
        224 * ITERATIONS
    );
    assert_output_and_no_leak("print_r_object_return_loop", &source, &expected);
}
