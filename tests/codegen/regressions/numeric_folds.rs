//! Purpose:
//! Regression tests for `array_sum()` and `array_product()` over lists of anything but ints.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The boxed-`mixed` runtime folds only add integers: a float in a mixed list was truncated,
//!   a hash summed to nothing, a float list and an untyped argument were refused at compile time.
//!   Such operands now fold through `crate::array_sum_prelude`, and the result is typed `mixed`
//!   when it can be either an int or a float.

use crate::support::*;

/// Sums and products keep php's int-or-float result for every operand shape.
#[test]
fn test_array_sum_and_product_over_mixed_floats_hashes_and_untyped_operands() {
    let out = compile_and_run_with_heap_debug(
        r#"<?php
function s1() { return array_sum([1, 1.5]); }
function s2() { $a = [1, 2]; $a[] = 2.5; return array_sum($a); }
function s3() { $a = ["x" => 1, "y" => 0.5]; return array_sum($a); }
function g($r) { return array_sum($r); }
function s4() { return g([1, 2]) . "/" . g([1.5, 1]); }
function s5() { return implode(",", array_map(fn($r) => array_sum($r), [[1, 2], [3]])); }
function s6() { $a = [1, "2", 3.5]; return array_sum($a); }
function s7() { return array_sum([1, 2, 3]) . "/" . array_sum([0.5, 0.25]) . "/" . array_sum([]); }
function s8() { return array_product([2, 1.5]) . "/" . array_product(["a" => 2, "b" => 3]) . "/" . array_product([]); }
function s9() { $t = array_sum([1, 2.5]); return gettype($t) . gettype(array_sum([1, 2])) . gettype(array_sum(["a" => 1])); }
echo s1(), "|", s2(), "|", s3(), "|", s4(), "|", s5(), "|", s6(), "|", s7(), "|", s8(), "|", s9(), "\n";
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "2.5|5.5|1.5|3/2.5|3,3|6.5|6/0.75/0|3/6/1|doubleintegerinteger\n"
    );
    assert!(
        out.stderr.contains("HEAP DEBUG: leak summary: clean"),
        "expected a clean heap, got: {}",
        out.stderr
    );
}
