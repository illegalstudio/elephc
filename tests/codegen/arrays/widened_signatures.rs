//! Purpose:
//! Integration tests for array builtin parameters that were previously rejected at compile time
//! because elephc's signature was shorter than reference PHP's: `implode($array)`,
//! `array_unshift(&$array, ...$values)`, `array_search(..., $strict)`,
//! `array_reverse($array, $preserve_keys)`, `range($start, $end, $step)`,
//! `array_slice($array, $offset, $length, $preserve_keys)`, and
//! `array_chunk($array, $length, $preserve_keys)`.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expected value is verbatim `LC_ALL=C php` output from PHP 8.4.20.
//! - Each new parameter is exercised BOTH positionally and as a named argument, except
//!   `array_unshift`'s variadic `values`, which reference PHP itself rejects as a named argument
//!   (`array_unshift() does not accept unknown named parameters`).
//! - The `range()` `ValueError` fixtures catch the exception in PHP so the process still exits 0;
//!   they also pin `RuntimeFnId::Range`'s `MAY_THROW` effect, without which dead-code elimination
//!   drops the diagnostic for a call whose result is unused.
//! - The `preserve_keys` results are integer-keyed HASHES, so they are read back with
//!   `foreach ($x as $k => $v)`, `count()` and key lookups rather than `implode()`, which does not
//!   accept an associative array in elephc.
//! - The dynamic-callable fixtures pin the wrapper ABI: the shape-changing `preserve_keys` flag is
//!   dropped from the callable signature, and the typed runtime target must still supply a
//!   concrete container layout where no per-call-site checked type exists.

use crate::support::*;

/// Verifies PHP's one-argument `implode($array)` form joins with an empty separator.
#[test]
fn test_implode_single_array_argument() {
    let out = compile_and_run("<?php echo implode([1, 2]);");
    assert_eq!(out, "12");
}

/// Verifies the one-argument `implode()` form accepts a runtime array variable.
#[test]
fn test_implode_single_array_variable() {
    let out = compile_and_run(r#"<?php $s = ["a", "b", "c"]; echo implode($s);"#);
    assert_eq!(out, "abc");
}

/// Verifies `implode([])` yields the empty string rather than an arity error.
#[test]
fn test_implode_single_empty_array() {
    let out = compile_and_run(r#"<?php echo strlen(implode([])), ":", implode([]), ";";"#);
    assert_eq!(out, "0:;");
}

/// Verifies the two-argument `implode()` form still works through named arguments.
#[test]
fn test_implode_named_arguments() {
    let out = compile_and_run(r#"<?php echo implode(separator: "-", array: [1, 2, 3]);"#);
    assert_eq!(out, "1-2-3");
}

/// Verifies the widened `implode()` signature keeps case-insensitive and namespaced lookup.
#[test]
fn test_implode_case_insensitive_and_namespaced() {
    let out = compile_and_run(r#"<?php echo IMPLODE([7, 8]), ":", \implode("|", [9, 10]);"#);
    assert_eq!(out, "78:9|10");
}

/// Verifies `array_unshift()` prepends two values in source order and returns the new count.
#[test]
fn test_array_unshift_two_values() {
    let out = compile_and_run(
        r#"<?php $a = [3, 4]; $n = array_unshift($a, 1, 2); echo $n, ":", implode(",", $a);"#,
    );
    assert_eq!(out, "4:1,2,3,4");
}

/// Verifies a four-value `array_unshift()` grows the payload before every prepend.
///
/// The source has no spare capacity, so each of the four prepends must re-run the growth check;
/// a single up-front check would write past the allocation on the second value.
#[test]
fn test_array_unshift_four_values_grow() {
    let out = compile_and_run(
        r#"<?php $a = [1, 2, 3]; $n = array_unshift($a, 10, 20, 30, 40); echo $n, ":", implode(",", $a);"#,
    );
    assert_eq!(out, "7:10,20,30,40,1,2,3");
}

/// Verifies a seven-value `array_unshift()` past several doublings stays in bounds.
#[test]
fn test_array_unshift_seven_values_grow() {
    let out = compile_and_run(
        r#"<?php $a = [1, 2, 3, 4]; $n = array_unshift($a, 7, 8, 9, 10, 11, 12, 13); echo $n, ":", implode(",", $a);"#,
    );
    assert_eq!(out, "11:7,8,9,10,11,12,13,1,2,3,4");
}

/// Verifies the value-less `array_unshift($array)` form reports the unchanged count.
#[test]
fn test_array_unshift_without_values() {
    let out = compile_and_run(
        r#"<?php $a = [5, 6]; $n = array_unshift($a); echo $n, ":", implode(",", $a);"#,
    );
    assert_eq!(out, "2:5,6");
}

/// Verifies a multi-value `array_unshift()` splits shared storage before mutating it.
#[test]
fn test_array_unshift_multi_value_copy_on_write() {
    let out = compile_and_run(
        r#"<?php $a = [1, 2]; $b = $a; $n = array_unshift($a, 0, -1); echo $n, ":", implode(",", $a), ":", implode(",", $b);"#,
    );
    assert_eq!(out, "4:0,-1,1,2:1,2");
}

/// Verifies the positional `array_search($needle, $haystack, true)` form.
#[test]
fn test_array_search_strict_positional() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30]; echo var_export(array_search(20, $a, true), true), ":", var_export(array_search(99, $a, true), true);"#,
    );
    assert_eq!(out, "1:false");
}

/// Verifies `array_search()`'s `strict` flag works as a named argument in both positions.
#[test]
fn test_array_search_strict_named() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30]; echo var_export(array_search(needle: 20, haystack: $a, strict: true), true), ":", var_export(array_search(20, $a, strict: false), true);"#,
    );
    assert_eq!(out, "1:1");
}

