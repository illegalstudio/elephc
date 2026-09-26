//! Purpose:
//! Integration tests for PHP's internal array pointer family: `key`, `current`, `next`,
//! `prev`, `reset` and `end`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries; assertions compare stdout.
//! - Every expected value in this file was produced by running the same fixture under
//!   `LC_ALL=C php` (PHP 8.4.20), not derived from the implementation.
//! - Coverage spans indexed arrays (`O(1)` ordinal reads) and associative hashes
//!   (insertion-order chain walk), the single unrecoverable invalid cursor, empty
//!   containers, `foreach` leaving the pointer alone, reassignment rewinding it, and
//!   case-insensitive/namespaced spellings.

use crate::support::*;

/// Verifies the full read/seek cycle over an indexed array, including the one-way invalid
/// cursor: after `end()` + `next()` runs off the back, `prev()` does NOT walk back in.
/// Fixture: a three-element indexed array driven through every member of the family.
#[test]
fn test_array_internal_pointer_indexed_cycle() {
    let out = compile_and_run(
        r#"<?php
$a = [10, 20, 30];
echo current($a), "|", key($a), "|";
echo next($a), "|", key($a), "|";
echo end($a), "|", key($a), "|";
var_dump(next($a)); var_dump(key($a));
var_dump(prev($a));
echo reset($a), "|", key($a), "|";
var_dump(prev($a)); var_dump(key($a));
"#,
    );
    assert_eq!(
        out,
        "10|0|20|1|30|2|bool(false)\nNULL\nbool(false)\n10|0|bool(false)\nNULL\n"
    );
}

/// Verifies the family walks an associative hash in insertion order and reports string keys.
/// Fixture: a three-entry string-keyed hash; `prev()` from the first entry invalidates.
#[test]
fn test_array_internal_pointer_assoc_cycle() {
    let out = compile_and_run(
        r#"<?php
$a = ["x" => 1, "y" => 2, "z" => 3];
echo current($a), key($a), next($a), key($a), end($a), key($a), reset($a), key($a);
var_dump(prev($a)); var_dump(key($a));
"#,
    );
    assert_eq!(out, "1x2y3z1xbool(false)\nNULL\n");
}

/// Verifies every family member reports the empty position on an empty array.
/// Fixture: `[]`, where `key()` yields null and the other five yield false.
#[test]
fn test_array_internal_pointer_empty_array() {
    let out = compile_and_run(
        r#"<?php
$e = [];
var_dump(current($e), key($e), next($e), prev($e), reset($e), end($e));
"#,
    );
    assert_eq!(
        out,
        "bool(false)\nNULL\nbool(false)\nbool(false)\nbool(false)\nbool(false)\n"
    );
}

/// Verifies the canonical `while (current(...) !== false) { ...; next(...); }` traversal
/// runs to completion for both storage kinds — the cursor must survive loop iterations
/// rather than being re-seeded on each one.
/// Fixture: a four-element indexed array and a three-entry hash.
#[test]
fn test_array_internal_pointer_while_traversal() {
    let out = compile_and_run(
        r#"<?php
$w = [1, 2, 3, 4];
reset($w);
while (($v = current($w)) !== false) { echo key($w), "=>", $v, " "; next($w); }
echo "|";
$h = ["a"=>1,"b"=>2,"c"=>3];
reset($h);
while (($v = current($h)) !== false) { echo key($h), "=>", $v, " "; next($h); }
"#,
    );
    assert_eq!(out, "0=>1 1=>2 2=>3 3=>4 |a=>1 b=>2 c=>3 ");
}

/// Verifies `foreach` does not move the internal pointer, by value or by reference.
/// PHP 7+ iterates an internal copy, so the pointer must sit where `next()` left it.
/// Fixture: pointer advanced to key 1, then two full `foreach` passes.
#[test]
fn test_array_internal_pointer_foreach_does_not_move_it() {
    let out = compile_and_run(
        r#"<?php
$f = [10, 20, 30];
next($f);
foreach ($f as $k => $vv) {}
var_dump(key($f));
foreach ($f as &$r) {}
unset($r);
var_dump(key($f));
"#,
    );
    assert_eq!(out, "int(1)\nint(1)\n");
}

/// Verifies binding the variable to a different array rewinds its pointer, because PHP's
/// pointer belongs to the hashtable rather than to the variable.
/// Fixture: advance the pointer, then assign a fresh array over the same local.
#[test]
fn test_array_internal_pointer_reassignment_rewinds() {
    let out = compile_and_run(
        r#"<?php
$r = [1,2,3]; next($r); $r = [4,5,6]; echo key($r), "|", current($r);
"#,
    );
    assert_eq!(out, "0|4");
}

