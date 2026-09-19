//! Purpose:
//! Regression tests for PHP's `$flags` argument on `ksort()` and `krsort()`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expectation is verbatim `php` (PHP 8.5.10) output for the same fixture.
//! - Before this suite both builtins were unary and always compared keys with `SORT_REGULAR`;
//!   `ksort($a, SORT_STRING)` did not compile.
//! - The mode changes the ANSWER, not just the path: `10` sorts before `9` under
//!   `SORT_STRING` and after it under `SORT_NUMERIC`, which is what each fixture pins.
//! - PHP resolves the comparison from `$flags & ~SORT_FLAG_CASE` and silently ignores every
//!   other value, so `999`, `3`, `4` and a bare `SORT_FLAG_CASE` all mean `SORT_REGULAR`.
//! - `SORT_NATURAL | SORT_FLAG_CASE` folds ASCII case only. php-src folds with libc
//!   `toupper()` under the process `LC_CTYPE`, which maps Latin-1 on Darwin and not on glibc;
//!   elephc gives the same answer on every target and one fixture pins that.

use crate::support::*;

/// Verifies each documented mode orders the same keys the way PHP does.
///
/// The fixture mixes integer and string keys on purpose. An integer key has no bytes of its
/// own, so every byte-comparing mode has to spell it out as decimal digits first -- which is
/// why `10` lands before `9` under `SORT_STRING` and after it under `SORT_NUMERIC`.
#[test]
fn key_sort_flags_order_keys_the_way_php_does() {
    let out = compile_and_run(
        r#"<?php
$modes = [SORT_REGULAR, SORT_NUMERIC, SORT_STRING, SORT_LOCALE_STRING, SORT_NATURAL,
          SORT_STRING | SORT_FLAG_CASE, SORT_NATURAL | SORT_FLAG_CASE];
foreach ($modes as $mode) {
    $a = [10 => 1, 9 => 1, "img12" => 1, "img2" => 1, "IMG1" => 1, "100" => 1];
    ksort($a, $mode);
    foreach ($a as $k => $v) { echo "[", $k, "]"; }
    echo "|";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "[9][10][100][IMG1][img12][img2]|",
            "[img12][img2][IMG1][9][10][100]|",
            "[10][100][9][IMG1][img12][img2]|",
            "[10][100][9][IMG1][img12][img2]|",
            "[9][10][100][IMG1][img2][img12]|",
            "[10][100][9][IMG1][img12][img2]|",
            "[9][10][100][IMG1][img2][img12]|",
        )
    );
}

/// Verifies `krsort()` reverses each mode without disturbing keys that compare equal.
///
/// `IMG1` and `img1` are equal under `SORT_STRING | SORT_FLAG_CASE`, so a stable sort leaves
/// them in insertion order in BOTH directions. Reversing by swapping the comparator's
/// operands, instead of by taking the left run on a tie, would flip them here.
#[test]
fn key_sort_flags_reverse_without_reordering_equal_keys() {
    let out = compile_and_run(
        r#"<?php
$modes = [SORT_STRING, SORT_NUMERIC, SORT_NATURAL, SORT_STRING | SORT_FLAG_CASE];
foreach ($modes as $mode) {
    $a = ["IMG1" => 1, "img1" => 1, "img10" => 1, "img9" => 1];
    krsort($a, $mode);
    foreach ($a as $k => $v) { echo "[", $k, "]"; }
    echo "|";
}
"#,
    );
    assert_eq!(
        out,
        concat!(
            "[img9][img10][img1][IMG1]|",
            "[IMG1][img1][img10][img9]|",
            "[img10][img9][img1][IMG1]|",
            "[img9][img10][IMG1][img1]|",
        )
    );
}

/// Verifies the flag words PHP ignores are ignored here too, rather than raising.
///
/// PHP picks the comparison from `$flags & ~SORT_FLAG_CASE` and falls back to `SORT_REGULAR`
/// for anything it does not recognize. `SORT_DESC` and `SORT_ASC` are `3` and `4`: they belong
/// to `array_multisort()` and do NOT reverse a key sort.
#[test]
fn key_sort_ignores_the_flag_words_php_ignores() {
    let out = compile_and_run(
        r#"<?php
$modes = [SORT_REGULAR, SORT_FLAG_CASE, SORT_DESC, SORT_ASC, 7, 999];
foreach ($modes as $mode) {
    $a = [10 => 1, 9 => 1, "b" => 1, "A" => 1];
    ksort($a, $mode);
    foreach ($a as $k => $v) { echo "[", $k, "]"; }
    echo "|";
}
"#,
    );
    let regular = "[9][10][A][b]|";
    assert_eq!(out, regular.repeat(6));
}

