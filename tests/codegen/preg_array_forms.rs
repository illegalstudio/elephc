//! Purpose:
//! Regression tests for the array and flag forms of php's replacement builtins --
//! `preg_replace()`, `preg_replace_callback()`, `preg_filter()`, `preg_replace_callback_array()`
//! -- and for `preg_grep()`, compiled and interpreted.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected output is `php -n` 8.5's own output for the same fixture (warnings lose php's
//!   "in FILE on line N" suffix, which a compiled binary cannot name).
//! - The compiled side reaches these forms through `src/preg_array_prelude.rs`; the interpreter
//!   through `interpreter::builtins::regex::preg_array`. The eval fixtures run the SAME programs
//!   inside `eval()`, so a difference between the two is a failure of one of them.

use crate::support::*;

/// Asserts stderr carries each of php's diagnostics, in php's order.
fn assert_diagnostics(stderr: &str, expected: &[&str]) {
    let mut rest = stderr;
    for diagnostic in expected {
        let Some(at) = rest.find(diagnostic) else {
            panic!("missing {diagnostic:?} in stderr: {stderr}");
        };
        rest = &rest[at + diagnostic.len()..];
    }
}

/// An array subject is replaced element by element under its own key, each element converted to a
/// string first. The native runtime replaces one string, so the call refused to compile.
#[test]
fn test_preg_replace_array_subject_keeps_keys_and_converts_elements() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
var_dump(preg_replace('/e/', 'E', ['a' => 'apple', 'b' => 'berry', 'n' => null, 'i' => 5, 7 => 'eel']));
"#,
    );
    assert_eq!(out.stdout, r#"array(5) {
  ["a"]=>
  string(5) "applE"
  ["b"]=>
  string(5) "bErry"
  ["n"]=>
  string(0) ""
  ["i"]=>
  string(1) "5"
  [7]=>
  string(3) "EEl"
}
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// An array replacement pairs with the patterns in ITERATION order -- its keys are ignored -- and
/// runs out as the empty string; a scalar replacement serves every pattern.
#[test]
fn test_preg_replace_array_replacement_is_positional() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
var_dump(preg_replace(['k1' => '/a/', 'k2' => '/b/'], ['z' => 'X', 'y' => 'Y'], 'ab'));
var_dump(preg_replace(['/a/', '/b/'], ['x'], 'ab'));
var_dump(preg_replace(['/a/', '/e/'], 'X', 'apple pie'));
var_dump(preg_replace(['/a/', '/e/'], ['1'], ['apple', 'pear'], -1, $count), $count);
"#,
    );
    assert_eq!(out.stdout, r#"string(2) "XY"
string(1) "x"
string(9) "XpplX piX"
array(2) {
  [0]=>
  string(4) "1ppl"
  [1]=>
  string(3) "p1r"
}
int(4)
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// php refuses an array replacement against a single pattern with a TypeError naming the function
/// the program called.
#[test]
fn test_preg_replace_scalar_pattern_with_array_replacement_throws() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
try { preg_replace('/a/', ['x'], 'a'); } catch (TypeError $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { preg_filter('/a/', ['x'], 'a'); } catch (TypeError $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(out.stdout, r#"TypeError: preg_replace(): Argument #1 ($pattern) must be of type array when argument #2 ($replacement) is an array, string given
TypeError: preg_filter(): Argument #1 ($pattern) must be of type array when argument #2 ($replacement) is an array, string given
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// A pattern that fails makes a scalar subject null and DROPS an array element, while `$count` still
/// totals the replacements made before the failure (php 8.5: one).
#[test]
fn test_preg_replace_failed_pattern_drops_subjects_and_keeps_count() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
var_dump(@preg_replace(['/a/', '/(/'], 'x', 'ab'));
var_dump(@preg_replace(['/a/', '/(/'], 'x', ['k' => 'ab', 'j' => 'cd'], -1, $c), $c);
"#,
    );
    assert_eq!(out.stdout, r#"NULL
array(0) {
}
int(1)
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// `preg_filter()` is `preg_replace()` that drops a subject no pattern matched: null for a scalar
/// one, a missing key for an array element.
#[test]
fn test_preg_filter_keeps_only_matched_subjects() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
var_dump(preg_filter('/a/', 'x', 'bcd'), preg_filter('/a/', 'x', 'abc'));
var_dump(preg_filter(['/a/', '/b/'], 'x', ['aa', 'cc', 'bb', 'k' => 'ab'], 1, $c), $c);
var_dump(preg_filter('/a/', 'x', ['apple', 'kiwi'], -1, $fresh), $fresh);
"#,
    );
    assert_eq!(out.stdout, r#"NULL
string(3) "xbc"
array(3) {
  [0]=>
  string(2) "xa"
  [2]=>
  string(2) "xb"
  ["k"]=>
  string(2) "xx"
}
int(4)
array(1) {
  [0]=>
  string(5) "xpple"
}
int(1)
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// Each `pattern => callback` entry runs over the subject the previous one produced, for a scalar
/// or an array subject, with `$limit` per pattern and `$count` totalled across them; an array
/// callable is as good as a closure.
#[test]
fn test_preg_replace_callback_array_applies_each_pattern_in_turn() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
class Lower { public function low($m) { return strtolower($m[0]); } }
$k = new Lower;
var_dump(preg_replace_callback_array(['/a/' => fn($m) => 'A', '/b/' => fn($m) => 'B'], 'aabbc', -1, $c), $c);
var_dump(preg_replace_callback_array(['/a/' => fn($m) => 'A', '/b/' => fn($m) => 'B'], ['x' => 'ab', 'y' => 'cc'], 1, $c), $c);
var_dump(preg_replace_callback_array(['/B/' => [$k, 'low'], '/a/' => fn($m) => 'A'], 'aBc', -1, $n), $n);
try { preg_replace_callback_array(['/a/' => 'no_such_function'], 'aa'); } catch (TypeError $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
"#,
    );
    assert_eq!(out.stdout, r#"string(5) "AABBc"
int(4)
array(2) {
  ["x"]=>
  string(2) "AB"
  ["y"]=>
  string(2) "cc"
}
int(2)
string(3) "Abc"
int(2)
TypeError: preg_replace_callback_array(): Argument #1 ($pattern) must contain only valid callbacks
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// `$flags` reach the callback: PREG_OFFSET_CAPTURE hands it `[text, offset]` pairs and
/// PREG_UNMATCHED_AS_NULL keeps a group that did not participate as null. Both were ignored.
#[test]
fn test_preg_replace_callback_flags_shape_the_matches() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
var_dump(preg_replace_callback('/(a)(x)?/', fn($m) => json_encode($m), 'za', -1, $c, PREG_OFFSET_CAPTURE));
var_dump(preg_replace_callback('/(a)(x)?/', fn($m) => json_encode($m), 'za', -1, $c, PREG_UNMATCHED_AS_NULL));
var_dump(preg_replace_callback('/(a)(x)?/', fn($m) => json_encode($m), 'zaza', 1, $c, PREG_OFFSET_CAPTURE), $c);
"#,
    );
    assert_eq!(out.stdout, r#"string(18) "z[["a",1],["a",1]]"
string(15) "z["a","a",null]"
string(20) "z[["a",1],["a",1]]za"
int(1)
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// `preg_replace_callback()` with several patterns, or several subjects, applies them the way
/// `preg_replace()` does.
#[test]
fn test_preg_replace_callback_array_patterns_and_subjects() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
var_dump(preg_replace_callback(['/a/', '/b/'], fn($m) => strtoupper($m[0]), ['ab', 'ba'], -1, $c), $c);
var_dump(preg_replace_callback('/r+/', fn($m) => strtoupper($m[0]), ['a' => 'apple', 'b' => 'berry'], -1, $n), $n);
var_dump(preg_replace_callback('/\d+/', fn($m) => $m[0] * 2, ['x1', 'y22'], 1, $c), $c);
"#,
    );
    assert_eq!(out.stdout, r#"array(2) {
  [0]=>
  string(2) "AB"
  [1]=>
  string(2) "BA"
}
int(4)
array(2) {
  ["a"]=>
  string(5) "apple"
  ["b"]=>
  string(5) "beRRy"
}
int(1)
array(2) {
  [0]=>
  string(2) "x2"
  [1]=>
  string(3) "y44"
}
int(2)
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// Named arguments reach the same forms: `flags:` skipping `$count`, array operands out of order,
/// and `preg_filter()` by name. A named call bailed out before the redirect and lost its flags.
#[test]
fn test_named_arguments_reach_the_array_and_flag_forms() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
var_dump(preg_replace_callback('/a/', fn($m) => 'A', 'za', flags: 0));
var_dump(preg_replace_callback(pattern: '/(a)(x)?/', callback: fn($m) => json_encode($m), subject: 'za', flags: PREG_OFFSET_CAPTURE));
var_dump(preg_replace(subject: ['k' => 'hello'], pattern: ['/l/', '/o/'], replacement: ['L', 'O']));
var_dump(preg_filter(subject: ['hello', 'x'], pattern: '/l/', replacement: 'L'));
"#,
    );
    assert_eq!(out.stdout, r#"string(2) "zA"
string(18) "z[["a",1],["a",1]]"
array(1) {
  ["k"]=>
  string(5) "heLLO"
}
array(1) {
  [0]=>
  string(5) "heLLo"
}
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// A call through `call_user_func()` / `call_user_func_array()` reaches the same forms; the static
/// callable lowering handed `preg_replace()` a hash subject.
#[test]
fn test_dynamic_calls_reach_the_array_forms() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
var_dump(call_user_func('preg_filter', '/e/', 'E', ['hello', 'xyz']));
var_dump(call_user_func_array('preg_replace', ['/e/', 'E', ['a' => 'hello']]));
"#,
    );
    assert_eq!(out.stdout, r#"array(1) {
  [0]=>
  string(5) "hEllo"
}
array(1) {
  ["a"]=>
  string(5) "hEllo"
}
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// A literal pattern PCRE2 rejects when it compiles -- an unbalanced group -- makes `preg_grep()`
/// answer false, so the call must be typed for it. It answered an empty array.
#[test]
fn test_preg_grep_literal_pattern_that_cannot_compile_is_false() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
var_dump(@preg_grep('/(/', ['a']));
var_dump(@preg_match('/a)/', 'a'));
var_dump(preg_grep('/^a/', ['x' => 'apple', 'y' => 'berry', 3 => 'avocado', 4 => 12]));
var_dump(preg_grep('/^a/', ['apple', 'berry', 'avocado'], PREG_GREP_INVERT));
"#,
    );
    assert_eq!(out.stdout, r#"bool(false)
bool(false)
array(2) {
  ["x"]=>
  string(5) "apple"
  [3]=>
  string(7) "avocado"
}
array(1) {
  [1]=>
  string(5) "berry"
}
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// A `preg_replace_callback()` whose pattern is a variable returns `?string`, boxed in the result
/// register -- which releasing the callback's descriptor reloaded, so the call returned the
/// descriptor's address: `var_dump` printed an `int`.
#[test]
fn test_boxed_callback_result_survives_descriptor_release() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
class K { public static function up($m) { return strtoupper($m[0]); } public function low($m) { return strtolower($m[0]); } }
function viaPattern($cb, $p) { $r = preg_replace_callback($p, $cb, 'APPLE'); return [$r]; }
var_dump(viaPattern([new K, 'low'], '/P/'));
var_dump(viaPattern('K::up', '/P/'));
"#,
    );
    assert_eq!(out.stdout, r#"array(1) {
  [0]=>
  string(5) "AppLE"
}
array(1) {
  [0]=>
  string(5) "APPLE"
}
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// The interpreter answers the array and flag forms exactly as compiled code does, down to the
/// function a warning names.
#[test]
fn test_eval_preg_array_forms_match_compiled() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
preg_match('/warm/', 'warm');
$code = <<<'PHP'
class Lower { public function low($m) { return strtolower($m[0]); } }
$k = new Lower;
var_dump(preg_replace(['k1' => '/a/', 'k2' => '/b/'], ['z' => 'X', 'y' => 'Y'], 'ab'));
try { preg_replace('/a/', ['x'], 'a'); } catch (TypeError $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
var_dump(preg_replace(['/a/', '/(/'], 'x', ['k' => 'ab', 'j' => 'cd'], -1, $c), $c);
var_dump(preg_replace('/a/', 'x', ['k' => 'ab', 'j' => 5, 'n' => null]));
var_dump(preg_filter(['/a/', '/b/'], 'x', ['aa', 'cc', 'bb', 'k' => 'ab'], 1, $c), $c);
var_dump(preg_replace_callback_array(['/B/' => [$k, 'low'], '/a/' => fn($m) => 'A'], ['x' => 'aBc'], -1, $n), $n);
var_dump(preg_replace_callback_array(['/(/' => fn($m) => 'A'], 'aa'));
var_dump(preg_replace_callback('/(a)(x)?/', fn($m) => json_encode($m), 'za', -1, $c, PREG_OFFSET_CAPTURE));
var_dump(preg_replace_callback(pattern: '/(a)(x)?/', callback: fn($m) => json_encode($m), subject: 'za', flags: PREG_UNMATCHED_AS_NULL));
var_dump(preg_grep('/^a/', ['x' => 'apple', 'y' => 'berry'], PREG_GREP_INVERT));
var_dump(@preg_grep('/(/', ['a']));
var_dump(preg_replace(['/a/'], 'x', [['nested'], 'a']));
PHP;
eval($code);
"#,
    );
    assert_eq!(out.stdout, r#"string(2) "XY"
TypeError: preg_replace(): Argument #1 ($pattern) must be of type array when argument #2 ($replacement) is an array, string given
array(0) {
}
int(1)
array(3) {
  ["k"]=>
  string(2) "xb"
  ["j"]=>
  string(1) "5"
  ["n"]=>
  string(0) ""
}
array(3) {
  [0]=>
  string(2) "xa"
  [2]=>
  string(2) "xb"
  ["k"]=>
  string(2) "xx"
}
int(4)
array(1) {
  ["x"]=>
  string(3) "Abc"
}
int(2)
NULL
string(18) "z[["a",1],["a",1]]"
string(15) "z["a","a",null]"
array(1) {
  ["y"]=>
  string(5) "berry"
}
bool(false)
array(2) {
  [0]=>
  string(5) "Arrxy"
  [1]=>
  string(1) "x"
}
"#, "stderr: {}", out.stderr);
    assert_diagnostics(
        &out.stderr,
        &[
            r#"Warning: preg_replace(): Compilation failed: missing closing parenthesis at offset 1"#,
            r#"Warning: preg_replace(): Compilation failed: missing closing parenthesis at offset 1"#,
            r#"Warning: preg_replace_callback_array(): Compilation failed: missing closing parenthesis at offset 1"#,
            r#"Warning: Array to string conversion"#,
        ],
    );
}

/// `@` in eval silences a warning that the runtime, not the interpreter, raises -- a builtin's
/// helper or the regex shim -- as php's `@` does.
#[test]
fn test_eval_error_suppression_silences_runtime_warnings() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
preg_match('/warm/', 'warm');
$code = <<<'PHP'
var_dump(@hex2bin('abc'));
var_dump(@preg_match('/(/', 'x'));
$x = @preg_match('/(/', 'x');
var_dump($x);
PHP;
eval($code);
"#,
    );
    assert_eq!(out.stdout, r#"bool(false)
bool(false)
bool(false)
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// Binding `$matches` by reference DEFINES it as null before the pattern is compiled, so a rejected
/// pattern leaves it null rather than undefined.
#[test]
fn test_eval_rejected_pattern_still_defines_matches() {
    let out = compile_and_run_capture_with_regex(
        r#"<?php
preg_match('/warm/', 'warm');
$code = <<<'PHP'
var_dump(@preg_match_all('/a/Q', 'abc', $all), $all);
var_dump(@preg_match('/(/', 'x', $m), $m);
$keep = ['keep'];
var_dump(@preg_match('/(/', 'x', $keep), $keep);
PHP;
eval($code);
"#,
    );
    assert_eq!(out.stdout, r#"bool(false)
NULL
bool(false)
NULL
bool(false)
array(1) {
  [0]=>
  string(4) "keep"
}
"#, "stderr: {}", out.stderr);
    assert!(!out.stderr.contains("Warning:"), "stderr: {}", out.stderr);
}

/// A NUL byte inside a pattern is an ordinary character to match, compiled and interpreted. The
/// compiled helper that hands the pattern to PCRE2 as a C string truncated it at the NUL, so
/// `"~[\s\0]+~"` (Laravel's `Str::trim()`) lost its closing bracket and failed to compile.
/// Output is php 8.5's.
#[test]
fn test_preg_pattern_with_a_nul_byte_matches_php() {
    let out = compile_and_run(r#"<?php
var_dump(preg_replace("~[\\s\0]+~", "", " x\0y "));
var_dump(preg_match("~a\0b~", "a\0b"));
var_dump(preg_match("~a\\\0b~", "a\0b"));
echo json_encode(preg_split("~\0~", "a\0b\0c")), "\n";
$code = 'return [preg_replace("~[\\\\s\0]+~", "", " x\0y "), preg_match("~a\0b~", "a\0b")];';
echo json_encode(eval($code)), "\n";
"#);
    assert_eq!(out, "string(2) \"xy\"\nint(1)\nint(1)\n[\"a\",\"b\",\"c\"]\n[\"xy\",1]\n");
}

/// `preg_match_all(..., PREG_SET_ORDER)` drops, per match, the trailing groups that did not
/// participate, as php does (`["ab","ab"]`, then `["cd=5","cd","5"]`); PATTERN_ORDER still fills
/// every column and UNMATCHED_AS_NULL keeps every group. The interpreter padded every row with
/// `""`, so Symfony's `HeaderBag::parseCacheControl()` read an empty string where php had `true`
/// and answered `Cache-Control: no-cache0private, private`. The same body runs compiled and
/// inside eval() (where it is declared by the fragment, so the interpreter's engine runs it).
/// Output is php 8.5's.
#[test]
fn test_preg_match_all_set_order_drops_trailing_unmatched_groups() {
    let out = compile_and_run(r##"<?php
function probe() {
    preg_match_all("#([a-z]+)(?:=([0-9]+))?#", "ab, cd=5", $m, PREG_SET_ORDER);
    echo json_encode($m), "\n";
    preg_match_all("#([a-z]+)(?:=([0-9]+))?#", "ab, cd=5", $m);
    echo json_encode($m), "\n";
    preg_match_all("#([a-z]+)(?:=([0-9]+))?#", "ab, cd=5", $m, PREG_SET_ORDER | PREG_UNMATCHED_AS_NULL);
    echo json_encode($m), "\n";
}
probe();
$code = 'function probe_eval() {
    preg_match_all("#([a-z]+)(?:=([0-9]+))?#", "ab, cd=5", $m, PREG_SET_ORDER);
    echo json_encode($m), "\n";
    preg_match_all("#([a-z]+)(?:=([0-9]+))?#", "ab, cd=5", $m);
    echo json_encode($m), "\n";
    preg_match_all("#([a-z]+)(?:=([0-9]+))?#", "ab, cd=5", $m, PREG_SET_ORDER | PREG_UNMATCHED_AS_NULL);
    echo json_encode($m), "\n";
}
probe_eval();';
eval($code);
"##);
    let expected = "[[\"ab\",\"ab\"],[\"cd=5\",\"cd\",\"5\"]]\n[[\"ab\",\"cd=5\"],[\"ab\",\"cd\"],[\"\",\"5\"]]\n[[\"ab\",\"ab\",null],[\"cd=5\",\"cd\",\"5\"]]\n";
    assert_eq!(out, format!("{expected}{expected}"));
}

/// `preg_match_all(..., PREG_SET_ORDER)` with a named group that only SOME matches use returns rows
/// of different shapes: a plain list for the first match (`["x","x"]`) and a keyed row for the next
/// (`{"0":",","1":"","s":",","2":","}`). Rows reached the result typed as lists whatever they held,
/// so the first read of a keyed row after a list row took a bus error. Symfony's
/// `HeaderUtils::split()` is exactly this call (named group `separator`), and every response that
/// built a `Cache-Control` header through it died or came out as `no-cache0private`. Output is
/// php 8.5's.
#[test]
fn test_preg_match_all_set_order_rows_may_mix_lists_and_keyed_rows() {
    let out = compile_and_run(r##"<?php
function probe() {
    preg_match_all('/(x)|(?<s>,)/', 'x,', $m, PREG_SET_ORDER);
    echo json_encode($m), "\n";
    var_dump($m[1]);
    preg_match_all('/(?<s>,)|(x)/', 'x,', $m, PREG_SET_ORDER);
    echo json_encode($m), "\n";
    $n = preg_match_all('/(?<k>\d)(?<v>[a-z])?/', '1a2', $mm, PREG_SET_ORDER);
    echo $n, json_encode($mm), "\n";
}
probe();
"##);
    assert_eq!(
        out,
        "[[\"x\",\"x\"],{\"0\":\",\",\"1\":\"\",\"s\":\",\",\"2\":\",\"}]\n\
         array(4) {\n  [0]=>\n  string(1) \",\"\n  [1]=>\n  string(0) \"\"\n  [\"s\"]=>\n  string(1) \",\"\n  [2]=>\n  string(1) \",\"\n}\n\
         [{\"0\":\"x\",\"s\":\"\",\"1\":\"\",\"2\":\"x\"},{\"0\":\",\",\"s\":\",\",\"1\":\",\"}]\n\
         2[{\"0\":\"1a\",\"k\":\"1\",\"1\":\"1\",\"v\":\"a\",\"2\":\"a\"},{\"0\":\"2\",\"k\":\"2\",\"1\":\"2\"}]\n"
    );
}