/// Verifies strict search rejects a cross-type match the loose form still finds.
///
/// `array_search(true, [1, 0], true)` is `false` under `===` while the loose form returns `0`,
/// which is the only case where the two modes diverge for a supported element layout.
#[test]
fn test_array_search_strict_cross_type() {
    let out = compile_and_run(
        r#"<?php $i = [1, 0]; echo var_export(array_search(true, $i, true), true), ":", var_export(array_search(true, $i, false), true);"#,
    );
    assert_eq!(out, "false:0");
}

/// Verifies strict search over an associative haystack still returns the string key.
#[test]
fn test_array_search_strict_assoc() {
    let out = compile_and_run(
        r#"<?php $m = ["a" => 1, "b" => 2]; echo var_export(array_search(2, $m, true), true), ":", var_export(array_search(2, $m, false), true);"#,
    );
    assert_eq!(out, "'b':'b'");
}

/// Verifies a runtime-unknown `strict` flag selects the mode at run time.
#[test]
fn test_array_search_strict_runtime_flag() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30]; $t = $argc > 0; echo var_export(array_search(20, $a, $t), true);"#,
    );
    assert_eq!(out, "1");
}

/// Verifies strict search over a string haystack keeps exact comparison.
#[test]
fn test_array_search_strict_string_haystack() {
    let out = compile_and_run(
        r#"<?php $s = ["x", "y", "z"]; echo var_export(array_search("y", $s, true), true), ":", var_export(array_search("q", $s, true), true);"#,
    );
    assert_eq!(out, "1:false");
}

/// Verifies `array_reverse($array, true)` keeps the source integer keys in reversed order.
#[test]
fn test_array_reverse_preserve_keys_positional() {
    let out = compile_and_run(
        r#"<?php $x = array_reverse([1, 2, 3], true); foreach ($x as $k => $v) { echo $k, "=>", $v, " "; }"#,
    );
    assert_eq!(out, "2=>3 1=>2 0=>1 ");
}

