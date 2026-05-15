//! Purpose:
//! Integration or regression tests for diagnostic coverage of array builtins, including array mixed type checks, array union operand checks, and indexed array union compatible element types.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Invalid PHP snippets are checked through shared diagnostic helpers for messages, spans, and recovery behavior.

use super::*;

#[test]
fn test_assoc_array_mixed_type_checks() {
    assert!(
        check_source(r#"<?php $a = ["name" => "Alice", "age" => 30];"#).is_ok(),
        "heterogeneous associative-array values should widen to mixed",
    );
}

#[test]
fn test_error_array_union_requires_array_operands() {
    expect_error(
        r#"<?php $result = [1, 2] + 3;"#,
        "Array union requires both operands to be arrays",
    );
}

#[test]
fn test_error_indexed_array_union_requires_compatible_element_types() {
    expect_error(
        r#"<?php $result = [1] + ["right", "side"];"#,
        "Array union requires compatible indexed array element types",
    );
}

// --- v0.6: array function argument errors ---

#[test]
fn test_error_array_reverse_wrong_args() {
    expect_error(
        "<?php array_reverse();",
        "array_reverse() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_merge_wrong_args() {
    expect_error(
        "<?php $a = [1]; array_merge($a);",
        "array_merge() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_sum_wrong_args() {
    expect_error("<?php array_sum();", "array_sum() takes exactly 1 argument");
}

#[test]
fn test_error_array_search_wrong_args() {
    expect_error(
        "<?php $a = [1]; array_search($a);",
        "array_search() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_key_exists_wrong_args() {
    expect_error(
        "<?php array_key_exists(1);",
        "array_key_exists() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_slice_wrong_args() {
    expect_error(
        "<?php $a = [1]; array_slice($a);",
        "array_slice() takes 2 or 3 arguments",
    );
}

#[test]
fn test_error_array_combine_wrong_args() {
    expect_error(
        "<?php $a = [1]; array_combine($a);",
        "array_combine() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_range_wrong_args() {
    expect_error("<?php range(1);", "range() takes exactly 2 arguments");
}

#[test]
fn test_error_shuffle_wrong_args() {
    expect_error("<?php shuffle();", "shuffle() takes exactly 1 argument");
}

#[test]
fn test_error_array_fill_wrong_args() {
    expect_error(
        "<?php array_fill(0, 5);",
        "array_fill() takes exactly 3 arguments",
    );
}

#[test]
fn test_error_array_push_wrong_args() {
    expect_error(
        "<?php array_push();",
        "array_push() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_pop_wrong_args() {
    expect_error("<?php array_pop();", "array_pop() takes exactly 1 argument");
}

#[test]
fn test_error_in_array_wrong_args() {
    expect_error("<?php in_array(1);", "in_array() takes exactly 2 arguments");
}

#[test]
fn test_error_array_keys_wrong_args() {
    expect_error(
        "<?php array_keys();",
        "array_keys() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_values_wrong_args() {
    expect_error(
        "<?php array_values();",
        "array_values() takes exactly 1 argument",
    );
}

#[test]
fn test_error_sort_wrong_args() {
    expect_error("<?php sort();", "sort() takes exactly 1 argument");
}

#[test]
fn test_error_rsort_wrong_args() {
    expect_error("<?php rsort();", "rsort() takes exactly 1 argument");
}

#[test]
fn test_error_isset_wrong_args() {
    expect_error("<?php isset();", "isset() takes exactly 1 argument");
}

#[test]
fn test_error_array_unique_wrong_args() {
    expect_error(
        "<?php array_unique();",
        "array_unique() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_product_wrong_args() {
    expect_error(
        "<?php array_product();",
        "array_product() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_shift_wrong_args() {
    expect_error(
        "<?php array_shift();",
        "array_shift() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_unshift_wrong_args() {
    expect_error(
        "<?php array_unshift();",
        "array_unshift() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_splice_wrong_args() {
    expect_error(
        "<?php array_splice();",
        "array_splice() takes 2 or 3 arguments",
    );
}

#[test]
fn test_error_array_flip_wrong_args() {
    expect_error(
        "<?php array_flip();",
        "array_flip() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_chunk_wrong_args() {
    expect_error(
        "<?php array_chunk();",
        "array_chunk() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_pad_wrong_args() {
    expect_error(
        "<?php array_pad();",
        "array_pad() takes exactly 3 arguments",
    );
}

#[test]
fn test_error_array_fill_keys_wrong_args() {
    expect_error(
        "<?php array_fill_keys();",
        "array_fill_keys() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_count_wrong_args() {
    expect_error("<?php count();", "count() takes exactly 1 argument");
}

#[test]
fn test_error_array_diff_wrong_args() {
    expect_error(
        "<?php array_diff();",
        "array_diff() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_intersect_wrong_args() {
    expect_error(
        "<?php array_intersect();",
        "array_intersect() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_diff_key_wrong_args() {
    expect_error(
        "<?php array_diff_key();",
        "array_diff_key() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_intersect_key_wrong_args() {
    expect_error(
        "<?php array_intersect_key();",
        "array_intersect_key() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_rand_wrong_args() {
    expect_error(
        "<?php array_rand();",
        "array_rand() takes exactly 1 argument",
    );
}

#[test]
fn test_error_asort_wrong_args() {
    expect_error("<?php asort();", "asort() takes exactly 1 argument");
}

#[test]
fn test_error_arsort_wrong_args() {
    expect_error("<?php arsort();", "arsort() takes exactly 1 argument");
}

#[test]
fn test_error_ksort_wrong_args() {
    expect_error("<?php ksort();", "ksort() takes exactly 1 argument");
}

#[test]
fn test_error_krsort_wrong_args() {
    expect_error("<?php krsort();", "krsort() takes exactly 1 argument");
}

#[test]
fn test_error_natsort_wrong_args() {
    expect_error("<?php natsort();", "natsort() takes exactly 1 argument");
}

#[test]
fn test_error_natcasesort_wrong_args() {
    expect_error(
        "<?php natcasesort();",
        "natcasesort() takes exactly 1 argument",
    );
}

#[test]
fn test_error_array_column_wrong_args() {
    expect_error(
        r#"<?php array_column([]);"#,
        "array_column() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_map_wrong_args() {
    expect_error(
        r#"<?php array_map("fn");"#,
        "array_map() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_filter_wrong_args() {
    expect_error(
        r#"<?php array_filter([]);"#,
        "array_filter() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_array_reduce_wrong_args() {
    expect_error(
        r#"<?php array_reduce([], "fn");"#,
        "array_reduce() takes exactly 3 arguments",
    );
}

#[test]
fn test_error_array_walk_wrong_args() {
    expect_error(
        r#"<?php array_walk([]);"#,
        "array_walk() takes exactly 2 arguments",
    );
}

#[test]
fn test_error_usort_wrong_args() {
    expect_error(r#"<?php usort([]);"#, "usort() takes exactly 2 arguments");
}

#[test]
fn test_error_uksort_wrong_args() {
    expect_error(r#"<?php uksort([]);"#, "uksort() takes exactly 2 arguments");
}

#[test]
fn test_error_uasort_wrong_args() {
    expect_error(r#"<?php uasort([]);"#, "uasort() takes exactly 2 arguments");
}

#[test]
fn test_error_list_unpack_non_array() {
    expect_error("<?php [$a, $b] = 42;", "List unpacking requires an array");
}

// --- call_user_func_array errors ---

#[test]
fn test_error_call_user_func_array_wrong_args() {
    expect_error(
        "<?php call_user_func_array(\"foo\");",
        "call_user_func_array() takes exactly 2 arguments",
    );
}

// --- v0.8 system function errors ---

#[test]
fn test_error_spread_non_array() {
    expect_error(
        "<?php $x = 5; $y = [...$x];",
        "Spread operator requires an array",
    );
}

#[test]
fn test_error_static_property_array_push_requires_array() {
    expect_error(
        "<?php class Box { public static int $items = 1; } Box::$items[] = 2;",
        "Array push requires an array static property, got int",
    );
}

#[test]
fn test_indexed_array_unrelated_object_values_widen_to_mixed() {
    assert!(
        check_source("<?php class Dog {} class Car {} $items = [new Dog(), new Car()];").is_ok(),
        "heterogeneous indexed-array values should widen to mixed",
    );
}

#[test]
fn test_error_call_user_func_array_rejects_ref_callback_params() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } $f = bump(...); $value = 1; call_user_func_array($f, [$value]);",
        "does not support pass-by-reference callback parameters yet",
    );
}

#[test]
fn test_error_call_user_func_array_string_literal_rejects_ref_callback_params() {
    expect_error(
        "<?php function bump(&$n) { $n = $n + 1; } $value = 1; call_user_func_array(\"bump\", [$value]);",
        "does not support pass-by-reference callback parameters yet",
    );
}