/// Verifies `SORT_NUMERIC` reads a string key with PHP's grammar, not libc's.
///
/// `zend_strtod` has no hexadecimal form, no `INF`/`NAN` spelling, and consumes an exponent
/// only when a digit follows it, so three of these keys are worth `0.0` and `'1e'` is worth
/// `1.0`. Handing the raw bytes to libc `strtod` would make `'0x10'` the largest key here.
#[test]
fn numeric_key_sort_uses_phps_numeric_grammar() {
    let out = compile_and_run(
        r#"<?php
$a = ["0x10" => 1, "INF" => 1, "1e" => 1, "1e2" => 1, ".5" => 1, "2" => 1];
ksort($a, SORT_NUMERIC);
foreach ($a as $k => $v) { echo "[", $k, "]"; }
"#,
    );
    assert_eq!(out, "[0x10][INF][.5][1e][2][1e2]");
}

/// Verifies two integer keys compare exactly under `SORT_NUMERIC`, not through a double.
///
/// `9223372036854775806` and `9223372036854775807` round to the same `f64`, so a comparison
/// that went through one would report them equal and leave them in insertion order.
#[test]
fn numeric_key_sort_keeps_large_integer_keys_apart() {
    let out = compile_and_run(
        r#"<?php
$a = [9223372036854775807 => 1, 9223372036854775806 => 1];
ksort($a, SORT_NUMERIC);
foreach ($a as $k => $v) { echo "[", $k, "]"; }
"#,
    );
    assert_eq!(out, "[9223372036854775806][9223372036854775807]");
}

/// Verifies `SORT_NATURAL` reproduces php-src's `strnatcmp_ex`, quirks included.
///
/// Two keys that differ only in skipped bytes compare EQUAL, which is why `'a 7'` and `'a7'`
/// stay in insertion order. A run starting with `0` on either side compares left-aligned, so
/// `'a0.5'` sorts before `'a0.10'`; any other run compares by length first, so `'x10'` sorts
/// after `'x9'`.
#[test]
fn natural_key_sort_reproduces_php_quirks() {
    let out = compile_and_run(
        r#"<?php
$a = ["a 7" => 1, "a7" => 1, "a0.5" => 1, "a0.10" => 1, "x10" => 1, "x9" => 1, "a007" => 1];
ksort($a, SORT_NATURAL);
foreach ($a as $k => $v) { echo "[", $k, "]"; }
"#,
    );
    assert_eq!(out, "[a0.5][a0.10][a007][a 7][a7][x9][x10]");
}

/// Verifies `SORT_LOCALE_STRING` compares the prefix a C string would carry.
///
/// `strcoll` stops at the first NUL, so `'ab'` and `'ab\0c'` compare EQUAL under it while
/// `SORT_STRING`, which is length-aware, orders them. The two modes disagreeing on this one
/// pair is the whole observable difference between them in the C locale.
#[test]
fn locale_key_sort_stops_at_an_embedded_nul() {
    let out = compile_and_run(
        "<?php\n\
$a = [\"ab\\0c\" => 1, \"ab\" => 1, \"aa\" => 1];\n\
ksort($a, SORT_LOCALE_STRING);\n\
foreach ($a as $k => $v) { echo \"[\", strlen($k), \"]\"; }\n\
echo \"|\";\n\
$b = [\"ab\\0c\" => 1, \"ab\" => 1, \"aa\" => 1];\n\
ksort($b, SORT_STRING);\n\
foreach ($b as $k => $v) { echo \"[\", strlen($k), \"]\"; }\n",
    );
    assert_eq!(out, "[2][4][2]|[2][2][4]");
}

/// Verifies natural-order case folding is bounded to ASCII on every target.
///
/// php-src folds with libc `toupper()` under the process `LC_CTYPE`, so PHP itself answers
/// differently on macOS and Linux for a byte above 127: the CLI forces `C.UTF-8` at startup,
/// where Darwin's single-byte table maps Latin-1 and glibc's does not. Under `LC_CTYPE=C` PHP
/// prints exactly what this fixture asserts on both. Elephc folds `a`..`z` and nothing else,
/// which is that `C` answer on every target it compiles for.
#[test]
fn natural_key_sort_folds_ascii_case_only() {
    let out = compile_and_run(
        "<?php\n\
$a = [\"\\xff\" => 1, \"\\x80\" => 1, \"B\" => 1, \"a\" => 1];\n\
ksort($a, SORT_NATURAL | SORT_FLAG_CASE);\n\
foreach ($a as $k => $v) { echo \"[\", ord($k), \"]\"; }\n",
    );
    assert_eq!(out, "[97][66][128][255]");
}