/// Verifies the `preserve_keys:` named argument and the resulting key-addressable lookups.
#[test]
fn test_array_reverse_preserve_keys_named() {
    let out = compile_and_run(
        r#"<?php $x = array_reverse([10, 20, 30], preserve_keys: true); foreach ($x as $k => $v) { echo $k, "=>", $v, " "; } echo "|", $x[0], ",", $x[2];"#,
    );
    assert_eq!(out, "2=>30 1=>20 0=>10 |10,30");
}

/// Verifies an explicit `false` flag keeps the renumbered indexed-array result.
#[test]
fn test_array_reverse_preserve_keys_false() {
    let out = compile_and_run(r#"<?php echo implode(",", array_reverse([1, 2, 3], false));"#);
    assert_eq!(out, "3,2,1");
}

/// Verifies the key-preserving reversal of an empty array produces an empty result.
#[test]
fn test_array_reverse_preserve_keys_empty() {
    let out = compile_and_run("<?php $x = array_reverse([], true); echo count($x);");
    assert_eq!(out, "0");
}

/// Verifies key-preserving reversal persists string payloads instead of aliasing the source.
#[test]
fn test_array_reverse_preserve_keys_strings() {
    let out = compile_and_run(
        r#"<?php $s = ["p", "q", "r"]; $x = array_reverse($s, true); foreach ($x as $k => $v) { echo $k, "=>", $v, " "; } echo "|", implode(",", $s);"#,
    );
    assert_eq!(out, "2=>r 1=>q 0=>p |p,q,r");
}

/// Verifies the positional `range($start, $end, $step)` form for ascending ranges.
#[test]
fn test_range_step_positional() {
    let out = compile_and_run(
        r#"<?php echo implode(",", range(1, 10, 2)), ":", implode(",", range(1, 10, 3));"#,
    );
    assert_eq!(out, "1,3,5,7,9:1,4,7,10");
}

/// Verifies `range()`'s `step:` named argument, alone and alongside named endpoints.
#[test]
fn test_range_step_named() {
    let out = compile_and_run(
        r#"<?php echo implode(",", range(start: 1, end: 7, step: 3)), ":", implode(",", range(1, 7, step: 2));"#,
    );
    assert_eq!(out, "1,4,7:1,3,5,7");
}

/// Verifies a descending range accepts either step sign, as reference PHP does.
#[test]
fn test_range_step_descending() {
    let out = compile_and_run(
        r#"<?php echo implode(",", range(10, 1, 2)), ":", implode(",", range(5, 1, -2)), ":", implode(",", range(10, -5, 4));"#,
    );
    assert_eq!(out, "10,8,6,4,2:5,3,1:10,6,2,-2");
}

/// Verifies the step magnitude boundaries: a step equal to the span, and degenerate ranges.
///
/// `range(1, 3, 2)` is accepted (step == span) while `range(1, 3, 3)` is a `ValueError`, and
/// `range(5, 5, 3)` yields `[5]` because an equal-endpoint range ignores the step magnitude.
#[test]
fn test_range_step_boundaries() {
    let out = compile_and_run(
        r#"<?php echo implode(",", range(1, 3, 2)), ":", implode(",", range(5, 5, 3)), ":", implode(",", range(1, 1, 1));"#,
    );
    assert_eq!(out, "1,3:5:1");
}

/// Verifies a runtime-unknown step still produces PHP's element sequence.
#[test]
fn test_range_step_runtime_value() {
    let out = compile_and_run(r#"<?php $s = 2; echo implode(",", range(1, 9, $s));"#);
    assert_eq!(out, "1,3,5,7,9");
}

/// Verifies a zero `range()` step raises PHP's catchable `ValueError`.
#[test]
fn test_range_step_zero_value_error() {
    let out = compile_and_run(
        r#"<?php $z = 0; try { range(1, 5, $z); echo "no"; } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(); }"#,
    );
    assert_eq!(out, "ValueError: range(): Argument #3 ($step) cannot be 0");
}

/// Verifies a negative `range()` step on an increasing range raises PHP's `ValueError`.
#[test]
fn test_range_step_negative_value_error() {
    let out = compile_and_run(
        r#"<?php $n = -2; try { range(1, 5, $n); echo "no"; } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(); }"#,
    );
    assert_eq!(
        out,
        "ValueError: range(): Argument #3 ($step) must be greater than 0 for increasing ranges"
    );
}

/// Verifies a `range()` step wider than the spanned interval raises PHP's `ValueError`.
#[test]
fn test_range_step_too_wide_value_error() {
    let out = compile_and_run(
        r#"<?php $w = 10; try { range(1, 3, $w); echo "no"; } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(); }"#,
    );
    assert_eq!(
        out,
        "ValueError: range(): Argument #3 ($step) must be less than the range spanned by argument #1 ($start) and argument #2 ($end)"
    );
}

/// Verifies `array_slice($array, $offset, $length, true)` keeps the source integer keys.
#[test]
fn test_array_slice_preserve_keys_positional() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; $x = array_slice($a, 1, 3, true); foreach ($x as $k => $v) { echo $k, "=>", $v, " "; }"#,
    );
    assert_eq!(out, "1=>20 2=>30 3=>40 ");
}

