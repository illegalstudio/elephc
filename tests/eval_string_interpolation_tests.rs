//! Purpose:
//! End-to-end tests for double-quoted string interpolation inside RUNTIME-INTERPRETED
//! `eval()` fragments.
//!
//! elephc has two `eval()` implementations. A literal `eval('...')` is lowered ahead of
//! time by the main compiler (`src/eval_aot.rs`), which uses the main lexer and has always
//! interpolated correctly. Anything the compiler cannot see through — a fragment built at
//! runtime, or one using a construct the AOT path declines — is executed by the
//! `elephc-magician` EvalIR interpreter, which has its OWN lexer. That lexer had no
//! interpolation scanner at all, so measured against PHP 8.5.6:
//!
//! ```text
//! eval(dyn('$i = 7; echo "a:$i:";'))   PHP -> a:7:      elephc -> a:$i:
//! ```
//!
//! It failed for EVERY type and every syntax form, and because `include`/`require`
//! executed from inside such a fragment runs the included file through the same lexer, it
//! silently removed interpolation from every transitively included file too.
//!
//! Called from:
//! - `cargo test --test eval_string_interpolation_tests` through Rust's test harness.
//!
//! Key details:
//! - EVERY expected string here was captured from reference PHP 8.5.6 with
//!   `php -d xdebug.mode=off` running the exact `source` literal in the same test.
//! - Fragments are built through `dyn()` (`substr('x' . $s, 1)`) SPECIFICALLY so the
//!   compiler cannot constant-fold them into the AOT path. A literal fragment would test
//!   the main lexer, which was never broken, and would prove nothing.
//! - `both_eval_paths_agree` is the load-bearing case: it runs one byte-identical fragment
//!   through BOTH implementations in one program and requires the two outputs to match, so
//!   the interpreter cannot drift away from the AOT path again.
//! - The refusal test matters as much as the positive ones. `"$a[]"` is a PHP parse error
//!   (`syntax error, unexpected token "]"`); a lexer that invents an empty-string key
//!   there would look "fixed" while accepting invalid PHP.
//! - These fixtures intentionally exercise runtime eval without optional regex support. The
//!   expected capability reminder is filtered exactly; every other compiler diagnostic remains
//!   visible to the assertions below.
//! - Host-target only; this is a pure host-side Rust lexer change with no emitted assembly.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Expected capability reminder for eval fixtures that intentionally omit regex support.
const EVAL_WITHOUT_REGEX_REMINDER: &str =
    "warning: dynamic eval was compiled without optional regex support";

