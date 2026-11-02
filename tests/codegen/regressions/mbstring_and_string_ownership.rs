//! Purpose:
//! Regression tests for the mbstring functions and for string temporaries they exposed leaking.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - These functions were once a UTF-8-only AST prelude; the native mbstring extension now
//!   implements every one of them, and a program declaring its own function of the same name
//!   (a polyfill) still keeps it.
//! - `$s[$i]` is an owned one-byte copy; it was treated as scratch and never released.
//! - A builtin's fresh string result was persisted again at returns, wrapper boundaries and before
//!   a concatenation's call-like right operand, orphaning the first copy.
//! - A call to a user function no longer makes the caller's return-alias summary `Unknown` unless
//!   the caller handed out a reference to one of its locals, so argument temporaries are released.

use crate::support::*;

/// The prelude's mbstring functions count characters as php counts them.
#[test]
fn test_mbstring_prelude_functions_match_php() {
    let out = compile_and_run(
        r#"<?php
$s = "héllo wörld ✓ 😀";
var_dump(mb_substr($s, 1, 4), mb_substr($s, -3), mb_substr($s, 2), mb_substr($s, -5, 2), mb_substr($s, 3, -4), mb_substr($s, 20), mb_substr($s, 0, 0));
var_dump(mb_strpos($s, "ö"), mb_strpos($s, "l", 4), mb_strpos($s, "z"), mb_strpos($s, "o", -6));
var_dump(mb_strrpos($s, "l"), mb_strrpos($s, "o", -8), mb_stripos("HÉLLO", "é"), mb_strripos("aXbx", "X"));
var_dump(mb_str_split("añb✓"), mb_str_split("añb✓c", 2));
var_dump(mb_substr_count("ñañaña", "ña"), mb_strstr("héllo", "l"), mb_strstr("héllo", "l", true), mb_strrchr("a/b/c", "/"), mb_stristr("HÉLLO", "é"));
var_dump(mb_internal_encoding(), mb_internal_encoding("UTF-8"), mb_check_encoding("héllo"), mb_check_encoding("\xC3\x28"), mb_check_encoding("\xED\xA0\x80"), mb_check_encoding("a\xF0\x9F\x98"));
var_dump(mb_str_pad("ñ", 4, "✓"), mb_str_pad("ñ", 5, "ab", STR_PAD_LEFT), mb_str_pad("ñ", 6, "xy", STR_PAD_BOTH), mb_ucfirst("élan"), mb_lcfirst("Élan"));
try { mb_strpos("abc", "a", 9); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { mb_str_split("abc", 0); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
echo implode("|", array_map(fn($w) => mb_substr($w, 0, 2), ["été", "über", "ça"])), "\n";
"#,
    );
    assert_eq!(out, "string(5) \"éllo\"\nstring(8) \"✓ 😀\"\nstring(19) \"llo wörld ✓ 😀\"\nstring(2) \"d \"\nstring(9) \"lo wörld\"\nstring(0) \"\"\nstring(0) \"\"\nint(7)\nint(9)\nbool(false)\nbool(false)\nint(9)\nint(4)\nint(1)\nint(3)\narray(4) {\n  [0]=>\n  string(1) \"a\"\n  [1]=>\n  string(2) \"ñ\"\n  [2]=>\n  string(1) \"b\"\n  [3]=>\n  string(3) \"✓\"\n}\narray(3) {\n  [0]=>\n  string(3) \"añ\"\n  [1]=>\n  string(4) \"b✓\"\n  [2]=>\n  string(1) \"c\"\n}\nint(3)\nstring(3) \"llo\"\nstring(3) \"hé\"\nstring(2) \"/c\"\nstring(5) \"ÉLLO\"\nstring(5) \"UTF-8\"\nbool(true)\nbool(true)\nbool(false)\nbool(false)\nbool(false)\nstring(11) \"ñ✓✓✓\"\nstring(6) \"ababñ\"\nstring(7) \"xyñxyx\"\nstring(5) \"Élan\"\nstring(5) \"élan\"\nmb_strpos(): Argument #3 ($offset) must be contained in argument #1 ($haystack)\nmb_str_split(): Argument #2 ($length) must be greater than 0\nét|üb|ça\n");
}

/// A polyfill guarded by `function_exists()` yields to the native mbstring function, and the
/// program still gets mbstring's other functions without a second declaration of any name.
///
/// mbstring is a native extension now (it used to be an AST prelude a user declaration could
/// replace), so an UNGUARDED `function mb_substr()` is the `Cannot redeclare` error php itself
/// raises with the extension loaded. The guarded form is how every real polyfill is written, and
/// php 8.5 prints `el|2` for this program: the extension's `mb_substr` wins.
#[test]
fn test_mbstring_prelude_yields_to_a_declared_polyfill() {
    let out = compile_and_run(
        r#"<?php
if (!function_exists('mb_substr')) {
    function mb_substr($s, $start, $length = null) { return "poly:" . substr($s, $start, $length ?? 99); }
}
echo mb_substr("hello", 1, 2), "|", mb_strpos("héllo", "l"), "\n";
"#,
    );
    assert_eq!(out, "el|2\n");
}

/// String offsets, builtin results and helper-calling functions release what they allocate.
#[test]
fn test_string_offsets_fresh_results_and_helper_calls_are_leak_free() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function adv(string $s, int $byte, int $chars): int { return $byte + $chars; }
function split2(string $string, int $length = 1, ?string $encoding = null): array {
    $_unused = $encoding;
    $parts = [];
    $len = strlen($string);
    $byte = 0;
    while ($byte < $len) { $next = adv($string, $byte, $length); $parts[] = substr($string, $byte, $next - $byte); $byte = $next; }
    return $parts;
}
function r1() { $s = "hello" . mt_rand(1, 1); $t = 0; for ($i = 0; $i < strlen($s); $i++) { $t += ord($s[$i]); } return $t; }
function r2() { $s = "hello" . mt_rand(1, 1); return $s[1]; }
function r4() { $s = "hello" . mt_rand(1, 1); $a = []; for ($i = 0; $i < 3; $i++) { $a[] = $s[$i]; } return implode("", $a); }
function n1() { $x = "é" . mt_rand(1, 1); return mb_strtoupper($x); }
function j4() { return mb_strtoupper(mb_substr("élan", 0, 1)) . mb_substr("élan", 1); }
function z1() { return count(split2("abc")); }
function m3() { return implode(",", mb_str_split("añb✓c", 2)); }
function run(): void {
    $out = "";
    for ($i = 0; $i < 4; $i++) {
        $out = r1() . "|" . r2() . "|" . r4() . "|" . n1() . "|" . j4() . "|" . z1() . "|" . m3();
    }
    echo $out, "\n";
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "581|e|hel|É1|Élan|3|añ,b✓,c\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}

/// A mixed value passed to a `string` parameter is coerced into a copy the caller owns. No callee
/// returns that copy (a string result is persisted, and only `.` heap temporaries are adopted in
/// place), so it is released after the call whether the callee concatenates, slices or returns
/// its parameter. A large `.` temporary handed to the identity function is still not freed twice.
#[test]
fn test_mixed_to_string_argument_copy_is_released_after_the_call() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function sub1(string $s) { return substr($s, 1); }
function sub2(string $s) { return $s . "!"; }
function sub3(string $s) { return $s; }
function u1($s) { return sub1($s); }
function u2($s) { return sub2($s); }
function u3($s) { return sub3($s); }
function run(): void {
    $out = "";
    $big = 0;
    for ($i = 0; $i < 4; $i++) {
        $word = "h\u{e9}llo" . mt_rand(1, 1);
        $out = u1($word) . "|" . u2($word) . "|" . u3($word) . "|" . u2(40 + mt_rand(2, 2));
        $big = strlen(sub3("a" . str_repeat("x", 70000 + $i)));
    }
    echo $out, "|", $big, "\n";
}
run();
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "\u{e9}llo1|h\u{e9}llo1!|h\u{e9}llo1|42!|70004\n");
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}