/// Verifies the fully named `array_slice()` call form, including `preserve_keys:`.
#[test]
fn test_array_slice_preserve_keys_named() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; $n = array_slice($a, offset: 1, length: 2, preserve_keys: true); foreach ($n as $k => $v) { echo $k, "=>", $v, " "; }"#,
    );
    assert_eq!(out, "1=>20 2=>30 ");
}

/// Verifies a `preserve_keys:` named argument that skips the optional `$length` slot.
///
/// The argument planner has to fill the gap with `$length`'s `null` default, so the runtime
/// helper still receives its four-value argument tuple and takes every remaining element.
#[test]
fn test_array_slice_preserve_keys_named_skipping_length() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; $g = array_slice($a, 1, preserve_keys: true); foreach ($g as $k => $v) { echo $k, "=>", $v, " "; }"#,
    );
    assert_eq!(out, "1=>20 2=>30 3=>40 4=>50 ");
}

/// Verifies a negative `array_slice()` offset with an explicit `null` length keeps the tail keys.
#[test]
fn test_array_slice_preserve_keys_negative_offset() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; $b = array_slice($a, -2, null, true); foreach ($b as $k => $v) { echo $k, "=>", $v, " "; }"#,
    );
    assert_eq!(out, "3=>40 4=>50 ");
}

/// Verifies a negative `array_slice()` length stops before the end while keeping the keys.
///
/// This pins the shared `emit_slice_bounds` window arithmetic on the key-preserving helper: a
/// negative length counts back from the SOURCE end, not from the offset.
#[test]
fn test_array_slice_preserve_keys_negative_length() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; $c = array_slice($a, 1, -1, true); foreach ($c as $k => $v) { echo $k, "=>", $v, " "; }"#,
    );
    assert_eq!(out, "1=>20 2=>30 3=>40 ");
}

/// Verifies the key-preserving slice clamps out-of-range windows to an empty result.
///
/// A backward length that consumes more than the window holds, and an offset past the end, must
/// both yield zero elements rather than a negative window that would read outside the payload.
#[test]
fn test_array_slice_preserve_keys_empty_windows() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; echo count(array_slice($a, 2, -4, true)), ":", count(array_slice($a, 10, 2, true)), ":", count(array_slice([], 0, 2, true));"#,
    );
    assert_eq!(out, "0:0:0");
}

/// Verifies an `array_slice()` offset before the start clamps to the first element.
#[test]
fn test_array_slice_preserve_keys_offset_before_start() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; $d = array_slice($a, -10, 2, true); foreach ($d as $k => $v) { echo $k, "=>", $v, " "; }"#,
    );
    assert_eq!(out, "0=>10 1=>20 ");
}

