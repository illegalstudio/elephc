//! Purpose:
//! Integration or regression tests for diagnostic coverage of array builtins, including array mixed type checks, array union operand checks, and indexed array union compatible element types.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

// Verifies that a heterogeneous associative array with string and integer values widens to `mixed` without error.
/// Verifies that assoc array mixed type checks.
#[test]
fn test_assoc_array_mixed_type_checks() {
    assert!(
        check_source(r#"<?php $a = ["name" => "Alice", "age" => 30];"#).is_ok(),
        "heterogeneous associative-array values should widen to mixed",
    );
}

/// Verifies a negative literal `array_fill()` count stays a runtime `ValueError`, not a diagnostic.
///
/// PHP reports it as a catchable `ValueError` at run time, so the checker must keep accepting the
/// call; the codegen guard covered by `test_array_fill_negative_count_string_value` owns the
/// rejection.
#[test]
fn test_array_fill_negative_count_is_not_a_compile_error() {
    assert!(
        check_source(r#"<?php $a = array_fill(0, -1, "x");"#).is_ok(),
        "array_fill() with a negative count must type-check and fail at run time",
    );
}

// Regression test: array union with a non-array right operand produces a type error.
/// Verifies that error array union requires array operands.
#[test]
fn test_error_array_union_requires_array_operands() {
    expect_error(
        r#"<?php $result = [1, 2] + 3;"#,
        "Array union requires both operands to be arrays",
    );
}

// Regression test: indexed array union with mismatched element types (int vs string) produces a type error.
/// Verifies that error indexed array union requires compatible element types.
#[test]
fn test_error_indexed_array_union_requires_compatible_element_types() {
    expect_error(
        r#"<?php $result = [1] + ["right", "side"];"#,
        "Array union requires compatible indexed array element types",
    );
}

// --- v0.6: array function argument errors ---

/// Verifies that error array reverse wrong args.
///
/// PHP's signature is `array_reverse(array $array, bool $preserve_keys = false)`, so a no-argument
/// call is short by one and a three-argument call is one too many.
#[test]
fn test_error_array_reverse_wrong_args() {
    expect_error(
        "<?php array_reverse();",
        "array_reverse() takes 1 or 2 arguments",
    );
    expect_error(
        "<?php array_reverse([1], true, 1);",
        "array_reverse() takes 1 or 2 arguments",
    );
}

/// Verifies `array_reverse()` rejects a non-literal `preserve_keys` flag in AOT mode.
///
/// The flag decides the result's static shape (indexed array vs integer-keyed hash), so it
/// cannot be resolved at run time the way `in_array()`'s `strict` flag can.
#[test]
fn test_error_array_reverse_non_literal_preserve_keys() {
    expect_error(
        "<?php $t = $argc > 0; array_reverse([1, 2], $t);",
        "array_reverse() preserve_keys argument must be a literal bool in AOT mode",
    );
}

/// Verifies `array_chunk()` rejects a non-literal `preserve_keys` flag in AOT mode.
///
/// The flag decides whether each chunk is a renumbered indexed array or an integer-keyed hash,
/// so it cannot be resolved at run time.
#[test]
fn test_error_array_chunk_non_literal_preserve_keys() {
    expect_error(
        "<?php $t = $argc > 0; array_chunk([1, 2], 1, $t);",
        "array_chunk() preserve_keys argument must be a literal bool in AOT mode",
    );
}

/// Verifies `array_chunk()` reports PHP's full 2-to-3 argument range.
#[test]
fn test_error_array_chunk_wrong_args() {
    expect_error(
        "<?php array_chunk([1]);",
        "array_chunk() takes 2 or 3 arguments",
    );
    expect_error(
        "<?php array_chunk([1], 1, true, 5);",
        "array_chunk() takes 2 or 3 arguments",
    );
}

/// Verifies `array_slice()` reports PHP's full 2-to-4 argument range.
#[test]
fn test_error_array_slice_wrong_args() {
    expect_error(
        "<?php array_slice([1]);",
        "array_slice() takes 2 to 4 arguments",
    );
    expect_error(
        "<?php array_slice([1], 1, 2, true, 5);",
        "array_slice() takes 2 to 4 arguments",
    );
}

/// Verifies `array_slice()` rejects a non-literal `preserve_keys` flag in AOT mode.
///
/// The flag decides the result's static shape (renumbered indexed array vs integer-keyed hash),
/// exactly like `array_reverse()`'s flag, so it cannot be resolved at run time.
#[test]
fn test_error_array_slice_non_literal_preserve_keys() {
    expect_error(
        "<?php $t = $argc > 0; array_slice([1, 2], 0, 1, $t);",
        "array_slice() preserve_keys argument must be a literal bool in AOT mode",
    );
}

/// Verifies a key-preserving `array_slice()` of a boxed array is rejected, not miscompiled.
///
/// The key-preserving helper copies the source header's `value_type` into the result hash, so
/// the element layout has to be known statically.
#[test]
fn test_error_array_slice_preserve_keys_boxed_source() {
    expect_error(
        r#"<?php $m = json_decode("[1,2,3]"); array_slice($m, 1, 2, true);"#,
        "array_slice() preserve_keys requires a statically known array type",
    );
}

/// Verifies that error array merge wrong args.
#[test]
fn test_error_array_merge_wrong_args() {
    expect_error(
        "<?php $a = [1]; array_merge($a);",
        "array_merge() takes exactly 2 arguments",
    );
}

/// Verifies that error array sum wrong args.
#[test]
fn test_error_array_sum_wrong_args() {
    expect_error("<?php array_sum();", "array_sum() takes exactly 1 argument");
}

/// Verifies that error array search wrong args.
///
/// PHP's signature is `array_search(mixed $needle, array $haystack, bool $strict = false)`, so a
/// one-argument call is short by one and a four-argument call is one too many.
#[test]
fn test_error_array_search_wrong_args() {
    expect_error(
        "<?php $a = [1]; array_search($a);",
        "array_search() takes 2 or 3 arguments",
    );
    expect_error(
        "<?php $a = [1]; array_search(1, $a, true, 1);",
        "array_search() takes 2 or 3 arguments",
    );
}

/// Verifies that error array key exists wrong args.
#[test]
fn test_error_array_key_exists_wrong_args() {
    expect_error(
        "<?php array_key_exists(1);",
        "array_key_exists() takes exactly 2 arguments",
    );
}

/// Verifies that error array slice wrong args.
#[test]
fn test_error_array_slice_too_few_args() {
    expect_error(
        "<?php $a = [1]; array_slice($a);",
        "array_slice() takes 2 to 4 arguments",
    );
}

/// Verifies that error array combine wrong args.
#[test]
fn test_error_array_combine_wrong_args() {
    expect_error(
        "<?php $a = [1]; array_combine($a);",
        "array_combine() takes exactly 2 arguments",
    );
}

/// Verifies that error range wrong args.
///
/// PHP's signature is `range($start, $end, int|float $step = 1)`, so a one-argument call is short
/// by one and a four-argument call is one too many.
#[test]
fn test_error_range_wrong_args() {
    expect_error("<?php range(1);", "range() takes 2 or 3 arguments");
    expect_error("<?php range(1, 5, 2, 3);", "range() takes 2 or 3 arguments");
}

/// Verifies that error shuffle wrong args.
#[test]
fn test_error_shuffle_wrong_args() {
    expect_error("<?php shuffle();", "shuffle() takes exactly 1 argument");
}

/// Verifies that error array fill wrong args.
#[test]
fn test_error_array_fill_wrong_args() {
    expect_error(
        "<?php array_fill(0, 5);",
        "array_fill() takes exactly 3 arguments",
    );
}

/// Verifies that error array push wrong args.
#[test]
fn test_error_array_push_wrong_args() {
    expect_error(
        "<?php array_push();",
        "array_push() takes exactly 2 arguments",
    );
}

/// Verifies that error array pop wrong args.
#[test]
fn test_error_array_pop_wrong_args() {
    expect_error("<?php array_pop();", "array_pop() takes exactly 1 argument");
}

/// Verifies that error in array wrong args.
#[test]
fn test_error_in_array_wrong_args() {
    expect_error("<?php in_array(1);", "in_array() takes 2 or 3 arguments");
}

/// Verifies that error array keys wrong args.
#[test]
fn test_error_array_keys_wrong_args() {
    expect_error(
        "<?php array_keys();",
        "array_keys() takes exactly 1 argument",
    );
}

/// Verifies that error array values wrong args.
#[test]
fn test_error_array_values_wrong_args() {
    expect_error(
        "<?php array_values();",
        "array_values() takes exactly 1 argument",
    );
}

/// Verifies that error sort wrong args.
#[test]
fn test_error_sort_wrong_args() {
    expect_error("<?php sort();", "sort() takes exactly 1 argument");
}

/// Verifies that error rsort wrong args.
#[test]
fn test_error_rsort_wrong_args() {
    expect_error("<?php rsort();", "rsort() takes exactly 1 argument");
}

/// Verifies that error isset wrong args.
#[test]
fn test_error_isset_wrong_args() {
    expect_error("<?php isset();", "isset() takes at least 1 argument");
}

/// Verifies a builtin's by-reference parameter refuses an argument with no storage.
///
/// `array_walk([1,2], "f")` RAN here — printing `1 2 reached`, exit 0 — where reference PHP
/// raises `Error: array_walk(): Argument #1 ($array) could not be passed by reference`. There
/// is nowhere to write the modified array back to. The guard is one authority over every
/// builtin that declares a by-reference parameter, not a per-builtin check: several builtins
/// hand-rolled it, and the ones nobody wrote it for accepted silently.
#[test]
fn test_error_by_ref_builtin_parameter_refuses_a_value_with_no_storage() {
    for (source, message) in [
        (
            r#"<?php function f($v) {} array_walk([1, 2], "f");"#,
            "array_walk(): Argument #1 ($array) could not be passed by reference",
        ),
        (
            "<?php sort([3, 1, 2]);",
            "sort(): Argument #1 ($array) could not be passed by reference",
        ),
        (
            "<?php array_push([1], 2);",
            "array_push(): Argument #1 ($array) could not be passed by reference",
        ),
    ] {
        expect_error(source, message);
    }
}

/// Verifies that error array unique wrong args.
#[test]
fn test_error_array_unique_wrong_args() {
    expect_error(
        "<?php array_unique();",
        "array_unique() takes exactly 1 argument",
    );
}

/// Verifies that error array product wrong args.
#[test]
fn test_error_array_product_wrong_args() {
    expect_error(
        "<?php array_product();",
        "array_product() takes exactly 1 argument",
    );
}

/// Verifies that error array shift wrong args.
#[test]
fn test_error_array_shift_wrong_args() {
    expect_error(
        "<?php array_shift();",
        "array_shift() takes exactly 1 argument",
    );
}

/// Verifies that error array unshift wrong args.
///
/// `array_unshift(array &$array, mixed ...$values)` is variadic, so PHP's own minimum is one
/// argument (`ArgumentCountError: array_unshift() expects at least 1 argument, 0 given`).
#[test]
fn test_error_array_unshift_wrong_args() {
    expect_error(
        "<?php array_unshift();",
        "array_unshift() takes at least 1 argument",
    );
}

/// Verifies `array_splice()` rejects both ends of its PHP 8.4 arity range: `$array`/`$offset`
/// are required and `$replacement` is the last accepted argument.
#[test]
fn test_error_array_splice_wrong_args() {
    expect_error(
        "<?php array_splice();",
        "array_splice() takes 2 to 4 arguments",
    );
    expect_error(
        "<?php $a = [1]; array_splice($a, 0, 0, [], 5);",
        "array_splice() takes 2 to 4 arguments",
    );
}

/// Verifies that error array flip wrong args.
#[test]
fn test_error_array_flip_wrong_args() {
    expect_error(
        "<?php array_flip();",
        "array_flip() takes exactly 1 argument",
    );
}

/// Verifies that error array chunk wrong args.
#[test]
fn test_error_array_chunk_no_args() {
    expect_error(
        "<?php array_chunk();",
        "array_chunk() takes 2 or 3 arguments",
    );
}

/// Verifies that error array pad wrong args.
#[test]
fn test_error_array_pad_wrong_args() {
    expect_error(
        "<?php array_pad();",
        "array_pad() takes exactly 3 arguments",
    );
}

/// Verifies that error array fill keys wrong args.
#[test]
fn test_error_array_fill_keys_wrong_args() {
    expect_error(
        "<?php array_fill_keys();",
        "array_fill_keys() takes exactly 2 arguments",
    );
}

/// Verifies that `count()` with no argument is rejected, naming the arity it now
/// accepts: PHP's optional `$mode` (`COUNT_RECURSIVE`) makes the second argument
/// legal, so the diagnostic reads "1 or 2" rather than "exactly 1".
#[test]
fn test_error_count_wrong_args() {
    expect_error("<?php count();", "count() takes 1 or 2 arguments");
}

/// Verifies that error array diff wrong args.
#[test]
fn test_error_array_diff_wrong_args() {
    expect_error(
        "<?php array_diff();",
        "array_diff() takes exactly 2 arguments",
    );
}

/// Verifies that error array intersect wrong args.
#[test]
fn test_error_array_intersect_wrong_args() {
    expect_error(
        "<?php array_intersect();",
        "array_intersect() takes exactly 2 arguments",
    );
}

/// Verifies that error array diff key wrong args.
#[test]
fn test_error_array_diff_key_wrong_args() {
    expect_error(
        "<?php array_diff_key();",
        "array_diff_key() takes exactly 2 arguments",
    );
}

/// Verifies that error array intersect key wrong args.
#[test]
fn test_error_array_intersect_key_wrong_args() {
    expect_error(
        "<?php array_intersect_key();",
        "array_intersect_key() takes exactly 2 arguments",
    );
}

/// Verifies that error array rand wrong args.
#[test]
fn test_error_array_rand_wrong_args() {
    expect_error(
        "<?php array_rand();",
        "array_rand() takes exactly 1 argument",
    );
}

/// Verifies that error asort wrong args.
#[test]
fn test_error_asort_wrong_args() {
    expect_error("<?php asort();", "asort() takes exactly 1 argument");
}

/// Verifies that error arsort wrong args.
#[test]
fn test_error_arsort_wrong_args() {
    expect_error("<?php arsort();", "arsort() takes exactly 1 argument");
}

/// Verifies that error ksort wrong args.
#[test]
fn test_error_ksort_wrong_args() {
    expect_error("<?php ksort();", "ksort() takes exactly 1 argument");
}

/// Verifies that error krsort wrong args.
#[test]
fn test_error_krsort_wrong_args() {
    expect_error("<?php krsort();", "krsort() takes exactly 1 argument");
}

/// Verifies that error natsort wrong args.
#[test]
fn test_error_natsort_wrong_args() {
    expect_error("<?php natsort();", "natsort() takes exactly 1 argument");
}

/// Verifies that error natcasesort wrong args.
#[test]
fn test_error_natcasesort_wrong_args() {
    expect_error(
        "<?php natcasesort();",
        "natcasesort() takes exactly 1 argument",
    );
}

/// Verifies that error array column wrong args.
#[test]
fn test_error_array_column_wrong_args() {
    expect_error(
        r#"<?php array_column([]);"#,
        "array_column() takes exactly 2 arguments",
    );
}

/// Verifies that error array map wrong args.
#[test]
fn test_error_array_map_wrong_args() {
    expect_error(
        r#"<?php array_map("fn");"#,
        "array_map() takes exactly 2 arguments",
    );
}

/// Verifies that error array filter wrong args.
#[test]
fn test_error_array_filter_wrong_args() {
    expect_error(
        r#"<?php array_filter([]);"#,
        "array_filter() takes 2 or 3 arguments",
    );
}

/// Verifies that error array reduce wrong args.
#[test]
fn test_error_array_reduce_wrong_args() {
    expect_error(
        r#"<?php array_reduce([], "fn");"#,
        "array_reduce() takes exactly 3 arguments",
    );
}

/// Verifies that error array walk wrong args.
#[test]
fn test_error_array_walk_wrong_args() {
    expect_error(
        r#"<?php array_walk([]);"#,
        "array_walk() takes exactly 2 arguments",
    );
}

/// Verifies that error usort wrong args.
#[test]
fn test_error_usort_wrong_args() {
    expect_error(r#"<?php usort([]);"#, "usort() takes exactly 2 arguments");
}

/// Verifies that error uksort wrong args.
#[test]
fn test_error_uksort_wrong_args() {
    expect_error(r#"<?php uksort([]);"#, "uksort() takes exactly 2 arguments");
}

/// Verifies that error uasort wrong args.
#[test]
fn test_error_uasort_wrong_args() {
    expect_error(r#"<?php uasort([]);"#, "uasort() takes exactly 2 arguments");
}

/// Verifies that error usort first class callable wrong arity.
#[test]
fn test_error_usort_first_class_callable_wrong_arity() {
    expect_error(
        r#"<?php
class BadComparator {
    public function cmp($a) {
        return 0;
    }
}

$bad = new BadComparator();
$values = [2, 1];
usort($values, $bad->cmp(...));
"#,
        "Method BadComparator::cmp expects 1 arguments, got 2",
    );
}

/// Verifies that error list unpack non array.
#[test]
fn test_error_list_unpack_non_array() {
    expect_error("<?php [$a, $b] = 42;", "List unpacking requires an array");
}

/// Verifies list unpacking rejects a nullable array when no guard removes the null member.
#[test]
fn test_error_list_unpack_nullable_array_without_guard() {
    expect_error(
        "<?php function row(): ?array { return null; } $entry = row(); [$a, $b] = $entry;",
        "List unpacking requires an array",
    );
}

// --- call_user_func_array errors ---

/// Verifies that error call user func array wrong args.
#[test]
fn test_error_call_user_func_array_wrong_args() {
    expect_error(
        "<?php call_user_func_array(\"foo\");",
        "call_user_func_array() takes exactly 2 arguments",
    );
}

// --- v0.8 system function errors ---

/// Verifies that error spread non array.
#[test]
fn test_error_spread_non_array() {
    expect_error(
        "<?php $x = 5; $y = [...$x];",
        "Spread operator requires an array",
    );
}

/// Verifies that error static property array push requires array.
#[test]
fn test_error_static_property_array_push_requires_array() {
    expect_error(
        "<?php class Box { public static int $items = 1; } Box::$items[] = 2;",
        "Array push requires an array static property, got int",
    );
}

/// Verifies that indexed array unrelated object values widen to mixed.
#[test]
fn test_indexed_array_unrelated_object_values_widen_to_mixed() {
    assert!(
        check_source("<?php class Dog {} class Car {} $items = [new Dog(), new Car()];").is_ok(),
        "heterogeneous indexed-array values should widen to mixed",
    );
}

/// Verifies `array_map()` rejects object elements until its callback runtime supports them.
#[test]
fn test_error_array_map_rejects_object_elements() {
    expect_error(
        "<?php final class Box {} $items = [new Box()]; array_map(static fn(Box $box): Box => $box, $items);",
        "array_map() does not yet support object array elements",
    );
}

/// Verifies contextual callback checking still rejects declarations incompatible with known elements.
#[test]
fn test_error_array_callback_rejects_known_element_mismatch() {
    expect_error(
        "<?php array_map(static fn(string $value): string => $value, [1, 2]);",
        "array_map() callback parameter $value expects Str, got Int",
    );
}

/// Verifies that error call user func array ref callback param requires variable.
#[test]
fn test_error_call_user_func_array_ref_callback_param_requires_variable() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } call_user_func_array(\"bump\", [1]);",
        "parameter $n must be passed a variable",
    );
}

/// Verifies that array_is_list() with no arguments reports an arity error.
#[test]
fn test_error_array_is_list_wrong_args() {
    expect_error(
        "<?php array_is_list();",
        "array_is_list() takes exactly 1 argument",
    );
}

/// Verifies that array_is_list() rejects a non-array argument.
#[test]
fn test_error_array_is_list_non_array() {
    expect_error(
        "<?php array_is_list(5);",
        "array_is_list() argument must be array",
    );
}

/// Verifies that array_key_first() with no arguments reports an arity error.
#[test]
fn test_error_array_key_first_wrong_args() {
    expect_error(
        "<?php array_key_first();",
        "array_key_first() takes exactly 1 argument",
    );
}

/// Verifies that array_key_last() rejects a non-array argument.
#[test]
fn test_error_array_key_last_non_array() {
    expect_error(
        "<?php array_key_last(\"x\");",
        "array_key_last() argument must be array",
    );
}

/// Verifies that array_replace() with a single argument reports an arity error.
#[test]
fn test_error_array_replace_wrong_args() {
    expect_error(
        "<?php $a = [\"k\" => 1]; array_replace($a);",
        "array_replace() takes exactly 2 arguments",
    );
}

/// Verifies that array_replace() rejects string-element indexed arrays (scalar indexed inputs
/// are supported; string/heap element indexed inputs are a follow-up).
#[test]
fn test_error_array_replace_string_indexed_unsupported() {
    expect_error(
        "<?php array_replace([\"a\", \"b\"], [\"c\"]);",
        "array_replace() arguments must be associative arrays or indexed arrays of scalars",
    );
}

/// Verifies that array_replace_recursive() with a single argument reports an arity error.
#[test]
fn test_error_array_replace_recursive_wrong_args() {
    expect_error(
        "<?php $a = [\"k\" => 1]; array_replace_recursive($a);",
        "array_replace_recursive() takes exactly 2 arguments",
    );
}

/// Verifies that array_replace_recursive() rejects string-element indexed arrays (scalar indexed
/// inputs are supported; string/heap element indexed inputs are a follow-up).
#[test]
fn test_error_array_replace_recursive_string_indexed_unsupported() {
    expect_error(
        "<?php array_replace_recursive([\"a\"], [\"b\"]);",
        "array_replace_recursive() arguments must be associative arrays or indexed arrays of scalars",
    );
}

/// Verifies that array_diff_assoc() with a single argument reports an arity error.
#[test]
fn test_error_array_diff_assoc_wrong_args() {
    expect_error(
        "<?php $a = [\"k\" => 1]; array_diff_assoc($a);",
        "array_diff_assoc() takes exactly 2 arguments",
    );
}

/// Verifies that array_intersect_assoc() rejects string-element indexed arrays (scalar indexed
/// inputs are supported; string/heap element indexed inputs are a follow-up).
#[test]
fn test_error_array_intersect_assoc_string_indexed_unsupported() {
    expect_error(
        "<?php array_intersect_assoc([\"a\", \"b\"], [\"a\"]);",
        "array_intersect_assoc() arguments must be associative arrays or indexed arrays of scalars",
    );
}

/// Verifies that array_merge_recursive() with a single argument reports an arity error.
#[test]
fn test_error_array_merge_recursive_wrong_args() {
    expect_error(
        "<?php $a = [\"k\" => 1]; array_merge_recursive($a);",
        "array_merge_recursive() takes exactly 2 arguments",
    );
}

/// Verifies that array_merge_recursive() rejects string-element indexed arrays (scalar indexed
/// inputs are supported; string/heap element indexed inputs are a follow-up).
#[test]
fn test_error_array_merge_recursive_string_indexed_unsupported() {
    expect_error(
        "<?php array_merge_recursive([\"a\"], [\"b\"]);",
        "array_merge_recursive() arguments must be associative arrays or indexed arrays of scalars",
    );
}

/// Verifies that array_find() with a single argument reports an arity error.
#[test]
fn test_error_array_find_wrong_args() {
    expect_error(
        "<?php function f($x) { return true; } array_find([1, 2]);",
        "array_find() takes exactly 2 arguments",
    );
}

/// Verifies that array_any() with a single argument reports an arity error.
#[test]
fn test_error_array_any_wrong_args() {
    expect_error(
        "<?php function f($x) { return true; } array_any([1, 2]);",
        "array_any() takes exactly 2 arguments",
    );
}

/// Verifies that array_all() rejects a non-array first argument.
#[test]
fn test_error_array_all_non_array() {
    expect_error(
        "<?php function f($x) { return true; } array_all(5, \"f\");",
        "array_all() first argument must be array",
    );
}

/// Verifies that array_walk_recursive() with a single argument reports an arity error.
#[test]
fn test_error_array_walk_recursive_wrong_args() {
    expect_error(
        "<?php function f($x) {} $a = [[1]]; array_walk_recursive($a);",
        "array_walk_recursive() takes exactly 2 arguments",
    );
}

/// Verifies that array_udiff() with two arguments reports an arity error.
#[test]
fn test_error_array_udiff_wrong_args() {
    expect_error(
        "<?php function c($a, $b) { return 0; } array_udiff([1], [2]);",
        "array_udiff() takes exactly 3 arguments",
    );
}

/// Verifies that array_uintersect() rejects a non-array first argument.
#[test]
fn test_error_array_uintersect_non_array() {
    expect_error(
        "<?php function c($a, $b) { return 0; } array_uintersect(5, [2], \"c\");",
        "array_uintersect() first argument must be array",
    );
}

/// Verifies that array_multisort() with a single argument reports an arity error.
#[test]
fn test_error_array_multisort_wrong_args() {
    expect_error(
        "<?php $a = [1, 2]; array_multisort($a);",
        "array_multisort() takes exactly 2 arguments",
    );
}

/// Verifies that `array_multisort()` rejects a literal in one of its by-reference array
/// positions, with php-src's own message.
///
/// Reference PHP ACCEPTS this exact call: `array_multisort($a, 5)` reads the `5` as a sort
/// flag, which those positions also allow. elephc does not implement the scalar flag
/// arguments, so it refuses — as it did before this message changed, under a wording of its
/// own invention. The refusal is the pre-existing gap; only the diagnostic moved.
#[test]
fn test_error_array_multisort_non_array() {
    expect_error(
        "<?php $a = [1, 2]; array_multisort($a, 5);",
        "array_multisort(): Argument #2 ($array2) could not be passed by reference",
    );
}

/// Verifies that an untyped closure/arrow-function parameter passed as an array builtin's
/// callback inherits the array's ELEMENT type instead of staying `Mixed`, so a string-only
/// builtin call in the body type-checks. Covers every builtin that types its callback
/// contextually; runtime behavior is covered by codegen tests where the backend supports it.
#[test]
fn test_array_callback_untyped_parameters_inherit_element_type() {
    expect_no_error(
        r#"<?php
$w = ["banana", "apple"];
usort($w, fn($a, $b) => strlen($a) <=> strlen($b));
"#,
    );
    expect_no_error(
        r#"<?php
$w = ["k" => "banana", "j" => "apple"];
uasort($w, fn($a, $b) => strlen($a) <=> strlen($b));
"#,
    );
    expect_no_error(
        r#"<?php
$w = ["banana", "apple"];
$r = array_filter($w, fn($v) => strlen($v) > 3);
"#,
    );
    expect_no_error(
        r#"<?php
$w = ["banana", "apple"];
$r = array_map(fn($v) => strtoupper($v), $w);
"#,
    );
    expect_no_error(
        r#"<?php
$w = ["banana", "apple"];
array_walk($w, function ($v) { echo strlen($v); });
"#,
    );
    expect_no_error(
        r#"<?php
$w = ["banana", "apple"];
$r = array_reduce($w, fn($c, $v) => $c + strlen($v), 0);
"#,
    );
}

/// Verifies that `uksort()` types its comparator parameters from the array's KEY type, so a
/// string-keyed array gives an untyped comparator two `string` parameters instead of `int`.
#[test]
fn test_uksort_untyped_parameters_inherit_key_type() {
    expect_no_error(
        r#"<?php
$w = ["banana" => 1, "fig" => 2];
uksort($w, fn($a, $b) => strlen($a) <=> strlen($b));
"#,
    );
}

/// Verifies that `array_walk()` also types the optional second callback parameter from the
/// array's key type, while a single-parameter callback still passes arity validation.
#[test]
fn test_array_walk_callback_second_parameter_inherits_key_type() {
    expect_no_error(
        r#"<?php
$w = ["banana" => 1, "fig" => 2];
array_walk($w, function ($v, $k) { echo strlen($k), $v; });
"#,
    );
    expect_no_error(
        r#"<?php
$w = ["banana", "apple"];
array_walk($w, function ($v) { echo strlen($v); });
"#,
    );
}

/// Verifies contextual callback typing does not silence real errors: an integer element type
/// still rejects a string-only builtin applied to the inherited parameter.
#[test]
fn test_array_callback_contextual_typing_still_rejects_wrong_element_use() {
    expect_error(
        r#"<?php
$w = [3, 1, 2];
usort($w, fn($a, $b) => strlen($a) <=> strlen($b));
"#,
        "strlen() argument must be string",
    );
}

/// Verifies `array_count_values()` rejects a missing argument.
#[test]
fn test_error_array_count_values_wrong_args() {
    expect_error(
        "<?php array_count_values();",
        "array_count_values() takes exactly 1 argument",
    );
}

/// Verifies `array_count_values()` rejects excess positional arguments.
#[test]
fn test_error_array_count_values_too_many_args() {
    expect_error(
        "<?php echo array_count_values([1], [2]);",
        "array_count_values() takes exactly 1 argument",
    );
}

/// Verifies `array_count_values()` rejects a non-array argument.
#[test]
fn test_error_array_count_values_wrong_type() {
    expect_error(
        "<?php echo array_count_values(\"x\");",
        "array_count_values() argument must be array",
    );
}

// --- internal array pointer family (key/current/next/prev/reset/end) ---

/// Verifies each internal-array-pointer builtin reports PHP's exact-one-argument arity.
///
/// PHP raises `ArgumentCountError` at run time; elephc is ahead of the program and
/// rejects the call at compile time with the registry's shared arity phrasing.
#[test]
fn test_error_array_pointer_wrong_arg_count() {
    for name in ["key", "current", "next", "prev", "reset", "end"] {
        expect_error(
            &format!("<?php {}();", name),
            &format!("{}() takes exactly 1 argument", name),
        );
        expect_error(
            &format!("<?php $a = [1, 2]; {}($a, 1);", name),
            &format!("{}() takes exactly 1 argument", name),
        );
    }
}

/// Verifies a non-array receiver is rejected, mirroring PHP's `TypeError`.
/// Fixture: a string local passed to each member of the family.
#[test]
fn test_error_array_pointer_non_array_receiver() {
    for name in ["key", "current", "next", "prev", "reset", "end"] {
        expect_error(
            &format!("<?php $s = \"str\"; {}($s);", name),
            &format!("{}() argument must be array", name),
        );
    }
}

/// Verifies an object-property receiver is a named compile error rather than a silently
/// detached cursor.
///
/// elephc keeps the internal pointer in a hidden slot beside the array LOCAL, so a
/// property has nowhere to store one. PHP accepts this shape, so the divergence is
/// deliberate and must stay loud.
#[test]
fn test_error_array_pointer_property_receiver() {
    expect_error(
        r#"<?php class C { public array $p = [1, 2]; } $o = new C(); echo key($o->p);"#,
        "key() argument must be an array variable",
    );
}

/// Verifies an array-element receiver is a named compile error for the same reason.
/// Fixture: `next($a[0])` on a nested indexed array.
#[test]
fn test_error_array_pointer_element_receiver() {
    expect_error(
        r#"<?php $a = [[1, 2], [3, 4]]; next($a[0]);"#,
        "next() argument must be an array variable",
    );
}

/// Verifies a call-result receiver is a named compile error for the same reason.
/// Fixture: `current(f())` where `f()` returns a fresh array.
#[test]
fn test_error_array_pointer_call_result_receiver() {
    expect_error(
        r#"<?php function f(): array { return [1, 2]; } echo current(f());"#,
        "current() argument must be an array variable",
    );
}

/// Verifies an array literal receiver is a named compile error for the same reason.
/// Fixture: `reset([1, 2, 3])`, which PHP itself also rejects for the by-reference members.
#[test]
fn test_error_array_pointer_literal_receiver() {
    expect_error(
        r#"<?php reset([1, 2, 3]);"#,
        "reset(): Argument #1 ($array) could not be passed by reference",
    );
}