/// Verifies string elements are returned (and persisted) correctly by the seek family.
/// Fixture: a three-element string array walked off the end.
#[test]
fn test_array_internal_pointer_string_elements() {
    let out = compile_and_run(
        r#"<?php
$t = ["a", "bb", "ccc"];
var_dump(reset($t), next($t), next($t), next($t));
"#,
    );
    assert_eq!(
        out,
        "string(1) \"a\"\nstring(2) \"bb\"\nstring(3) \"ccc\"\nbool(false)\n"
    );
}

/// Verifies `current()` preserves each element's runtime type in a heterogeneous array.
/// Fixture: int, string, float, bool and null in one array, read one position at a time.
#[test]
fn test_array_internal_pointer_heterogeneous_values() {
    let out = compile_and_run(
        r#"<?php
$m = [1, "two", 3.5, true, null];
reset($m);
var_dump(current($m)); next($m);
var_dump(current($m)); next($m);
var_dump(current($m)); next($m);
var_dump(current($m)); next($m);
var_dump(current($m));
"#,
    );
    assert_eq!(
        out,
        "int(1)\nstring(3) \"two\"\nfloat(3.5)\nbool(true)\nNULL\n"
    );
}

/// Verifies `key()` reports the real integer keys of a sparse-looking integer-keyed hash
/// rather than the cursor ordinal.
/// Fixture: a hash keyed `5` and `9`.
#[test]
fn test_array_internal_pointer_integer_hash_keys() {
    let out = compile_and_run(
        r#"<?php
$a = [5 => "a", 9 => "b"];
var_dump(key($a)); next($a); var_dump(key($a));
"#,
    );
    assert_eq!(out, "int(5)\nint(9)\n");
}

/// Verifies the family resolves case-insensitively and through a root-namespace prefix,
/// like every other PHP-visible builtin.
/// Fixture: `CURRENT`, `\key`, `Next`, `RESET` and `End` on one array.
#[test]
fn test_array_internal_pointer_case_insensitive_and_namespaced() {
    let out = compile_and_run(
        r#"<?php
$a = [7, 8];
echo CURRENT($a), "|", \key($a), "|", Next($a), "|", RESET($a), "|", End($a);
"#,
    );
    assert_eq!(out, "7|0|8|7|8");
}

/// Verifies `function_exists()` now reports the whole family, since they are registry
/// builtins rather than unknown names.
/// Fixture: a `function_exists()` probe per name.
#[test]
fn test_array_internal_pointer_function_exists() {
    let out = compile_and_run(
        r#"<?php
foreach (["key","current","next","prev","reset","end"] as $f) {
    echo function_exists($f) ? "y" : "n";
}
"#,
    );
    assert_eq!(out, "yyyyyy");
}

/// Verifies two array locals keep independent cursors, so moving one does not disturb the
/// other — each variable owns its own hidden cursor slot.
/// Fixture: two arrays advanced by different amounts.
#[test]
fn test_array_internal_pointer_independent_per_variable() {
    let out = compile_and_run(
        r#"<?php
$a = [1,2,3];
$b = [4,5,6];
next($a);
next($b); next($b);
echo key($a), "|", key($b), "|", current($a), "|", current($b);
"#,
    );
    assert_eq!(out, "1|2|2|6");
}

/// Verifies an array built INSIDE a loop body gets its pointer rewound on every iteration.
///
/// Regression: the rewind lives in the local store, but the cursor slot is created lazily
/// at the first pointer call, which the loop body lowers AFTER the store. Without the
/// loop pre-declaration pass the second iteration inherited the first iteration's cursor
/// and printed `18` where PHP prints `07`.
/// Fixture: two iterations that rebuild `$z` and then advance its pointer.
#[test]
fn test_array_internal_pointer_loop_local_rewinds_each_iteration() {
    let out = compile_and_run(
        r#"<?php
for ($i = 0; $i < 2; $i++) {
    $z = [7, 8];
    echo key($z), current($z), " ";
    next($z);
    echo key($z), current($z), " ";
}
"#,
    );
    assert_eq!(out, "07 18 07 18 ");
}

/// Verifies the family works inside a user function and that a second call to the same
/// function starts from a fresh pointer, since the cursor is a frame slot.
/// Fixture: a helper that walks a locally-built array, called twice.
#[test]
fn test_array_internal_pointer_inside_function_is_per_call() {
    let out = compile_and_run(
        r#"<?php
function walk(): string {
    $a = [1, 2, 3];
    $out = "";
    reset($a);
    while (($v = current($a)) !== false) { $out .= key($a) . ":" . $v . " "; next($a); }
    return $out;
}
echo walk(), "|", walk();
"#,
    );
    assert_eq!(out, "0:1 1:2 2:3 |0:1 1:2 2:3 ");
}