/// Verifies an explicit `false` flag keeps the renumbered indexed-array result.
#[test]
fn test_array_slice_preserve_keys_false() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; echo implode(",", array_slice($a, 1, 3, false));"#,
    );
    assert_eq!(out, "20,30,40");
}

/// Verifies the key-preserving slice owns the refcounted elements it copies.
///
/// `__rt_array_slice_to_hash` retains every heap-backed element it copies, but on x86_64 it
/// handed `__rt_incref` the pointer in the wrong register, so nothing was retained: replacing
/// the first result released arrays the source still held, which crashed or printed garbage.
#[test]
fn test_array_slice_preserve_keys_retains_refcounted_elements() {
    let out = compile_and_run(
        r#"<?php
$i = 1;
$nested = [[$i], [2], [3]];
$a = array_slice($nested, 1, null, true);
$a = array_slice($nested, 1, null, true);
echo json_encode($a), "|", json_encode($nested), "|";
$mixed = ["p" . $i, 2, [3]];
$b = array_slice($mixed, 1, 2, true);
$b = array_slice($mixed, 1, 2, true);
echo json_encode($b), "|", json_encode($mixed), "\n";
"#,
    );
    assert_eq!(
        out,
        "{\"1\":[2],\"2\":[3]}|[[1],[2],[3]]|{\"1\":2,\"2\":[3]}|[\"p1\",2,[3]]\n"
    );
}

/// Verifies the key-preserving slice stays key-addressable and reports the window's count.
#[test]
fn test_array_slice_preserve_keys_lookup() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; $y = array_slice($a, 1, 3, true); echo $y[1], ",", $y[3], ",", count($y);"#,
    );
    assert_eq!(out, "20,40,3");
}

/// Verifies the key-preserving slice retains heterogeneous and float payloads.
#[test]
fn test_array_slice_preserve_keys_mixed_and_float_payloads() {
    let out = compile_and_run(
        r#"<?php $m = [1, "two", 3.5]; $x = array_slice($m, 1, 2, true); foreach ($x as $k => $v) { echo $k, "=>", $v, " "; } echo "|"; $f = [1.5, 2.5, 3.5, 4.5]; $z = array_slice($f, 2, 2, true); foreach ($z as $k => $v) { echo $k, "=>", $v, " "; }"#,
    );
    assert_eq!(out, "1=>two 2=>3.5 |2=>3.5 3=>4.5 ");
}

/// Verifies dynamic `array_slice` dispatch without a flag renumbers like the direct call.
///
/// The callable wrapper keeps `$preserve_keys` as a runtime parameter; when the caller omits it,
/// both callable spellings must keep working exactly like the three-argument direct call.
#[test]
fn test_array_slice_dynamic_callable_dispatch() {
    let out = compile_and_run(
        r#"<?php $f = "array_slice"; echo implode(",", $f([1, 2, 3, 4], 1, 2)), "|"; $g = array_slice(...); echo implode(",", $g([5, 6, 7, 8], 2));"#,
    );
    assert_eq!(out, "2,3|7,8");
}

/// Verifies dynamic `array_reverse` dispatch returns a usable array instead of crashing.
///
/// The declared `mixed` return type used to reach the backend for callable dispatch, which
/// stored the raw array pointer into a boxed-Mixed slot and faulted when it was read back.
#[test]
fn test_array_reverse_dynamic_callable_dispatch() {
    let out = compile_and_run(
        r#"<?php $f = "array_reverse"; echo implode(",", $f([1, 2, 3, 4]));"#,
    );
    assert_eq!(out, "4,3,2,1");
}

/// Verifies `array_chunk($array, $length, true)` keeps each chunk's source integer keys.
#[test]
fn test_array_chunk_preserve_keys_positional() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; foreach (array_chunk($a, 2, true) as $ci => $chunk) { echo "[", $ci, "]"; foreach ($chunk as $k => $v) { echo " ", $k, "=>", $v; } echo "|"; }"#,
    );
    assert_eq!(out, "[0] 0=>10 1=>20|[1] 2=>30 3=>40|[2] 4=>50|");
}

