//! Purpose:
//! End-to-end tests for `preg_grep()`, `preg_last_error()`, `preg_last_error_msg()` and
//! the `PREG_*` / `PCRE_VERSION*` constants.
//!
//! Called from:
//! - `cargo test --test codegen_tests_ext` through the PCRE test module.
//!
//! Key details:
//! - Expected output is PHP 8.5's, except `PCRE_VERSION*`, which report the managed
//!   PCRE2 package (10.47) rather than the PHP build's library.
//! - Malformed UTF-8 under `/u` is the portable way to provoke a match error.
//! - The eval fixture builds its source at runtime so it runs through Magician.

use crate::support::*;

/// The error and grep constants have PHP's values; the version constants name PCRE2 10.47.
#[test]
fn test_preg_error_constants() {
    let out = compile_and_run(r#"<?php
echo PREG_NO_ERROR, PREG_INTERNAL_ERROR, PREG_BACKTRACK_LIMIT_ERROR, PREG_RECURSION_LIMIT_ERROR,
    PREG_BAD_UTF8_ERROR, PREG_BAD_UTF8_OFFSET_ERROR, PREG_JIT_STACKLIMIT_ERROR, PREG_GREP_INVERT, "\n";
echo PCRE_VERSION, '|', PCRE_VERSION_MAJOR, '.', PCRE_VERSION_MINOR, "\n";
"#);
    assert_eq!(out, "01234561\n10.47 2025-10-21|10.47\n");
}

/// Every call resets the error state; invalid patterns and malformed UTF-8 set it.
#[test]
fn test_preg_last_error_tracks_each_call() {
    let out = compile_and_run(r#"<?php
function report(): void { echo preg_last_error(), ':', preg_last_error_msg(), "\n"; }
report();
var_dump(@preg_match('/(/', 'a'));
report();
var_dump(preg_match('/a/', 'a'));
report();
var_dump(preg_match('/./u', "\xff"));
report();
var_dump(preg_match('/./u', "\xff", $m), $m);
var_dump(preg_match_all('/./u', "a\xff"));
report();
var_dump(preg_split('/,/u', "a,\xff"));
report();
var_dump(preg_replace('/a/u', 'b', "a\xff"));
report();
var_dump(preg_replace_callback('/a/u', fn($m) => 'b', "a\xff"));
report();
var_dump(preg_replace('/a/', 'b', 'a'));
report();
var_dump(@preg_split('/(/', 'a'));
report();
"#);
    assert_eq!(
        out,
        "0:No error\n\
         bool(false)\n1:Internal error\n\
         int(1)\n0:No error\n\
         bool(false)\n4:Malformed UTF-8 characters, possibly incorrectly encoded\n\
         bool(false)\narray(0) {\n}\n\
         bool(false)\n4:Malformed UTF-8 characters, possibly incorrectly encoded\n\
         bool(false)\n4:Malformed UTF-8 characters, possibly incorrectly encoded\n\
         NULL\n4:Malformed UTF-8 characters, possibly incorrectly encoded\n\
         NULL\n4:Malformed UTF-8 characters, possibly incorrectly encoded\n\
         string(1) \"b\"\n0:No error\n\
         bool(false)\n1:Internal error\n"
    );
}

/// `preg_grep()` keeps keys and original values, honours `PREG_GREP_INVERT`, and fails like PHP.
#[test]
fn test_preg_grep() {
    let out = compile_and_run(r#"<?php
var_dump(preg_grep('/^\d+$/', [1, '2', 3.5, true, 'x', 'k' => '42']));
var_dump(PREG_GREP('/a/', ['a', 'b', 'ca'], PREG_GREP_INVERT));
var_dump(preg_grep(pattern: '/b/', array: ['x' => 'abc', 'y' => 'z']));
var_dump(@preg_grep('/(/', []));
echo preg_last_error(), "\n";
var_dump(preg_grep('/a/u', ['x' => 'a', 5 => "\xff", 7 => 'a']));
echo preg_last_error(), "\n";
var_dump(@preg_grep('/a/e', ['a']));
echo preg_last_error(), "\n";
"#);
    assert_eq!(
        out,
        "array(4) {\n  [0]=>\n  int(1)\n  [1]=>\n  string(1) \"2\"\n  [3]=>\n  bool(true)\n  [\"k\"]=>\n  string(2) \"42\"\n}\n\
         array(1) {\n  [1]=>\n  string(1) \"b\"\n}\n\
         array(1) {\n  [\"x\"]=>\n  string(3) \"abc\"\n}\n\
         bool(false)\n1\n\
         array(1) {\n  [\"x\"]=>\n  string(1) \"a\"\n}\n4\n\
         bool(false)\n1\n"
    );
}

/// A backtrack-limit error after earlier matches releases the partial `preg_split()` pieces,
/// the partial replacement strings, the capture arrays and the partial `preg_grep()` result.
#[test]
fn test_preg_match_errors_heap_clean() {
    let out = compile_and_run_with_heap_debug(r#"<?php
$bad = 'x,y,z,' . str_repeat('a', 24) . 'b';
$pattern = '/,|(a+)+$/';
$failed = 0;
for ($i = 0; $i < 2; $i++) {
    $failed += (int) (preg_split($pattern, $bad) === false);
    $failed += (int) (preg_replace($pattern, '-', $bad) === null);
    $failed += (int) (preg_replace_callback($pattern, fn($m) => $m[0] . '!', $bad) === null);
    $failed += (int) (preg_match_all($pattern, $bad, $all) === false);
    $failed += (int) (preg_match('/(a+)+$/', $bad, $one) === false);
    $failed += count(preg_grep('/(a+)+$/', ['a', $bad, 'a']));
}
echo $failed, ':', preg_last_error(), ':', count($all), count($one), "\n";
"#);
    assert!(out.success, "{}", out.stderr);
    assert_eq!(out.stdout, "12:2:20\n");
    assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", out.stderr);
}

/// A namespaced `preg_grep()` (braced or statement-form namespace) does not suppress the
/// global one: unqualified global calls and `\preg_grep()` still reach PHP's function.
#[test]
fn test_preg_grep_namespaced_homonym_keeps_global_function() {
    let braced = compile_and_run(r#"<?php
namespace App { function preg_grep() { return 'app'; } }
namespace { echo json_encode(preg_grep('/a/', ['a', 'b'])), '|', \App\preg_grep(), "\n"; }
"#);
    assert_eq!(braced, "[\"a\"]|app\n");
    let statement = compile_and_run(r#"<?php
namespace App;
function preg_grep() { return 'app'; }
echo preg_grep(), '|', json_encode(\preg_grep('/a/', ['a', 'b'])), "\n";
"#);
    assert_eq!(statement, "app|[\"a\"]\n");
}

/// A destructor that runs while `$matches` is overwritten and fails its own nested match does
/// not change the outer `preg_match()` / `preg_match_all()` result, and the outer call's own
/// (clean) outcome is what `preg_last_error()` reports afterwards, like php-src.
#[test]
fn test_preg_match_result_survives_matches_destructor_errors() {
    let out = compile_and_run(r#"<?php
class Noisy { public function __destruct() { var_dump(preg_match('/./u', "\xff")); } }
$one = new Noisy();
var_dump(preg_match('/a/', 'a', $one), $one, preg_last_error());
$all = new Noisy();
var_dump(preg_match_all('/a/', 'aa', $all), count($all[0]), preg_last_error());
$none = new Noisy();
var_dump(preg_match('/b/', 'a', $none), $none, preg_last_error());
"#);
    assert_eq!(
        out,
        "bool(false)\nint(1)\narray(1) {\n  [0]=>\n  string(1) \"a\"\n}\nint(0)\n\
         bool(false)\nint(2)\nint(2)\nint(0)\n\
         bool(false)\nint(0)\narray(0) {\n}\nint(0)\n"
    );
}

/// `preg_grep()` matches each entry as PHP's string conversion, in compiled code and in eval:
/// an object runs its `__toString()` and keeps its object value, an object without one throws
/// PHP's conversion `Error`, an array warns and matches as `"Array"`, and scalars use their
/// string casts, with `PREG_GREP_INVERT` and keys preserved.
#[test]
fn test_preg_grep_matches_php_string_conversions() {
    let source = r#"class S { public $v; public function __construct($v) { $this->v = $v; } public function __toString(): string { echo "toString({$this->v})|"; return $this->v; } }
class P {}
function keys($found) { foreach ($found as $key => $value) { echo $key, ','; } echo "\n"; }
$in = ['k' => new S('a'), 3 => new S('b'), 'arr' => [1], 'i' => 1, 'f' => 1.5, 't' => true, 'n' => null, 'fa' => false, 's' => 'cat'];
keys(preg_grep('/a|1/', $in));
keys(preg_grep('/a|1/', $in, PREG_GREP_INVERT));
$r = preg_grep('/a/', [5 => new S('a')]);
echo get_class($r[5]), "\n";
try { preg_grep('/a/', ['x', new P()]); } catch (Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
echo json_encode(preg_grep('/^Array$/', [[1], 'Array', 'b'])), "\n";
"#;
    let expected = "toString(a)|toString(b)|k,arr,i,f,t,s,\n\
toString(a)|toString(b)|3,n,fa,\n\
toString(a)|S\n\
Error: Object of class P could not be converted to string\n\
[[1],\"Array\"]\n";
    let quoted = source.replace('\\', "\\\\").replace('\'', "\\'");
    for program in [
        format!("<?php\n{source}"),
        format!("<?php\neval('{quoted}' . ' // ' . $argc);\n"),
    ] {
        let out = compile_and_run_capture_with_regex(&program);
        assert!(out.success, "{}", out.stderr);
        assert_eq!(out.stdout, expected, "{program}");
        assert_eq!(out.stderr.matches("Array to string conversion").count(), 3, "{}", out.stderr);
    }
}

/// A `__toString()` that throws during `preg_grep()` releases the partial result and the
/// entry temporaries, so catching and retrying leaks nothing, in compiled code and in eval.
#[test]
fn test_preg_grep_throwing_to_string_heap_clean() {
    let source = r#"class T { public function __toString(): string { throw new Exception('no'); } }
class U { public function __toString(): string { return 'ua'; } }
$items = ['a', 'k' => 'ab', 2 => new U(), 3 => new T(), 4 => 'b'];
$caught = 0;
for ($i = 0; $i < 3; $i++) {
    try { preg_grep('/a/', $items); } catch (Exception $e) { $caught += $e->getMessage() === 'no' ? 1 : 0; }
}
$kept = preg_grep('/a/', ['a', 'b', 'ca', 1, new U()]);
echo $caught, ':', count($kept), "\n";
"#;
    let quoted = source.replace('\\', "\\\\").replace('\'', "\\'");
    for program in [
        format!("<?php\n{source}"),
        format!("<?php\neval('{quoted}' . ' // ' . $argc);\n"),
    ] {
        let out = compile_and_run_with_heap_debug_and_regex(&program);
        assert!(out.success, "{}", out.stderr);
        assert_eq!(out.stdout, "3:3\n", "{program}");
        assert!(out.stderr.contains("HEAP DEBUG: leak summary: clean"), "{program}\n{}", out.stderr);
    }
}

/// Wraps a fixture body in an `eval()` whose source is only known at runtime, so it runs
/// through Magician rather than being compiled inline.
fn opaque_eval(body: &str) -> String {
    let quoted = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php\neval('{quoted}' . ' // ' . $argc);\n")
}

/// Fixture: a nested `preg_*` error inside a `preg_replace_callback()` callback stays visible
/// through `preg_last_error()` but does not fail the outer call, also when the last match ends
/// at the subject's end, while the outer call's own match error still returns `null`.
const NESTED_CALLBACK_ERRORS: &str = r#"var_dump(preg_replace_callback('/a/', function ($m) { preg_match('/./u', "\xff"); return 'B'; }, 'xax'));
echo preg_last_error(), "\n";
var_dump(preg_replace_callback('/a/', function ($m) { return preg_replace('/x/u', 'y', "\xff") === null ? 'N' : 'S'; }, 'zaz'));
echo preg_last_error(), "\n";
var_dump(preg_replace_callback('/a/u', function ($m) { preg_match('/a/', 'a'); return 'B'; }, "a\xff"));
echo preg_last_error(), "\n";
var_dump(preg_replace_callback('/a$/', function ($m) { preg_match('/./u', "\xff"); return 'B'; }, 'ba'));
echo preg_last_error(), "\n";
"#;

/// Fixture: a positive `preg_split()` `$limit` stops matching once enough pieces are collected,
/// so a match error past the limit neither fails the call nor sets `preg_last_error()`; the
/// same split without room for the remainder still fails.
const SPLIT_LIMIT_ERRORS: &str = r#"$s = 'x,' . str_repeat('a', 24) . 'b';
echo json_encode(preg_split('/,|(a+)+$/', $s, 2)), preg_last_error(), "\n";
echo json_encode(preg_split('/,|(a+)+$/', $s, 2, PREG_SPLIT_DELIM_CAPTURE)), preg_last_error(), "\n";
var_dump(preg_split('/,|(a+)+$/', $s, 3));
echo preg_last_error(), "\n";
"#;

/// Fixture: `preg_grep()` checks that the pattern compiles without matching it against a probe
/// subject: a valid pattern that fails on `""` (`/a|(?R)/` loops on an empty subject) still
/// filters its entries, a match error stops at the failing entry, an empty array runs no match
/// and leaves no error, and only an invalid pattern returns `false`.
const GREP_PATTERN_CHECK: &str = r#"echo json_encode(preg_grep('/a|(?R)/', ['a'])), preg_last_error(), "\n";
echo json_encode(preg_grep('/a|(?R)/', ['x' => 'a', 'k' => 'ba', 'y' => 'a'])), preg_last_error() === PREG_NO_ERROR ? 'ok' : 'error', "\n";
echo json_encode(preg_grep('/a|(?R)/', [])), preg_last_error(), "\n";
echo json_encode(preg_grep('/a/', [])), preg_last_error(), "\n";
var_dump(preg_grep('/(/', []));
echo preg_last_error(), "\n";
"#;

/// Fixture: `preg_replace_callback()` interleaves its matches with the callbacks: a callback
/// sees the error state of the matches before it, not a later match's backtrack-limit error,
/// and a nested error a callback recorded stays visible to the next callback.
const CALLBACK_LATER_ERRORS: &str = r#"$s = 'x,' . str_repeat('a', 24) . 'b';
var_dump(preg_replace_callback('/,|(a+)+$/', function ($m) { echo 'cb:', preg_last_error(), '|'; return '-'; }, $s));
echo preg_last_error(), "\n";
var_dump(preg_replace_callback('/,/', function ($m) { echo 'cb:', preg_last_error(), '|'; preg_match('/./u', "\xff"); return '-'; }, 'a,b,c'));
echo preg_last_error(), "\n";
"#;

/// Fixture: eval's own error state reports the same codes, messages and `preg_grep()` results
/// as compiled code, including `false` (not a fatal) for an invalid `preg_grep()` pattern.
const EVAL_PARITY: &str = r#"<?php
$source = 'var_dump(preg_match("/./u", "\xff"));
echo preg_last_error(), ":", preg_last_error_msg(), "|";
preg_match("/a/", "a");
echo preg_last_error(), ":", preg_last_error_msg(), "|";
echo json_encode(preg_grep("/a/", ["k" => "a", "b", "ca"])), json_encode(preg_grep("/a/", ["a", "b"], PREG_GREP_INVERT)), "|";
var_dump(preg_grep("/(/", []));
echo preg_last_error(), "after|";
echo PREG_BAD_UTF8_ERROR, PCRE_VERSION_MAJOR;' . ' // ' . $argc;
eval($source);
"#;

/// Fixture: each PCRE2 match-error category reports php-src's code, on every helper: the depth
/// limit is `PREG_RECURSION_LIMIT_ERROR`, the heap limit `PREG_INTERNAL_ERROR` and the match
/// limit `PREG_BACKTRACK_LIMIT_ERROR` (PHP 8.5 with `pcre.jit=0`; the managed PCRE2 has no JIT).
const PCRE2_ERROR_CATEGORIES: &str = r#"$depth = '/(*NO_START_OPT)(*LIMIT_DEPTH=10)(a|b)*c/';
$s = str_repeat('a', 50) . 'b';
echo json_encode([preg_match($depth, $s), preg_last_error()]);
echo json_encode([preg_match($depth, $s, $m), $m, preg_last_error()]);
echo json_encode([preg_match_all($depth, $s), preg_last_error()]);
echo json_encode([preg_replace($depth, '-', $s), preg_last_error()]);
echo json_encode([preg_split($depth, $s), preg_last_error()]);
echo json_encode([preg_replace_callback($depth, function ($m) { return '-'; }, $s), preg_last_error_msg()]), "\n";
$heap = '/(*NO_START_OPT)(*LIMIT_HEAP=1)(a|b)*c/';
$long = str_repeat('a', 100000) . 'b';
echo json_encode([preg_match($heap, $long), preg_last_error()]);
echo json_encode([preg_match_all($heap, $long, $all), preg_last_error_msg()]);
echo json_encode([preg_match('/(*NO_START_OPT)(*LIMIT_MATCH=10)(a|b)*c/', $s), preg_last_error()]), "\n";
"#;

/// Fixture: eval and compiled code share one `preg_last_error()` state, both ways, like PHP.
const SHARED_EVAL_STATE: &str = r#"<?php
eval('preg_match("/./u", "\xff");' . ' // ' . $argc);
echo preg_last_error(), ':', preg_last_error_msg(), "|";
preg_match('/./u', "\xff");
eval('echo preg_last_error(), ":", preg_last_error_msg(), "|";' . ' // ' . $argc);
eval('preg_match("/a/", "a");' . ' // ' . $argc);
echo preg_last_error(), "\n";
"#;

/// Fixture: in eval, a `$matches` destructor whose nested `preg_match()` succeeds does not
/// clear the outer call's own error, like php-src (which empties `$matches` before matching).
const EVAL_MATCHES_DESTRUCTOR: &str = r#"class EvalMatchesDtor { public function __destruct() { preg_match("/a/", "a"); } }
$m = [new EvalMatchesDtor];
var_dump(preg_match("/./u", "\xff", $m), $m, preg_last_error());
$all = [new EvalMatchesDtor];
var_dump(preg_match_all("/./u", "\xff", $all), preg_last_error());
"#;

/// The stdout-only regex error fixtures, each in compiled code and through a runtime-built
/// `eval()`, share one compile: every opaque eval lowers the whole builtin catalog.
#[test]
fn test_preg_errors_compiled_and_eval_bundle() {
    let split_rest = r#""aaaaaaaaaaaaaaaaaaaaaaaab""#;
    let split_expected = format!("[\"x\",{split_rest}]0\n[\"x\",{split_rest}]0\nbool(false)\n2\n");
    let nested_expected = "string(3) \"xBx\"\n4\nstring(3) \"zNz\"\n4\nNULL\n4\nstring(2) \"bB\"\n4\n";
    let grep_expected = "[\"a\"]0\n{\"x\":\"a\"}error\n[]0\n[]0\nbool(false)\n1\n";
    let later_expected = "cb:0|NULL\n2\ncb:0|cb:4|string(5) \"a-b-c\"\n4\n";
    let grep_error_expected = "{\"x\":\"a\"}4\n[\"a\"]0\n";
    let aot = |body: &str| format!("<?php\n{body}");
    let sources = [
        aot(NESTED_CALLBACK_ERRORS), opaque_eval(NESTED_CALLBACK_ERRORS),
        aot(SPLIT_LIMIT_ERRORS), opaque_eval(SPLIT_LIMIT_ERRORS),
        aot(GREP_PATTERN_CHECK), opaque_eval(GREP_PATTERN_CHECK),
        aot(CALLBACK_LATER_ERRORS), opaque_eval(CALLBACK_LATER_ERRORS),
        aot(GREP_MATCH_ERROR), opaque_eval(GREP_MATCH_ERROR),
        aot(PCRE2_ERROR_CATEGORIES), opaque_eval(PCRE2_ERROR_CATEGORIES),
        opaque_eval(EVAL_MATCHES_DESTRUCTOR),
    ];
    let categories_expected = "[false,3][false,[],3][false,3][null,3][false,3][null,\"Recursion limit exhausted\"]\n\
        [false,1][false,\"Internal error\"][false,2]\n";
    let malformed = "Malformed UTF-8 characters, possibly incorrectly encoded";
    let shared_expected = format!("4:{malformed}|4:{malformed}|0\n");
    let destructor_expected = "bool(false)\narray(0) {\n}\nint(4)\nbool(false)\nint(4)\n";
    assert_bundle_with(
        &[
            BundleCase {
                name: "preg_errors_grep_eval_parity",
                source: EVAL_PARITY,
                expected: "bool(false)\n4:Malformed UTF-8 characters, possibly incorrectly encoded|0:No error|{\"k\":\"a\",\"1\":\"ca\"}{\"1\":\"b\"}|bool(false)\n1after|410",
            },
            BundleCase { name: "nested_callback_errors_aot", source: &sources[0], expected: nested_expected },
            BundleCase { name: "nested_callback_errors_eval", source: &sources[1], expected: nested_expected },
            BundleCase { name: "split_limit_errors_aot", source: &sources[2], expected: &split_expected },
            BundleCase { name: "split_limit_errors_eval", source: &sources[3], expected: &split_expected },
            BundleCase { name: "grep_pattern_check_aot", source: &sources[4], expected: grep_expected },
            BundleCase { name: "grep_pattern_check_eval", source: &sources[5], expected: grep_expected },
            BundleCase { name: "callback_later_errors_aot", source: &sources[6], expected: later_expected },
            BundleCase { name: "callback_later_errors_eval", source: &sources[7], expected: later_expected },
            BundleCase { name: "grep_match_error_aot", source: &sources[8], expected: grep_error_expected },
            BundleCase { name: "grep_match_error_eval", source: &sources[9], expected: grep_error_expected },
            BundleCase { name: "pcre2_error_categories_aot", source: &sources[10], expected: categories_expected },
            BundleCase { name: "pcre2_error_categories_eval", source: &sources[11], expected: categories_expected },
            BundleCase { name: "eval_matches_destructor", source: &sources[12], expected: destructor_expected },
            BundleCase { name: "shared_eval_state", source: SHARED_EVAL_STATE, expected: &shared_expected },
        ],
        compile_and_run_with_regex,
    );
}

/// Fixture: a nested `preg_*` error raised by an entry's `__toString()` does not stop
/// `preg_grep()`: both matching entries come back, like php-src.
const GREP_NESTED_TO_STRING_ERROR: &str = r#"class GrepNestedToString { public function __toString(): string { preg_match('/./u', "\xff"); return 'x'; } }
echo json_encode(preg_grep('/x/', [new GrepNestedToString(), 'x', 'y'])), "\n";
echo preg_last_error(), "\n";
"#;

/// A nested error from an entry's `__toString()` is not `preg_grep()`'s own error: the scan
/// keeps going, and the error stays visible through `preg_last_error()` after the call (4),
/// because php-src resets the state once per call, not before each entry's match. Compiled
/// code and eval agree.
#[test]
fn test_preg_grep_nested_to_string_error_keeps_scanning() {
    let compiled = compile_and_run_with_regex(&format!("<?php\n{GREP_NESTED_TO_STRING_ERROR}"));
    assert_eq!(compiled, "[{},\"x\"]\n4\n");
    let eval = compile_and_run_with_regex(&opaque_eval(GREP_NESTED_TO_STRING_ERROR));
    assert_eq!(eval, "[{},\"x\"]\n4\n");
}

/// A valid modifier eval does not implement stays eval's explicit runtime error for
/// `preg_grep()`, like the other eval `preg_*` builtins, instead of PHP `false`.
#[test]
fn test_preg_grep_eval_unsupported_modifier_is_a_runtime_error() {
    let out = compile_and_run_capture_with_regex(&opaque_eval("echo 'before|'; preg_grep('/^a$/D', ['a']); echo 'after';"));
    assert!(!out.success, "{}", out.stdout);
    assert_eq!(out.stdout, "before|");
    assert!(out.stderr.contains("Fatal error: eval() runtime failed"), "{}", out.stderr);
}

/// Fixture: a match error stops `preg_grep()`; a clean call afterwards resets the state.
const GREP_MATCH_ERROR: &str = r#"echo json_encode(preg_grep('/a/u', ['x' => 'a', 5 => "\xff", 7 => 'a'])), preg_last_error(), "\n";
echo json_encode(preg_grep('/a/', ['a', 'b'])), preg_last_error(), "\n";
"#;

/// Fixture: an entry's `__toString()` records a nested `preg_*` error but returns matching
/// text; `class` names the declaring class so compiled and eval cases can share a bundle.
fn grep_nested_to_string_result(class: &str) -> String {
    format!(
        r#"class {class} {{ public function __toString(): string {{ preg_match('/./u', "\xff"); return 'x'; }} }}
echo json_encode(preg_grep('/x/', [new {class}(), 'x', 'y'])), preg_last_error(), "\n";
"#
    )
}

/// Fixture: a later entry's `__toString()` runs a `preg_*` call that succeeds and so clears the
/// nested error an earlier entry recorded; `prefix` names the two classes.
fn grep_nested_error_cleared(prefix: &str) -> String {
    format!(
        r#"class {prefix}Bad {{ public function __toString(): string {{ preg_match('/./u', "\xff"); return 'x1'; }} }}
class {prefix}Good {{ public function __toString(): string {{ preg_match('/a/', 'a'); return 'x2'; }} }}
echo json_encode(preg_grep('/x/', [new {prefix}Bad(), new {prefix}Good(), 'y'])), preg_last_error(), "\n";
"#
    )
}

/// From PHP 8.6 (php-src GH-11936) `preg_grep()` returns `false` instead of the entries
/// collected so far whenever its error state is set at the end: by its own match error, and
/// by a nested error an entry's `__toString()` recorded, which `preg_last_error()` still
/// reports (php-src resets the state only once per call). A later entry whose nested
/// `preg_*` call succeeds clears that error, so the call returns its entries. Compiled code
/// and eval agree. The 8.5 cases live in the 8.5 bundle.
#[test]
fn test_preg_grep_php_8_6_bundle() {
    let match_error = "false4\n[\"a\"]0\n";
    let sources = [
        format!("<?php\n{GREP_MATCH_ERROR}"),
        opaque_eval(GREP_MATCH_ERROR),
        format!("<?php\n{}", grep_nested_to_string_result("GrepNestedAot86")),
        opaque_eval(&grep_nested_to_string_result("GrepNestedEval86")),
        format!("<?php\n{}", grep_nested_error_cleared("GrepClearedAot86")),
        opaque_eval(&grep_nested_error_cleared("GrepClearedEval86")),
    ];
    let cleared = "[{},{}]0\n";
    assert_bundle_with(
        &[
            BundleCase { name: "grep_match_error_aot", source: &sources[0], expected: match_error },
            BundleCase { name: "grep_match_error_eval", source: &sources[1], expected: match_error },
            BundleCase { name: "grep_nested_to_string_aot", source: &sources[2], expected: "false4\n" },
            BundleCase { name: "grep_nested_to_string_eval", source: &sources[3], expected: "false4\n" },
            BundleCase { name: "grep_nested_error_cleared_aot", source: &sources[4], expected: cleared },
            BundleCase { name: "grep_nested_error_cleared_eval", source: &sources[5], expected: cleared },
        ],
        |source| {
            compile_and_run_with_regex_and_php_version(
                source,
                elephc::php_version::PhpVersion::Php86,
            )
        },
    );
}