/// Verifies `current()` returns a nested array element intact, exercising the boxed
/// container retain path in `__rt_mixed_from_value`.
/// Fixture: an array of arrays walked one position forward.
#[test]
fn test_array_internal_pointer_nested_array_values() {
    let out = compile_and_run(
        r#"<?php
$n = [[1,2],[3,4]];
$x = current($n);
echo $x[0], $x[1], "|";
next($n);
$y = current($n);
echo $y[0], $y[1];
"#,
    );
    assert_eq!(out, "12|34");
}

/// Verifies the family also works inside `eval()`, so the compiled registry and the
/// magician interpreter agree on the pointer semantics.
/// Fixture: an indexed walk and an associative read, both executed through `eval()`.
#[test]
fn test_array_internal_pointer_through_eval() {
    let out = compile_and_run(
        r#"<?php
eval('$a = [10, 20, 30]; echo current($a), "|", key($a), "|"; echo next($a), "|", key($a), "|"; echo end($a), "|", key($a), "|"; var_dump(next($a)); var_dump(key($a)); echo reset($a), "|", key($a);');
echo "\n";
eval('$h = ["x"=>1,"y"=>2]; echo current($h), key($h); next($h); echo current($h), key($h);');
"#,
    );
    assert_eq!(out, "10|0|20|1|30|2|bool(false)\nNULL\n10|0\n1x2y");
}


/// The whole pointer family over a hash whose VALUES are boxed `Mixed` cells.
///
/// A hash entry carries its own `value_tag`, and `__rt_array_ptr_value` handed it straight to
/// `__rt_mixed_from_value`. Tag 7 says the payload IS a Mixed cell, and that helper's tag-7 arm
/// retains it and allocates ANOTHER cell around it — a Mixed holding a Mixed, which no display
/// path unwraps twice, so every one of these printed nothing while `key()` stayed correct.
///
/// Checked arithmetic is what boxes the values here (`2 * $n` can overflow to float), which is
/// the ordinary way an `array<string, mixed>` appears. Raw int and string payloads were always
/// right, and so was an INDEXED array of boxed cells — it delegates to `__rt_array_get_mixed_key`,
/// which understands every `value_type`. Only the hash branch boxed blind.
#[test]
fn test_pointer_family_over_a_hash_of_boxed_mixed_values() {
    let out = compile_and_run(
        r#"<?php
$n = 2;
$h = ["b" => 2 * $n, "a" => 1 * $n];
echo current($h), "|", key($h), "|";
next($h);
echo current($h), "|", key($h), "|";
reset($h);
echo current($h), "|", key($h), "|";
echo end($h), "|", key($h);
"#,
    );
    assert_eq!(out, "4|b|2|a|4|b|2|a");
}

/// The entry's own cell is returned with a retain instead of a fresh wrapper, so the caller's
/// release has to balance it. Five hundred reads would leak five hundred cells if it did not —
/// and the boxed case now allocates FEWER blocks than the raw one, because the wrapper is gone.
#[test]
fn test_repeated_pointer_reads_over_boxed_values_do_not_leak() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$n = 2;
$h = ["b" => 2 * $n, "a" => 1 * $n];
$t = 0;
for ($i = 0; $i < 500; $i++) { reset($h); $t += current($h); next($h); $t += current($h); }
echo $t;
"#,
    );
    assert_eq!(out.stdout, "3000", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}

/// An entry held by a PHP reference is dereferenced to the reference's boxed Mixed cell, which
/// must be returned with a retain like any boxed entry: boxing it again nested a Mixed in a
/// Mixed, so an array reached through `next()` counted 0 and indexed as null. PHP 8.5.10 prints
/// `array same 1 8`; the heap must stay balanced across the repeated reads.
#[test]
fn test_pointer_reads_over_referenced_entries_return_the_value() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
$m = ['k' => ($argc > 5 ? 'str' : 42), 'j' => ($argc > 5 ? 1.5 : [7])];
$r4 = &$m['k'];
$r5 = &$m['j'];
for ($i = 0; $i < 20; $i++) {
    $c = current($m);
    $n = next($m);
    $e = end($m);
    reset($m);
}
echo gettype($c), " ", $c === 42 ? "same" : "diff", "|";
echo gettype($n), " ", $n === [7] ? "same" : "diff", " ", count($n), " ", $n[0] + 1, "|";
echo $e === $n ? "same" : "diff", "|";
// The returned value now shares the reference's own cell, so a write to it must separate:
// neither the array nor the reference sees the append.
$n[] = 1;
echo count($n), " ", count($m['j']), " ", count($r5), "|";
reset($m); $last = end($m); $last[9] = 2;
echo count($last), " ", count($m['j']);
"#,
    );
    assert_eq!(out.stdout, "integer same|array same 1 8|same|2 1 1|2 1", "stderr: {}", out.stderr);
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected clean heap, got: {}",
        out.stderr
    );
}