/// Verifies the `preserve_keys:` named argument on an unevenly divided source.
#[test]
fn test_array_chunk_preserve_keys_named() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; foreach (array_chunk($a, 3, preserve_keys: true) as $ci => $chunk) { echo "[", $ci, "]"; foreach ($chunk as $k => $v) { echo " ", $k, "=>", $v; } echo "|"; }"#,
    );
    assert_eq!(out, "[0] 0=>10 1=>20 2=>30|[1] 3=>40 4=>50|");
}

/// Verifies a fully named `array_chunk()` call whose chunk size exceeds the source length.
#[test]
fn test_array_chunk_preserve_keys_all_named_oversized_length() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; foreach (array_chunk(array: $a, length: 10, preserve_keys: true) as $ci => $chunk) { echo "[", $ci, "]"; foreach ($chunk as $k => $v) { echo " ", $k, "=>", $v; } echo "|"; }"#,
    );
    assert_eq!(out, "[0] 0=>10 1=>20 2=>30 3=>40 4=>50|");
}

/// Verifies key-preserving chunks stay countable and key-addressable from the outer array.
#[test]
fn test_array_chunk_preserve_keys_nested_lookup() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; $c = array_chunk($a, 2, true); echo count($c), ":", count($c[1]), ":", $c[1][2], ":", $c[2][4];"#,
    );
    assert_eq!(out, "3:2:30:50");
}

/// Verifies chunking an empty array with `preserve_keys` produces no chunks.
#[test]
fn test_array_chunk_preserve_keys_empty() {
    let out = compile_and_run("<?php echo count(array_chunk([], 2, true));");
    assert_eq!(out, "0");
}

/// Verifies key-preserving chunks retain heterogeneous payloads.
#[test]
fn test_array_chunk_preserve_keys_mixed_payloads() {
    let out = compile_and_run(
        r#"<?php $m = [1, "two", 3.5, true]; foreach (array_chunk($m, 2, true) as $ci => $chunk) { echo "[", $ci, "]"; foreach ($chunk as $k => $v) { echo " ", $k, "=>", $v; } echo "|"; }"#,
    );
    assert_eq!(out, "[0] 0=>1 1=>two|[1] 2=>3.5 3=>1|");
}

/// Verifies an explicit `false` flag keeps the renumbered nested indexed arrays.
#[test]
fn test_array_chunk_preserve_keys_false() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30, 40, 50]; foreach (array_chunk($a, 2, false) as $ci => $chunk) { echo "[", $ci, "]"; foreach ($chunk as $k => $v) { echo " ", $k, "=>", $v; } echo "|"; }"#,
    );
    assert_eq!(out, "[0] 0=>10 1=>20|[1] 0=>30 1=>40|[2] 0=>50|");
}

/// Verifies the key-preserving chunk form still raises PHP's non-positive `$length` `ValueError`.
#[test]
fn test_array_chunk_preserve_keys_zero_length_value_error() {
    let out = compile_and_run(
        r#"<?php $a = [10, 20, 30]; try { array_chunk($a, 0, true); echo "no"; } catch (ValueError $e) { echo get_class($e), ": ", $e->getMessage(); }"#,
    );
    assert_eq!(
        out,
        "ValueError: array_chunk(): Argument #2 ($length) must be greater than 0"
    );
}

/// Verifies dynamic `array_chunk` dispatch produces the renumbered nested arrays.
///
/// The callable ABI drops the shape-changing `$preserve_keys` flag, and the typed runtime target
/// supplies the concrete `array<array<T>>` layout that a wrapper has no checked type for.
#[test]
fn test_array_chunk_dynamic_callable_dispatch() {
    let out = compile_and_run(
        r#"<?php $f = "array_chunk"; $c = $f([1, 2, 3, 4], 2); echo count($c), ":", $c[0][1], ":", $c[1][0];"#,
    );
    assert_eq!(out, "2:2:3");
}