/// The `dyn()` helper every fragment goes through to defeat AOT constant folding.
const DYN_HELPER: &str = "function dyn(string $s): string { return substr('x' . $s, 1); }";

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
/// anything elephc emitted. The intentional eval capability reminder is excluded exactly;
/// every other compiler diagnostic still surfaces.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            *line != EVAL_WITHOUT_REGEX_REMINDER
                && (line.starts_with("error")
                    || line.starts_with("Error")
                    || line.starts_with("warning")
                    || line.starts_with("Warning: ")
                    || line.starts_with("EIR backend error"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compiles `source` to a plain executable, asserting elephc reported no diagnostic.
fn compile(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let diagnostics = elephc_diagnostics(&stderr);
    // Report the RAW stderr when the filter matched nothing: a `native project error`
    // starts with none of the recognised prefixes, so filtering first made a hard
    // compile failure assert with an empty message and no way to tell what went wrong.
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        if diagnostics.is_empty() {
            stderr.trim()
        } else {
            &diagnostics
        }
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected elephc diagnostic:\n{diagnostics}"
    );
    dir.join(stem)
}

/// Runs a compiled executable in its own temp dir and returns its stdout.
fn run_binary_in(dir: &Path, bin: &Path) -> String {
    let output = Command::new(bin)
        .current_dir(dir)
        .output()
        .expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Runs a compiled executable that is expected to abort, returning `(stdout, stderr)`.
fn run_binary_expecting_failure(dir: &Path, bin: &Path) -> (String, String) {
    let output = Command::new(bin)
        .current_dir(dir)
        .output()
        .expect("failed to run compiled binary");
    assert!(
        !output.status.success(),
        "compiled binary unexpectedly succeeded:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Compiles `source`, runs it, and asserts stdout equals `expected`.
fn assert_program_output(prefix: &str, source: &str, expected: &str) {
    let dir = make_test_dir(prefix);
    let bin = compile(&dir, source, prefix);
    assert_eq!(run_binary_in(&dir, &bin), expected);
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies interpolation of every scalar type a runtime-interpreted fragment can hold.
///
/// PHP 8.5.6 reference output for this exact program:
/// `int:7|null:[]|true:[1]|false:[]|float:1.5|whole:1|str:S|mixed:pre-7-S-post|plain|`
///
/// The bool and null columns are the ones a naive "call a to-string helper" fix gets
/// wrong: PHP renders `true` as `1`, and both `false` and `null` as the empty string.
#[test]
fn runtime_interpreted_fragment_interpolates_every_scalar_type() {
    let source = format!(
        r#"<?php
{DYN_HELPER}
eval(dyn('
$i = 7; $n = null; $t = true; $f = false; $fl = 1.5; $z = 1.0; $s = "S";
echo "int:$i|", "null:[$n]|", "true:[$t]|", "false:[$f]|";
echo "float:$fl|", "whole:$z|", "str:$s|", "mixed:pre-$i-$s-post|", "plain|";
echo "\n";
'));
"#
    );
    assert_program_output(
        "elephc_interp_scalars",
        &source,
        "int:7|null:[]|true:[1]|false:[]|float:1.5|whole:1|str:S|mixed:pre-7-S-post|plain|\n",
    );
}

/// Verifies PHP's simple and complex interpolation syntax inside an interpreted fragment.
///
/// PHP 8.5.6 reference output: `7|7|ZERO|KV|KV|FIVE|NEG|KV|PV|MV|KV|`
///
/// `"$a[k]"` with an UNQUOTED key is the form that most often gets missed, and `"$a[-1]"`
/// pins that a negative simple offset lexes as one integer rather than a unary minus.
#[test]
fn runtime_interpreted_fragment_supports_simple_and_complex_syntax() {
    let source = format!(
        r#"<?php
class Box {{ public $p = "PV"; public function m() {{ return "MV"; }} }}
{DYN_HELPER}
eval(dyn('
$v = 7; $k = "k"; $a = [0 => "ZERO", "k" => "KV", 5 => "FIVE", -1 => "NEG"]; $o = new Box();
echo "{{$v}}|", "${{v}}|", "$a[0]|", "$a[k]|", "$a[$k]|", "$a[5]|", "$a[-1]|";
echo "{{$a[\'k\']}}|", "$o->p|", "{{$o->m()}}|", "{{$a[$k]}}|";
echo "\n";
'));
"#
    );
    assert_program_output(
        "elephc_interp_syntax",
        &source,
        "7|7|ZERO|KV|KV|FIVE|NEG|KV|PV|MV|KV|\n",
    );
}

/// Verifies the forms PHP does NOT interpolate stay literal text.
///
/// PHP 8.5.6 reference output:
/// `esc:$x|dbl:$X|brace:{ X }|escbrace:\{X}|bslash:\X|backref:$2-$1|trail:abc$|plain:plain|`
///
/// `backref:$2-$1` is the case an over-eager fix breaks: a PHP variable name cannot start
/// with a digit, so `"$2-$1"` is literal — and that is exactly how `preg_replace()`
/// back-references are written inside a double-quoted replacement string.
#[test]
fn runtime_interpreted_fragment_keeps_non_interpolating_forms_literal() {
    let source = format!(
        r#"<?php
{DYN_HELPER}
eval(dyn('
$x = "X";
echo "esc:\$x|", "dbl:$$x|", "brace:{{ $x }}|", "escbrace:\{{$x}}|", "bslash:\\\\$x|";
echo "backref:$2-$1|", "trail:abc$|", "plain:plain|";
echo "\n";
'));
"#
    );
    assert_program_output(
        "elephc_interp_literals",
        &source,
        "esc:$x|dbl:$X|brace:{ X }|escbrace:\\{X}|bslash:\\X|backref:$2-$1|trail:abc$|plain:plain|\n",
    );
}

/// Verifies a file included FROM a runtime-interpreted fragment still interpolates.
///
/// PHP 8.5.6 reference output: `included:[W]|W|AK|`
///
/// This is the blast radius the interpreter lexer had beyond `eval()` itself: an included
/// file is tokenized by the same lexer, so before the fix every double-quoted string in
/// every transitively included file lost its interpolation too.
#[test]
fn included_file_from_an_interpreted_fragment_still_interpolates() {
    let dir = make_test_dir("elephc_interp_include");
    fs::write(
        dir.join("inc_lib.php"),
        "<?php\n$w = \"W\";\necho \"included:[$w]|{$w}|$arr[k]|\";\n",
    )
    .unwrap();
    let source = format!(
        r#"<?php
{DYN_HELPER}
eval(dyn('
$arr = ["k" => "AK"];
include __DIR__ . "/inc_lib.php";
echo "\n";
'));
"#
    );
    let bin = compile(&dir, &source, "elephc_interp_include");
    assert_eq!(run_binary_in(&dir, &bin), "included:[W]|W|AK|\n");
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies the AOT and interpreted `eval()` paths produce identical output for ONE
/// byte-identical fragment.
///
/// The two paths use two different lexers, so this is the case that would catch a future
/// drift in either direction rather than only pinning today's strings. The literal
/// `eval('...')` is lowered ahead of time; `eval(dyn('...'))` cannot be, so it runs
/// through the interpreter.
///
/// PHP 8.5.6 reference output (both lines): `7|7|ZERO|KV|$v|$X|{ X }|pre-7-post|plain|`
#[test]
fn both_eval_paths_agree_on_one_identical_fragment() {
    let fragment = r#"$v = 7; $x = "X"; $a = [0 => "ZERO", "k" => "KV"];
echo "$v|{$v}|$a[0]|$a[k]|\$v|$$x|{ $x }|pre-$v-post|plain|";"#;
    let source = format!(
        "<?php\n{DYN_HELPER}\neval('{fragment}');\necho \"\\n\";\neval(dyn('{fragment}'));\necho \"\\n\";\n"
    );
    let expected = "7|7|ZERO|KV|$v|$X|{ X }|pre-7-post|plain|\n";
    assert_program_output(
        "elephc_interp_both_paths",
        &source,
        &format!("{expected}{expected}"),
    );
}

/// Verifies `"$a[]"` is REFUSED rather than silently read as an empty-string key.
///
/// PHP 8.5.6: `Parse error: syntax error, unexpected token "]", expecting "-" or
/// identifier or variable or number`. The program prints what precedes the eval and then
/// stops, exactly as PHP does.
///
/// The main compiler lexer accepts this construct and invents an empty key
/// (`src/lexer/literals/strings.rs`), so porting it verbatim would have imported a
/// measured over-acceptance; this test is what keeps the port stricter than its source.
#[test]
fn empty_simple_offset_is_refused_at_fragment_parse_time() {
    let dir = make_test_dir("elephc_interp_refusal");
    let source = format!(
        r#"<?php
{DYN_HELPER}
echo "before\n";
eval(dyn('$a = [1]; echo "$a[]";'));
echo "after\n";
"#
    );
    let bin = compile(&dir, &source, "elephc_interp_refusal");
    let (stdout, stderr) = run_binary_expecting_failure(&dir, &bin);
    assert_eq!(stdout, "before\n");
    assert!(
        stderr.contains("Parse error: syntax error, unexpected end of file"),
        "expected an eval parse-error refusal, got stderr:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}