/// Verifies a flagged key sort reaches every receiver shape the backend represents.
///
/// The sort relinks the receiver in place, so the receiver has to be the caller's hash and not
/// a copy of it, whichever expression named it.
#[test]
fn key_sort_flags_reach_every_receiver() {
    let out = compile_and_run(
        r#"<?php
function show(array $a): void {
    foreach ($a as $k => $v) { echo "[", $k, "]"; }
    echo "|";
}
class Box { public array $items = ["img12" => 1, "img10" => 1, "img2" => 1]; }
$o = new Box();
ksort($o->items, SORT_NATURAL);
show($o->items);

$grid = ["inner" => ["img12" => 1, "img10" => 1, "img2" => 1]];
ksort($grid["inner"], SORT_NATURAL);
show($grid["inner"]);

function sortIt(array &$v, int $f): void { ksort($v, $f); }
$byRef = ["img12" => 1, "img10" => 1, "img2" => 1];
sortIt($byRef, SORT_NATURAL);
show($byRef);

$named = ["img12" => 1, "img10" => 1, "img2" => 1];
ksort(array: $named, flags: SORT_NATURAL);
show($named);

$reordered = ["img12" => 1, "img10" => 1, "img2" => 1];
ksort(flags: SORT_NATURAL, array: $reordered);
show($reordered);
"#,
    );
    assert_eq!(out, "[img2][img10][img12]|".repeat(5));
}

/// Verifies a packed receiver still promotes for `krsort()` when a flag is present.
///
/// An indexed array stores its keys as slot positions, so `krsort()` has to promote it to an
/// integer-keyed hash before anything can be reordered. That promotion used to run only when
/// the call had exactly one argument.
#[test]
fn packed_receivers_still_promote_with_a_flag() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
krsort($a, SORT_STRING);
foreach ($a as $k => $v) { echo "[", $k, "=", $v, "]"; }
echo "|";
$b = [10, 20, 30];
ksort($b, SORT_STRING);
foreach ($b as $k => $v) { echo "[", $k, "=", $v, "]"; }
"#,
    );
    assert_eq!(out, "[2=30][1=20][0=10]|[0=10][1=20][2=30]");
}

/// Verifies the flag expression is evaluated exactly once, in source order.
///
/// A named-argument call writes the flag before the receiver, and PHP evaluates what is
/// written in the order it is written.
#[test]
fn the_flag_expression_runs_once_in_source_order() {
    let out = compile_and_run(
        r#"<?php
$log = "";
function flag(string &$log, int $mode): int { $log .= "F"; return $mode; }
function receiver(string &$log, array $a): array { $log .= "A"; return $a; }

$a = ["b" => 1, "a" => 1];
ksort($a, flag($log, SORT_STRING));
echo $log, "|";

$log = "";
$b = ["b" => 1, "a" => 1];
ksort(flags: flag($log, SORT_STRING), array: $b);
echo $log, "|";
foreach ($b as $k => $v) { echo "[", $k, "]"; }
"#,
    );
    assert_eq!(out, "F|F|[a][b]");
}

/// Verifies a flagged key sort permutes links without allocating per comparison.
///
/// Every mode but `SORT_REGULAR` materializes each key as bytes before comparing, and an
/// integer key has to be spelled out to do that. Spelling it into the heap instead of the
/// comparator's own frame would show up here.
#[test]
fn key_sort_with_flags_stays_heap_clean() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$n = 0;
for ($i = 0; $i < 200; $i++) {
    $a = ["img12" => 1, "img10" => 2, "img2" => 3, "IMG1" => 4, 25 => 5, 3 => 6];
    ksort($a, SORT_NATURAL | SORT_FLAG_CASE);
    foreach ($a as $k => $v) { $n += $v; }
    krsort($a, SORT_NUMERIC);
    foreach ($a as $k => $v) { $n += $v; }
    ksort($a, SORT_LOCALE_STRING);
    foreach ($a as $k => $v) { $n += $v; }
}
echo $n;
"#,
    );
    assert_eq!(out.stdout, "12600", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("leak summary: clean"),
        "a flagged key sort must not allocate per comparison: {}",
        out.stderr
    );
}
